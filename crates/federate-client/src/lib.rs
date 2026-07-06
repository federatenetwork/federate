//! federate-client — HTTP client for Node 1 APIs (bootstrap, root, manifests, blocks).

use federate_core::{FederateError, Result};
use federate_manifest::Manifest;
use federate_root::RootZone;

/// One-off JSON GET (shared HTTP stack for small callers).
pub async fn get_json(url: &str) -> Result<serde_json::Value> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| FederateError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(FederateError::Network(format!(
            "{url} returned {}",
            resp.status()
        )));
    }
    resp.json()
        .await
        .map_err(|e| FederateError::Network(e.to_string()))
}

#[derive(Clone)]
pub struct NodeClient {
    base: String,
    http: reqwest::Client,
}

impl NodeClient {
    pub fn new(bootstrap_url: &str) -> Self {
        Self {
            base: bootstrap_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    async fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(FederateError::Network(format!(
                "{url} returned {}",
                resp.status()
            )));
        }
        Ok(resp
            .bytes()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?
            .to_vec())
    }

    pub async fn fetch_root(&self) -> Result<RootZone> {
        let bytes = self.get_bytes("/v1/root").await?;
        let zone: RootZone = serde_json::from_slice(&bytes)?;
        zone.validate()?;
        Ok(zone)
    }

    /// Fetch the raw, content-addressed manifest bytes and verify they hash to
    /// `hash`. Returns the exact bytes (so callers can cache them under `hash`
    /// and get a real cache hit on re-read).
    pub async fn fetch_manifest_bytes(&self, hash: &str) -> Result<Vec<u8>> {
        if !federate_storage::is_valid_hash(hash) {
            return Err(FederateError::BlockNotFound(hash.to_string()));
        }
        let bytes = self.get_bytes(&format!("/v1/manifest/{hash}")).await?;
        federate_storage::verify(&bytes, hash)?;
        Ok(bytes)
    }

    pub async fn fetch_manifest(&self, hash: &str) -> Result<Manifest> {
        let bytes = self.fetch_manifest_bytes(hash).await?;
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Fetch a content block and verify its hash before returning.
    pub async fn fetch_block(&self, hash: &str) -> Result<Vec<u8>> {
        if !federate_storage::is_valid_hash(hash) {
            return Err(FederateError::BlockNotFound(hash.to_string()));
        }
        let bytes = self.get_bytes(&format!("/v1/block/{hash}")).await?;
        federate_storage::verify(&bytes, hash)?;
        Ok(bytes)
    }

    pub async fn health(&self) -> Result<bool> {
        Ok(self.get_bytes("/health").await.is_ok())
    }
}
