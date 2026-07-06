//! federate-bootstrap — how new nodes discover the network.
//!
//! A bootstrap node answers "who is out there": root mirrors, DNS nodes,
//! gateway nodes, directory nodes, other bootstrap nodes. It never decides
//! what is valid — that is the signed root zone's job.

use federate_core::{FederateError, Result};
use serde::{Deserialize, Serialize};

/// What `/v1/bootstrap` returns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BootstrapInfo {
    pub network: String,
    pub root_public_key: String,
    pub root_version: u64,
    /// URL where the signed root zone can be fetched (`/v1/root`).
    pub root_url: String,
    /// Base URLs of root mirrors serving signed root zone copies.
    #[serde(default)]
    pub root_mirrors: Vec<String>,
    /// host:port of DNS nodes.
    #[serde(default)]
    pub dns_nodes: Vec<String>,
    /// Base URLs of gateway nodes.
    #[serde(default)]
    pub gateway_nodes: Vec<String>,
    /// Base URLs of node directories.
    #[serde(default)]
    pub directory_nodes: Vec<String>,
    /// Base URLs of other bootstrap nodes.
    #[serde(default)]
    pub bootstrap_nodes: Vec<String>,
}

#[derive(Clone)]
pub struct BootstrapClient {
    http: reqwest::Client,
}

impl Default for BootstrapClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BootstrapClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
        }
    }

    pub async fn fetch(&self, base_url: &str) -> Result<BootstrapInfo> {
        let url = format!("{}/v1/bootstrap", base_url.trim_end_matches('/'));
        self.http
            .get(&url)
            .send()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))
    }

    /// Try bootstrap URLs in order (official first, then any known bootstrap
    /// nodes) and return the first reachable answer.
    pub async fn discover(&self, urls: &[String]) -> Result<BootstrapInfo> {
        for url in urls {
            if let Ok(info) = self.fetch(url).await {
                return Ok(info);
            }
        }
        Err(FederateError::Network(
            "no bootstrap node reachable".to_string(),
        ))
    }
}
