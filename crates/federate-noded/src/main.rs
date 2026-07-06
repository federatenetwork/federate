//! federate-noded: generic Federate participant node.
//!
//! Runs one or more roles from a config file (or --roles override):
//! gateway, dns, storage, cdn, search, bootstrap, root-mirror.
//!
//! One HTTP listener serves the role routes (health, blocks, root mirror,
//! search, bootstrap) with the gateway as the fallback handler; the dns role
//! adds a UDP listener.

use axum::extract::{Path as AxPath, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Json;
use clap::Parser;
use federate_client::NodeClient;
use federate_directory::{DirectoryClient, NodeRole};
use federate_dns::DnsServer;
use federate_node::{NodeConfig, NodeRuntime};
use federate_resolution::Resolver;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "federate-noded", about = "Federate Network multi-role node")]
struct Args {
    /// Node config file (TOML)
    #[arg(long, default_value = "federate.toml")]
    config: std::path::PathBuf,
    /// Override roles from the config, e.g. --roles gateway,dns,cdn
    #[arg(long, value_delimiter = ',')]
    roles: Option<Vec<NodeRole>>,
}

struct MirrorState {
    resolver: Arc<Resolver>,
}

struct BlockState {
    cache: Arc<federate_cdn::CdnCache>,
    resolver: Arc<Resolver>,
    /// CDN nodes fetch-on-miss; storage-only nodes serve what they hold.
    fetch_on_miss: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    let mut config = NodeConfig::load(&args.config)?;
    if let Some(roles) = args.roles {
        config.node.roles = roles;
    }
    if config.node.roles.contains(&NodeRole::RootAuthority) {
        return Err(
            "the root-authority role is reserved for the official Federate root (federate-server)"
                .into(),
        );
    }
    let roles = config.node.roles.clone();
    tracing::info!(
        "starting federate-noded with roles: {}",
        roles
            .iter()
            .map(|r| r.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let runtime = NodeRuntime::new(config.clone())?;
    tracing::info!("node id: {}", runtime.node_id());
    let data_dir = config.data_dir();
    let directory_url = config.directory_url().to_string();
    let directory = DirectoryClient::new(&directory_url);

    let resolver = Arc::new(
        Resolver::new(
            NodeClient::new(&config.network.bootstrap),
            &data_dir,
            config.network.root_key.clone(),
        )?
        .with_directory(directory.clone(), Some(config.node.region.clone())),
    );
    match resolver.refresh_root().await {
        Ok(zone) => tracing::info!("verified root zone v{}", zone.root_version),
        Err(e) => tracing::warn!("root zone not loaded yet: {e} (will retry)"),
    }

    // --- Assemble the HTTP service router by role ---
    let mut app = runtime.health_router();

    let storage = roles.contains(&NodeRole::Storage);
    let cdn = roles.contains(&NodeRole::Cdn);
    let mut block_cache: Option<Arc<federate_cdn::CdnCache>> = None;
    if storage || cdn {
        let max_bytes = config.capacity.storage_gb.max(1) * 1024 * 1024 * 1024;
        let cache = Arc::new(federate_cdn::CdnCache::new(
            &data_dir.join("cdn"),
            max_bytes,
        )?);
        block_cache = Some(cache.clone());
        let state = Arc::new(BlockState {
            cache: cache.clone(),
            resolver: resolver.clone(),
            fetch_on_miss: cdn,
        });
        app = app.route("/v1/block/:hash", get(serve_block).with_state(state));
        // Announce cached blocks so gateways can find us as a provider. The
        // announcement is signed with this node's identity.
        let announce_dir = directory.clone();
        let announce_rt = runtime.clone();
        tokio::spawn(async move {
            loop {
                let blocks = cache.cached_hashes();
                if !blocks.is_empty() {
                    if let Err(e) = announce_dir
                        .announce_blocks(&announce_rt.identity, blocks)
                        .await
                    {
                        tracing::debug!("block announce failed: {e}");
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
    }

    if roles.contains(&NodeRole::RootMirror) {
        let state = Arc::new(MirrorState {
            resolver: resolver.clone(),
        });
        app = app.route("/v1/root", get(serve_root_mirror).with_state(state));
        // Keep the mirrored (signature-verified) zone fresh.
        let mirror_resolver = resolver.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = mirror_resolver.refresh_root().await {
                    tracing::warn!("root mirror refresh failed: {e}");
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
    }

    if roles.contains(&NodeRole::Search) {
        let index = Arc::new(RwLock::new(federate_search::SearchIndex::default()));
        app = app.merge(federate_search::router(index.clone()));
        let index_resolver = resolver.clone();
        tokio::spawn(async move {
            loop {
                match federate_search::index_from_resolver(&index_resolver).await {
                    Ok(new_index) => *index.write().await = new_index,
                    Err(e) => tracing::warn!("search indexing failed: {e}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(600)).await;
            }
        });
    }

    if roles.contains(&NodeRole::Bootstrap) {
        let bootstrap_url = config.network.bootstrap.clone();
        app = app.route(
            "/v1/bootstrap",
            get(move || {
                let url = bootstrap_url.clone();
                async move {
                    // Relay the official bootstrap answer; clients verify the
                    // root zone signature themselves, so relaying is safe.
                    match federate_bootstrap_fetch(&url).await {
                        Some(v) => Json(v).into_response(),
                        None => StatusCode::BAD_GATEWAY.into_response(),
                    }
                }
            }),
        );
    }

    if roles.contains(&NodeRole::Gateway) || roles.contains(&NodeRole::Origin) {
        app = app.merge(federate_gateway::router(resolver.clone()));
    }

    // --- DNS role: UDP listener ---
    if roles.contains(&NodeRole::Dns) {
        let upstream: std::net::SocketAddr = config.network.upstream_dns.parse()?;
        let server = DnsServer::new(resolver.clone(), directory.clone(), upstream);
        let listen: std::net::SocketAddr = config.node.dns_listen.parse()?;
        tokio::spawn(async move {
            if let Err(e) = server.run(listen).await {
                tracing::error!("dns server exited: {e}");
            }
        });
    }

    // --- Native Federate protocol listener (every node speaks it) ---
    if !config.node.native_listen.is_empty() {
        let native_listen: std::net::SocketAddr = config.node.native_listen.parse()?;
        let service = Arc::new(NativeService {
            runtime: runtime.clone(),
            resolver: resolver.clone(),
            cache: block_cache.clone(),
            fetch_on_miss: cdn,
        });
        let listener = tokio::net::TcpListener::bind(native_listen).await?;
        tracing::info!("federate native protocol on fed-tcp://{native_listen}");
        tokio::spawn(federate_transport::serve(
            listener,
            service,
            format!("federate-noded/{}", federate_node::NODE_VERSION),
        ));
    }

    // --- Register with the directory + heartbeat ---
    tokio::spawn(
        runtime
            .clone()
            .registration_loop(std::time::Duration::from_secs(60)),
    );

    let listen: std::net::SocketAddr = config.node.listen.parse()?;
    let listener = tokio::net::TcpListener::bind(listen).await?;
    tracing::info!("federate-noded HTTP service on http://{listen}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Native-protocol face of this node: answers Federate protocol requests
/// from the same verified stores the HTTP compatibility routes use.
struct NativeService {
    runtime: Arc<NodeRuntime>,
    resolver: Arc<Resolver>,
    cache: Option<Arc<federate_cdn::CdnCache>>,
    fetch_on_miss: bool,
}

#[federate_transport::async_trait]
impl federate_transport::NodeService for NativeService {
    fn node_id(&self) -> String {
        self.runtime.node_id()
    }

    fn capabilities(&self) -> Vec<federate_protocol::Capability> {
        let mut caps = vec![federate_protocol::Capability::Root];
        if self.cache.is_some() {
            caps.push(federate_protocol::Capability::Blocks);
        }
        caps
    }

    async fn handle(&self, request: federate_protocol::Message) -> federate_protocol::Message {
        use federate_protocol::{ErrorCode, Message};
        match request {
            Message::GetRoot => match self.resolver.root().await {
                // Serve only the locally VERIFIED zone; receivers re-verify
                // its signature against their own pinned key anyway.
                Ok(zone) => match serde_json::to_vec(&*zone) {
                    Ok(zone_json) => Message::Root { zone_json },
                    Err(e) => err(ErrorCode::Unavailable, &e.to_string()),
                },
                Err(e) => err(
                    ErrorCode::Unavailable,
                    &format!("no verified root zone: {e}"),
                ),
            },
            Message::GetBlock { hash } => {
                if !federate_storage::is_valid_hash(&hash) {
                    return err(ErrorCode::BadRequest, "not a valid content address");
                }
                let Some(cache) = &self.cache else {
                    return err(ErrorCode::Unsupported, "this node does not serve blocks");
                };
                if let Ok(bytes) = cache.get(&hash) {
                    return Message::Block { hash, bytes };
                }
                if self.fetch_on_miss {
                    if let Ok(bytes) = self.resolver.fetch_and_cache_block(&hash).await {
                        cache.put(&hash, &bytes).ok();
                        return Message::Block { hash, bytes };
                    }
                }
                err(ErrorCode::NotFound, "block not held by this node")
            }
            Message::GetStatus => Message::Status {
                node_id: self.runtime.node_id(),
                roles: self
                    .runtime
                    .config
                    .node
                    .roles
                    .iter()
                    .map(|r| r.as_str().to_string())
                    .collect(),
                region: self.runtime.config.node.region.clone(),
                agent: format!("federate-noded/{}", federate_node::NODE_VERSION),
                root_version: self.resolver.root().await.ok().map(|z| z.root_version),
            },
            _ => err(
                ErrorCode::Unsupported,
                "this node answers GetRoot, GetBlock, and GetStatus",
            ),
        }
    }
}

fn err(code: federate_protocol::ErrorCode, detail: &str) -> federate_protocol::Message {
    federate_protocol::Message::Error {
        code,
        detail: detail.to_string(),
    }
}

async fn serve_block(
    State(s): State<Arc<BlockState>>,
    AxPath(hash): AxPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    // The URL is the content address of the bytes: responses are immutable,
    // so downstream caches may keep them forever.
    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream"),
        (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
    ];
    if let Ok(bytes) = s.cache.get(&hash) {
        return Ok((headers, bytes));
    }
    if s.fetch_on_miss {
        // CDN behavior: pull from upstream (hash-verified), cache, serve.
        if let Ok(bytes) = s.resolver.fetch_and_cache_block(&hash).await {
            s.cache.put(&hash, &bytes).ok();
            return Ok((headers, bytes));
        }
    }
    Err(StatusCode::NOT_FOUND)
}

async fn serve_root_mirror(
    State(s): State<Arc<MirrorState>>,
) -> Result<impl IntoResponse, StatusCode> {
    // Serve only the locally *verified* zone; a mirror can distribute but
    // never modify root data (clients re-verify the signature anyway).
    match s.resolver.root().await {
        Ok(zone) => Ok(Json(serde_json::to_value(&*zone).unwrap())),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

async fn federate_bootstrap_fetch(base: &str) -> Option<serde_json::Value> {
    reqwest_get_json(&format!("{}/v1/bootstrap", base.trim_end_matches('/'))).await
}

async fn reqwest_get_json(url: &str) -> Option<serde_json::Value> {
    // reuse the shared client crate's HTTP stack via a tiny helper
    federate_client::get_json(url).await.ok()
}
