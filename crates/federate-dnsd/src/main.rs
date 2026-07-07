//! federate-dnsd: a Federate DNS node anyone can run.
//!
//! Answers Federate TLDs with multiple healthy gateway IPs from the node
//! directory (low TTL, never one hardcoded IP) and forwards everything else
//! to upstream DNS. Verifies the root zone signature before trusting any
//! TLD data, including data served by root mirrors.

use clap::Parser;
use federate_client::NodeClient;
use federate_directory::{DirectoryClient, NodeCapacity, NodeRole};
use federate_dns::DnsServer;
use federate_node::{NetworkSection, NodeConfig, NodeRuntime, NodeSection};
use federate_resolution::Resolver;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "federate-dnsd", about = "Federate Network DNS node")]
struct Args {
    /// UDP listen address (production: 0.0.0.0:53)
    #[arg(long, default_value = "0.0.0.0:5353")]
    listen: SocketAddr,
    /// Bootstrap / root zone source (Node 1 or a root mirror)
    #[arg(long, default_value = federate_core::DEFAULT_BOOTSTRAP_URL)]
    bootstrap: String,
    /// Node directory URL (defaults to the bootstrap URL)
    #[arg(long)]
    directory: Option<String>,
    /// Upstream DNS for non-Federate names
    #[arg(long, default_value = federate_dns::DEFAULT_UPSTREAM)]
    upstream: SocketAddr,
    /// Pinned Federate Root public key (hex). Strongly recommended.
    #[arg(long)]
    root_key: Option<String>,
    /// Data/cache directory
    #[arg(long, default_value = ".federate-dnsd")]
    data_dir: std::path::PathBuf,
    /// Public IP to register in the node directory (enables registration)
    #[arg(long)]
    public_ip: Option<String>,
    /// Region label for the directory (e.g. br-sp)
    #[arg(long, default_value = "unknown")]
    region: String,
    /// Health API listen address
    #[arg(long, default_value = "0.0.0.0:8053")]
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
    let resolver = Arc::new(Resolver::new(
        client,
        &args.data_dir,
        args.root_key.clone(),
    )?);
    match resolver.refresh_root().await {
        Ok(zone) => tracing::info!(
            "verified root zone v{}: {} TLDs",
            zone.root_version,
            zone.tlds.len()
        ),
        Err(e) => tracing::warn!("root zone not loaded yet: {e} (will retry)"),
    }

    // Register with the directory as a DNS node when a public IP is given.
    if let Some(public_ip) = args.public_ip.clone() {
        let config = NodeConfig {
            node: NodeSection {
                roles: vec![NodeRole::Dns],
                region: args.region.clone(),
                public_ip,
                listen: args.health_listen.to_string(),
                dns_listen: args.listen.to_string(),
                native_listen: String::new(),
                registry_files: Vec::new(),
                data_dir: Some(args.data_dir.clone()),
            },
            network: NetworkSection {
                bootstrap: args.bootstrap.clone(),
                directory: Some(directory_url.clone()),
                root_key: args.root_key.clone(),
                native_providers: Vec::new(),
                upstream_dns: args.upstream.to_string(),
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

    let server = DnsServer::new(
        resolver,
        DirectoryClient::new(&directory_url),
        args.upstream,
    );
    server.run(args.listen).await?;
    Ok(())
}
