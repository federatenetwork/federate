//! federate-client: HTTP client for Node 1 APIs (bootstrap, root, manifests, blocks).

use federate_core::{FederateError, Result};
use federate_manifest::Manifest;
use federate_root::RootZone;

/// Download caps. Every fetch from another node (Node 1, mirrors, providers)
/// is bounded so a malicious or broken peer cannot stream unbounded bytes
/// into memory. Blocks/manifests are hash-verified after the capped read.
pub const MAX_ROOT_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_BLOCK_BYTES: u64 = 64 * 1024 * 1024;

/// Read a response body, failing as soon as it exceeds `max` bytes (checks
/// the Content-Length header first, then enforces the cap while streaming).
pub async fn read_capped(resp: reqwest::Response, max: u64) -> Result<Vec<u8>> {
    let url = resp.url().to_string();
    let too_big =
        |got: u64| FederateError::Network(format!("{url} response exceeds {max} bytes ({got})"));
    if let Some(len) = resp.content_length() {
        if len > max {
            return Err(too_big(len));
        }
    }
    let mut out: Vec<u8> = Vec::new();
    let mut resp = resp;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| FederateError::Network(e.to_string()))?
    {
        if out.len() as u64 + chunk.len() as u64 > max {
            return Err(too_big(out.len() as u64 + chunk.len() as u64));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// One-off JSON GET (shared HTTP stack for small callers). Timeout and size
/// capped like every other cross-node fetch.
pub async fn get_json(url: &str) -> Result<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("reqwest client");
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| FederateError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(FederateError::Network(format!(
            "{url} returned {}",
            resp.status()
        )));
    }
    let bytes = read_capped(resp, 4 * 1024 * 1024).await?;
    Ok(serde_json::from_slice(&bytes)?)
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

    async fn get_bytes(&self, path: &str, max: u64) -> Result<Vec<u8>> {
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
        read_capped(resp, max).await
    }

    pub async fn fetch_root(&self) -> Result<RootZone> {
        let bytes = self.get_bytes("/v1/root", MAX_ROOT_BYTES).await?;
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
        let bytes = self
            .get_bytes(&format!("/v1/manifest/{hash}"), MAX_MANIFEST_BYTES)
            .await?;
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
        let bytes = self
            .get_bytes(&format!("/v1/block/{hash}"), MAX_BLOCK_BYTES)
            .await?;
        federate_storage::verify(&bytes, hash)?;
        Ok(bytes)
    }

    pub async fn health(&self) -> Result<bool> {
        Ok(self.get_bytes("/health", 4096).await.is_ok())
    }
}
