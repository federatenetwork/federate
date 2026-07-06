//! federate-gatewayd — a public Federate gateway node anyone can run.
//!
//! Browsers reach this node via Federate DNS; it resolves Host headers
//! through the shared verification chain (root → TLD → domain → manifest →
//! blocks) and serves the content. Blocks come from CDN/storage/origin
//! providers via the directory, falling back to Node 1.

use clap::Parser;
use federate_client::NodeClient;
use federate_directory::{DirectoryClient, NodeCapacity, NodeRole};
use federate_node::{NetworkSection, NodeConfig, NodeRuntime, NodeSection};
use federate_resolution::Resolver;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "federate-gatewayd", about = "Federate Network gateway node")]
struct Args {
    /// HTTP listen address (production: 0.0.0.0:80 behind DNS)
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: SocketAddr,
    /// Bootstrap / root zone source
    #[arg(long, default_value = federate_core::DEFAULT_BOOTSTRAP_URL)]
    bootstrap: String,
    /// Node directory URL (defaults to the bootstrap URL)
    #[arg(long)]
    directory: Option<String>,
    /// Pinned Federate Root public key (hex). Strongly recommended.
    #[arg(long)]
    root_key: Option<String>,
    /// Data/cache directory
    #[arg(long, default_value = ".federate-gatewayd")]
    data_dir: std::path::PathBuf,
    /// Public IP to register in the node directory (enables registration)
    #[arg(long)]
    public_ip: Option<String>,
    /// Region label (e.g. br-sp)
    #[arg(long, default_value = "unknown")]
    region: String,
    /// Health API listen address
    #[arg(long, default_value = "0.0.0.0:8081")]
    health_listen: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    std::fs::create_dir_all(&args.data_dir)?;

    let directory_url = args
        .directory
        .clone()
        .unwrap_or_else(|| args.bootstrap.clone());
    let client = NodeClient::new(&args.bootstrap);
    let resolver = Arc::new(
        Resolver::new(client, &args.data_dir, args.root_key.clone())?.with_directory(
            DirectoryClient::new(&directory_url),
            Some(args.region.clone()),
        ),
    );
    match resolver.refresh_root().await {
        Ok(zone) => tracing::info!("verified root zone v{}", zone.root_version),
        Err(e) => tracing::warn!("root zone not loaded yet: {e} (will retry)"),
    }

    if let Some(public_ip) = args.public_ip.clone() {
        let config = NodeConfig {
            node: NodeSection {
                roles: vec![NodeRole::Gateway],
                region: args.region.clone(),
                public_ip,
                listen: args.health_listen.to_string(),
                dns_listen: String::new(),
                data_dir: Some(args.data_dir.clone()),
            },
            network: NetworkSection {
                bootstrap: args.bootstrap.clone(),
                directory: Some(directory_url.clone()),
                root_key: args.root_key.clone(),
                upstream_dns: federate_dns_default(),
            },
            capacity: NodeCapacity::default(),
        };
        let runtime = NodeRuntime::new(config)?;
        tracing::info!("node id: {}", runtime.node_id());
        let health = runtime.health_router();
        let health_listen = args.health_listen;
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(health_listen)
                .await
                .expect("bind health API");
            axum::serve(listener, health).await.expect("health API");
        });
        tokio::spawn(runtime.registration_loop(std::time::Duration::from_secs(60)));
    }

    federate_gateway::serve(resolver, args.listen).await?;
    Ok(())
}

fn federate_dns_default() -> String {
    "1.1.1.1:53".into()
}
