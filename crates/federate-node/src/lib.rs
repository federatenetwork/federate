//! federate-node — generic node runtime shared by every Federate daemon.
//!
//! Handles: config file, node identity, signed registration with the node
//! directory, periodic re-registration (heartbeat), and the standard health
//! API every node must expose (`/health`, `/status`, `/roles`).

use federate_core::Result;
use federate_directory::{DirectoryClient, NodeCapacity, NodeRegistration, NodeRole};
use federate_identity::NodeIdentity;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const NODE_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Node config file (TOML):
///
/// ```toml
/// [node]
/// roles = ["gateway", "cdn"]
/// region = "br-sp"
/// public_ip = "x.x.x.x"
///
/// [network]
/// bootstrap = "https://federate.network"
/// root_key = "..."
///
/// [capacity]
/// storage_gb = 100
/// bandwidth_mbps = 500
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node: NodeSection,
    pub network: NetworkSection,
    #[serde(default)]
    pub capacity: NodeCapacity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSection {
    pub roles: Vec<NodeRole>,
    pub region: String,
    pub public_ip: String,
    /// HTTP service listen address (gateway/storage/mirror/health routes).
    #[serde(default = "default_http_listen")]
    pub listen: String,
    /// UDP DNS listen address (only used with the dns role).
    #[serde(default = "default_dns_listen")]
    pub dns_listen: String,
    /// Data/cache directory. Defaults to the OS data dir + "federate-node".
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
}

fn default_http_listen() -> String {
    "0.0.0.0:8080".into()
}

fn default_dns_listen() -> String {
    "0.0.0.0:5353".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSection {
    pub bootstrap: String,
    /// Node directory URL. Defaults to the bootstrap URL (Node 1 hosts the
    /// official directory).
    #[serde(default)]
    pub directory: Option<String>,
    /// Pinned Federate Root public key (hex). Strongly recommended.
    #[serde(default)]
    pub root_key: Option<String>,
    /// Upstream DNS for non-Federate names (dns role only).
    #[serde(default = "default_upstream_dns")]
    pub upstream_dns: String,
}

fn default_upstream_dns() -> String {
    "1.1.1.1:53".into()
}

impl NodeConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| {
            federate_core::FederateError::InvalidRoot(format!(
                "invalid node config {}: {e}",
                path.display()
            ))
        })
    }

    pub fn directory_url(&self) -> &str {
        self.network
            .directory
            .as_deref()
            .unwrap_or(&self.network.bootstrap)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.node.data_dir.clone().unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("federate-node")
        })
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// Shared runtime for any node role: identity + signed registration +
/// heartbeat + health state.
pub struct NodeRuntime {
    pub config: NodeConfig,
    pub identity: NodeIdentity,
    pub started_at: String,
}

impl NodeRuntime {
    pub fn new(config: NodeConfig) -> Result<Arc<Self>> {
        let data_dir = config.data_dir();
        std::fs::create_dir_all(&data_dir)?;
        let identity = NodeIdentity::load_or_create(&data_dir)?;
        Ok(Arc::new(Self {
            config,
            identity,
            started_at: chrono::Utc::now().to_rfc3339(),
        }))
    }

    pub fn node_id(&self) -> String {
        self.identity.node_id()
    }

    /// Build a signed registration for the directory. `health_endpoint` is
    /// the public base URL of this node's health API.
    pub fn build_registration(&self) -> Result<NodeRegistration> {
        let port = self.config.node.listen.rsplit(':').next().unwrap_or("8080");
        let mut reg = NodeRegistration {
            node_id: self.node_id(),
            public_key: self.node_id(),
            roles: self.config.node.roles.clone(),
            public_ips: vec![self.config.node.public_ip.clone()],
            region: self.config.node.region.clone(),
            version: NODE_VERSION.into(),
            capacity: self.config.capacity.clone(),
            health_endpoint: format!("http://{}:{}", self.config.node.public_ip, port),
            registered_at: chrono::Utc::now().to_rfc3339(),
            signature_algorithm: "ed25519".into(),
            signature: None,
        };
        reg.signature = Some(self.identity.sign(&reg.signable_bytes()?));
        Ok(reg)
    }

    /// Register with the directory now, then re-register on an interval so
    /// the directory keeps seeing us alive (and survives directory restarts).
    pub async fn registration_loop(self: Arc<Self>, interval: std::time::Duration) {
        let client = DirectoryClient::new(self.config.directory_url());
        loop {
            match self.build_registration() {
                Ok(reg) => match client.register(&reg).await {
                    Ok(()) => tracing::debug!("registered with directory {}", client.base_url()),
                    Err(e) => tracing::warn!("directory registration failed: {e}"),
                },
                Err(e) => tracing::error!("cannot build registration: {e}"),
            }
            tokio::time::sleep(interval).await;
        }
    }

    /// Standard health API every node exposes: `/health`, `/status`, `/roles`.
    pub fn health_router(self: &Arc<Self>) -> axum::Router {
        use axum::extract::State;
        use axum::routing::get;
        use axum::Json;

        async fn status(State(rt): State<Arc<NodeRuntime>>) -> Json<serde_json::Value> {
            Json(serde_json::json!({
                "node_id": rt.node_id(),
                "roles": rt.config.node.roles,
                "region": rt.config.node.region,
                "version": NODE_VERSION,
                "started_at": rt.started_at,
                "capacity": rt.config.capacity,
                "bootstrap": rt.config.network.bootstrap,
                "directory": rt.config.directory_url(),
            }))
        }

        async fn roles(State(rt): State<Arc<NodeRuntime>>) -> Json<serde_json::Value> {
            Json(serde_json::json!({ "roles": rt.config.node.roles }))
        }

        axum::Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/status", get(status))
            .route("/roles", get(roles))
            .with_state(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_and_registration_signs() {
        let cfg: NodeConfig = toml::from_str(
            r#"
            [node]
            roles = ["gateway", "cdn"]
            region = "br-sp"
            public_ip = "45.1.1.1"

            [network]
            bootstrap = "https://federate.network"

            [capacity]
            storage_gb = 100
            bandwidth_mbps = 500
            "#,
        )
        .unwrap();
        assert_eq!(cfg.node.roles.len(), 2);
        assert_eq!(cfg.directory_url(), "https://federate.network");

        let mut cfg = cfg;
        cfg.node.data_dir =
            Some(std::env::temp_dir().join(format!("fed-node-test-{}", std::process::id())));
        let rt = NodeRuntime::new(cfg.clone()).unwrap();
        let reg = rt.build_registration().unwrap();
        assert!(reg.verify().is_ok());
        std::fs::remove_dir_all(cfg.data_dir()).ok();
    }
}
