//! federate-searchd — Federate search node.
//!
//! Indexes public Federate pages through the signature-verifying resolution
//! engine and exposes /v1/search. No ads, no tracking, no AI training;
//! opt-out (`<meta name="federate" content="noindex">`) is honored.

use clap::Parser;
use federate_client::NodeClient;
use federate_resolution::Resolver;
use federate_search::SearchIndex;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "federate-searchd", about = "Federate Network search node")]
struct Args {
    /// HTTP listen address
    #[arg(long, default_value = "0.0.0.0:8090")]
    listen: SocketAddr,
    /// Bootstrap / root zone source
    #[arg(long, default_value = federate_core::DEFAULT_BOOTSTRAP_URL)]
    bootstrap: String,
    /// Pinned Federate Root public key (hex)
    #[arg(long)]
    root_key: Option<String>,
    /// Data/cache directory
    #[arg(long, default_value = ".federate-searchd")]
    data_dir: std::path::PathBuf,
    /// Reindex interval in seconds
    #[arg(long, default_value = "600")]
    reindex_secs: u64,
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

    let resolver = Arc::new(Resolver::new(
        NodeClient::new(&args.bootstrap),
        &args.data_dir,
        args.root_key,
    )?);
    let index = Arc::new(RwLock::new(SearchIndex::default()));

    let bg_resolver = resolver.clone();
    let bg_index = index.clone();
    let interval = args.reindex_secs;
    tokio::spawn(async move {
        loop {
            match federate_search::index_from_resolver(&bg_resolver).await {
                Ok(new_index) => *bg_index.write().await = new_index,
                Err(e) => tracing::warn!("indexing failed: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        }
    });

    let app =
        federate_search::router(index).route("/health", axum::routing::get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!("federate-searchd listening on http://{}", args.listen);
    axum::serve(listener, app).await?;
    Ok(())
}
