//! federate-cdn — block caching, replica lookup, provider selection, eviction.
//!
//! CDN/storage nodes are gateway-facing, never browser-facing: browsers talk
//! HTTP to gateways; gateways fetch hash-verified blocks from CDN/storage/
//! origin providers discovered through the node directory.

use federate_core::{FederateError, Result};
use federate_directory::{NodeEntry, NodeStatus};
use federate_storage::BlockStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Provider selection
// ---------------------------------------------------------------------------

/// Rank block providers for a fetch: online before degraded, same region
/// first, then lowest latency. Returns providers best-first so callers can
/// fail over down the list.
pub fn rank_providers<'a>(providers: &'a [NodeEntry], region: Option<&str>) -> Vec<&'a NodeEntry> {
    let mut ranked: Vec<&NodeEntry> = providers
        .iter()
        .filter(|p| p.status != NodeStatus::Offline)
        .collect();
    ranked.sort_by_key(|p| {
        (
            (p.status != NodeStatus::Online) as u8,
            (region.is_none_or(|r| p.registration.region != r)) as u8,
            p.latency_ms.unwrap_or(u64::MAX),
        )
    });
    ranked
}

/// Fetch a block from a provider's block API and verify its hash. A provider
/// returning wrong bytes is detected here — nodes are never trusted blindly.
pub async fn fetch_block_from(provider: &NodeEntry, hash: &str) -> Result<Vec<u8>> {
    let base = provider.registration.health_endpoint.trim_end_matches('/');
    let url = format!("{base}/v1/block/{hash}");
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("reqwest client")
        .get(&url)
        .send()
        .await
        .map_err(|e| FederateError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(FederateError::BlockNotFound(hash.to_string()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| FederateError::Network(e.to_string()))?
        .to_vec();
    federate_storage::verify(&bytes, hash)?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// LRU block cache
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheIndex {
    /// hash -> (last_used unix seconds, size bytes)
    entries: HashMap<String, (u64, u64)>,
}

/// A size-bounded content-addressed cache with LRU eviction, backed by the
/// same hash-verified BlockStore used everywhere else.
pub struct CdnCache {
    store: BlockStore,
    index: Mutex<CacheIndex>,
    index_path: std::path::PathBuf,
    max_bytes: u64,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl CdnCache {
    pub fn new(data_dir: &Path, max_bytes: u64) -> Result<Self> {
        let store = BlockStore::new(data_dir)?;
        let index_path = data_dir.join("cdn-index.json");
        let index = std::fs::read(&index_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Ok(Self {
            store,
            index: Mutex::new(index),
            index_path,
            max_bytes,
        })
    }

    pub fn store(&self) -> &BlockStore {
        &self.store
    }

    fn persist_index(&self, index: &CacheIndex) {
        if let Ok(bytes) = serde_json::to_vec(index) {
            std::fs::write(&self.index_path, bytes).ok();
        }
    }

    /// Hashes currently cached (for directory announcements).
    pub fn cached_hashes(&self) -> Vec<String> {
        self.index.lock().unwrap().entries.keys().cloned().collect()
    }

    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let bytes = self.store.get(hash)?;
        let mut index = self.index.lock().unwrap();
        index
            .entries
            .entry(hash.to_string())
            .or_insert((0, bytes.len() as u64))
            .0 = now_secs();
        self.persist_index(&index);
        Ok(bytes)
    }

    /// Insert a block, evicting least-recently-used entries past the size cap.
    pub fn put(&self, hash: &str, bytes: &[u8]) -> Result<()> {
        self.store.put(hash, bytes)?;
        let mut index = self.index.lock().unwrap();
        index
            .entries
            .insert(hash.to_string(), (now_secs(), bytes.len() as u64));

        let mut total: u64 = index.entries.values().map(|(_, s)| s).sum();
        if total > self.max_bytes {
            let mut by_age: Vec<(String, u64, u64)> = index
                .entries
                .iter()
                .map(|(h, (t, s))| (h.clone(), *t, *s))
                .collect();
            by_age.sort_by_key(|(_, t, _)| *t);
            for (victim, _, size) in by_age {
                if total <= self.max_bytes || victim == hash {
                    continue;
                }
                self.store.remove(&victim).ok();
                index.entries.remove(&victim);
                total -= size;
                tracing::debug!("cdn cache evicted {victim} ({size} bytes)");
            }
        }
        self.persist_index(&index);
        Ok(())
    }

    pub fn has(&self, hash: &str) -> bool {
        self.store.has(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_eviction_respects_cap() {
        let dir = std::env::temp_dir().join(format!("fed-cdn-test-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let cache = CdnCache::new(&dir, 10).unwrap();
        let a = b"aaaaaa"; // 6 bytes
        let b = b"bbbbbb"; // 6 bytes -> total 12 > 10, evict a
        let ha = federate_storage::hash_bytes(a);
        let hb = federate_storage::hash_bytes(b);
        cache.put(&ha, a).unwrap();
        cache.put(&hb, b).unwrap();
        assert!(!cache.has(&ha));
        assert!(cache.has(&hb));
        std::fs::remove_dir_all(&dir).ok();
    }
}
