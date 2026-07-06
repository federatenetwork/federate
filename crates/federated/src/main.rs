//! federated: the local Federate Network daemon.
//!
//! Starts the browser gateway (127.0.0.1:80), the local API (127.0.0.1:7777),
//! the cache, and node identity. Future hooks: DNS resolver, peer discovery.

use axum::extract::{Query, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use clap::Parser;
use federate_client::NodeClient;
use federate_core::DaemonConfig;
use federate_identity::NodeIdentity;
use federate_resolution::{Resolved, Resolver};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "federated", about = "Federate Network local daemon")]
struct Args {
    /// Node 1 bootstrap URL
    #[arg(long, default_value = federate_core::DEFAULT_BOOTSTRAP_URL)]
    bootstrap: String,
    /// Browser gateway bind address (must be 127.0.0.1:80 for portless URLs)
    #[arg(long, default_value = federate_core::DEFAULT_GATEWAY_ADDR)]
    gateway_addr: SocketAddr,
    /// Local daemon API bind address
    #[arg(long, default_value = federate_core::DEFAULT_API_ADDR)]
    api_addr: SocketAddr,
    /// Data/cache directory (default: OS data dir /federate)
    #[arg(long)]
    data_dir: Option<std::path::PathBuf>,
    /// Federate Root public key (hex) to pin as trust anchor. When omitted,
    /// the key is pinned from the first verified root zone (TOFU) and stored
    /// in the data dir.
    #[arg(long)]
    root_key: Option<String>,
}

struct AppState {
    resolver: Arc<Resolver>,
    identity: NodeIdentity,
    config: DaemonConfig,
}

#[tokio::main]
async fn main() -> anyhow_lite::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    let config = DaemonConfig {
        bootstrap_url: args.bootstrap,
        gateway_addr: args.gateway_addr,
        api_addr: args.api_addr,
        data_dir: args.data_dir.unwrap_or_else(DaemonConfig::default_data_dir),
    };
    std::fs::create_dir_all(&config.data_dir)?;

    let identity = NodeIdentity::load_or_create(&config.data_dir)?;
    tracing::info!("node identity: {}", identity.node_id());

    let client = NodeClient::new(&config.bootstrap_url);
    let resolver = Arc::new(Resolver::new(client, &config.data_dir, args.root_key)?);

    match resolver.refresh_root().await {
        Ok(zone) => tracing::info!(
            "root zone v{} loaded: {} domains, {} TLDs",
            zone.root_version,
            zone.domains.len(),
            zone.tlds.len()
        ),
        Err(e) => tracing::warn!("could not load root zone yet: {e} (will retry on demand)"),
    }

    // Keep the verified root zone fresh: Node 1 re-signs the zone (and new
    // manifest hashes) whenever sites change or it restarts. Without this
    // loop the daemon would keep resolving against a stale in-memory zone
    // until its own restart. Rollback protection still rejects older zones.
    {
        let resolver = resolver.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                if let Err(e) = resolver.refresh_root().await {
                    tracing::debug!("root zone refresh failed: {e} (keeping cached zone)");
                }
            }
        });
    }

    let state = Arc::new(AppState {
        resolver: resolver.clone(),
        identity,
        config: config.clone(),
    });

    // Local daemon API for CLI / desktop app integration.
    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/status", get(api_status))
        .route("/resolve", get(api_resolve))
        .route("/root", get(api_root))
        .route("/cache/list", get(api_cache_list))
        .route("/cache/clear", delete(api_cache_clear))
        .with_state(state.clone());
    let api_addr = config.api_addr;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(api_addr)
            .await
            .expect("bind daemon API");
        tracing::info!("daemon API listening on http://{api_addr}");
        axum::serve(listener, api).await.expect("daemon API");
    });

    // Browser gateway; blocks until shutdown.
    federate_gateway::serve(resolver, config.gateway_addr).await?;
    Ok(())
}

async fn api_status(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let root = s.resolver.root().await.ok();
    let node_up = NodeClient::new(&s.config.bootstrap_url)
        .health()
        .await
        .unwrap_or(false);
    Json(serde_json::json!({
        "daemon": "running",
        "node_id": s.identity.node_id(),
        "bootstrap": s.config.bootstrap_url,
        "node1_reachable": node_up,
        "gateway": s.config.gateway_addr.to_string(),
        "root_version": root.as_ref().map(|r| r.root_version),
        "domains": root.as_ref().map(|r| r.domains.keys().cloned().collect::<Vec<_>>()),
        "cached_blocks": s.resolver.block_store().list().map(|l| l.len()).unwrap_or(0),
        "trusted_root_key": s.resolver.trusted_root_key().await,
    }))
}

async fn api_resolve(
    State(s): State<Arc<AppState>>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let domain = q.get("domain").cloned().unwrap_or_default();
    let path = q.get("path").cloned().unwrap_or_else(|| "/".into());
    match s.resolver.resolve(&domain, &path).await {
        Ok(Resolved::Content {
            domain,
            path,
            bytes,
            mime,
        }) => Json(serde_json::json!({
            "status": "ok", "domain": domain, "path": path,
            "mime": mime, "size": bytes.len(),
        })),
        Ok(other) => Json(serde_json::json!({ "status": format!("{other:?}") })),
        Err(e) => Json(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

async fn api_root(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    match s.resolver.root().await {
        Ok(zone) => Json(serde_json::to_value(&*zone).unwrap()),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn api_cache_list(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let blocks = s.resolver.block_store().list().unwrap_or_default();
    Json(serde_json::json!({
        "blocks": blocks.iter().map(|(h, size)| serde_json::json!({"hash": h, "size": size})).collect::<Vec<_>>(),
    }))
}

async fn api_cache_clear(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    match s.resolver.block_store().clear() {
        Ok(n) => Json(serde_json::json!({ "cleared": n })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// tiny local Result alias so we don't pull in anyhow
mod anyhow_lite {
    pub type Result = std::result::Result<(), Box<dyn std::error::Error>>;
}
