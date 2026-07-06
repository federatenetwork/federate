//! federate-bootstrap: how new nodes discover the network.
//!
//! A bootstrap node answers "who is out there": root mirrors, DNS nodes,
//! gateway nodes, directory nodes, other bootstrap nodes. It never decides
//! what is valid; that is the signed root zone's job.

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
    /// TCP port of the answering node's native Federate protocol listener.
    /// Combined with the bootstrap host this is the first native peer a new
    /// client talks to; HTTP is only needed for this one discovery call.
    #[serde(default)]
    pub native_port: Option<u16>,
    /// `host:port` native-protocol listeners of other known healthy nodes.
    #[serde(default)]
    pub native_nodes: Vec<String>,
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

impl BootstrapInfo {
    /// Native-protocol providers advertised by this answer, dialable as
    /// `host:port`: the answering node's own listener first (derived from
    /// the bootstrap URL host + `native_port`), then every other advertised
    /// native node, deduplicated.
    pub fn native_providers(&self, bootstrap_url: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let (Some(port), Some(host)) = (self.native_port, url_host(bootstrap_url)) {
            out.push(format!("{host}:{port}"));
        }
        for node in &self.native_nodes {
            if !node.is_empty() && !out.contains(node) {
                out.push(node.clone());
            }
        }
        out
    }
}

/// Host of an `http(s)://host[:port][/path]` URL. IPv6 hosts keep their
/// brackets so `host:port` stays dialable. Dependency-free on purpose.
fn url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.is_empty() {
        return None;
    }
    let host = if authority.starts_with('[') {
        // [ipv6] or [ipv6]:port
        let end = authority.find(']')?;
        authority[..=end].to_string()
    } else {
        authority.split(':').next().unwrap_or(authority).to_string()
    };
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_providers_from_bootstrap_answer() {
        let info = BootstrapInfo {
            native_port: Some(4077),
            native_nodes: vec![
                "45.1.1.1:4077".into(),
                "45.1.1.1:4077".into(), // duplicate dropped
                "".into(),              // empty dropped
            ],
            ..Default::default()
        };
        assert_eq!(
            info.native_providers("https://federate.network"),
            vec!["federate.network:4077", "45.1.1.1:4077"]
        );
        // IPv6 bootstrap host keeps brackets so host:port stays dialable.
        assert_eq!(
            info.native_providers("http://[2001:db8::1]:9000/x"),
            vec!["[2001:db8::1]:4077", "45.1.1.1:4077"]
        );
        // No native_port advertised: only peer nodes remain.
        let no_port = BootstrapInfo {
            native_nodes: vec!["45.2.2.2:4077".into()],
            ..Default::default()
        };
        assert_eq!(
            no_port.native_providers("https://federate.network"),
            vec!["45.2.2.2:4077"]
        );
        assert_eq!(url_host("not a url"), None);
    }
}
