//! federate-resolution: the central resolution engine.
//!
//! domain -> root zone -> TLD record -> domain record -> manifest -> blocks
//!
//! Every layer is cryptographically verified before serving:
//!   1. root zone signature (Federate Root Key, pinned trust anchor)
//!   2. TLD record signature (Federate Root Key)
//!   3. domain record signature (authorized TLD operator key)
//!   4. manifest signature (domain owner key from the domain record)
//!   5. content block hashes
//!
//! Node 1 is a distributor of signed data, never a blindly trusted authority.
//!
//! This crate is transport-agnostic: used by the HTTP gateway today, reusable
//! by the future DNS resolver, desktop app, publishing tools, peer/CDN.

use federate_client::NodeClient;
use federate_core::{FederateError, Result};
use federate_manifest::Manifest;
use federate_naming::{FederateDomain, RegistryType};
use federate_root::{RootCache, RootZone};
use federate_storage::BlockStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Outcome of resolving a (host, path) pair.
#[derive(Debug)]
pub enum Resolved {
    /// Fully verified content ready to serve.
    Content {
        domain: String,
        path: String,
        bytes: Vec<u8>,
        mime: String,
    },
    /// Host is not even a syntactically valid Federate name.
    NotFederate { host: String },
    /// TLD not present in the Federate root registry.
    TldNotFound { tld: String },
    /// TLD exists but is not resolvable (reserved/blocked/disabled/pending/expired/revoked).
    TldUnavailable { tld: String, status: String },
    /// TLD is delegated but delegated registry resolution is not active yet.
    DelegatedNotImplemented { domain: String, tld: String },
    /// TLD resolvable but domain not registered.
    DomainNotFound { domain: String },
    /// Domain exists but is not active (suspended/expired/revoked/...).
    DomainUnavailable { domain: String, status: String },
    /// Domain exists but the manifest has no entry for this path.
    PathNotFound { domain: String, path: String },
    /// A signature/hash verification failed. Content is never served.
    SecurityFailure {
        domain: String,
        layer: String,
        reason: String,
    },
}

/// The resolution engine. Caches root zone, manifests, and blocks locally so
/// cached (verified) sites keep working when Node 1 is temporarily offline.
pub struct Resolver {
    client: NodeClient,
    root_cache: RootCache,
    blocks: BlockStore,
    manifest_dir: PathBuf,
    root: RwLock<Option<Arc<RootZone>>>,
    /// Pinned Federate Root public key (trust anchor). Configured explicitly
    /// or pinned on first use (TOFU) and persisted to disk.
    trusted_root_key: RwLock<Option<String>>,
    trusted_key_path: PathBuf,
    /// Optional node directory: when set, blocks are fetched from healthy
    /// CDN/storage/origin providers first, falling back to Node 1. Every
    /// provider response is hash-verified; providers are never trusted.
    directory: Option<federate_directory::DirectoryClient>,
    region: Option<String>,
}

impl Resolver {
    /// `configured_root_key`: explicit trust anchor (recommended for
    /// production). When None, the key is pinned from the first
    /// self-consistent signed root zone (TOFU) and persisted.
    pub fn new(
        client: NodeClient,
        data_dir: &Path,
        configured_root_key: Option<String>,
    ) -> Result<Self> {
        let manifest_dir = data_dir.join("manifests");
        std::fs::create_dir_all(&manifest_dir)?;
        let trusted_key_path = data_dir.join("trusted-root-key");
        let trusted = configured_root_key
            .or_else(|| {
                std::fs::read_to_string(&trusted_key_path)
                    .ok()
                    .map(|s| s.trim().to_string())
            })
            .filter(|s| !s.is_empty());
        Ok(Self {
            client,
            root_cache: RootCache::new(data_dir),
            blocks: BlockStore::new(data_dir)?,
            manifest_dir,
            root: RwLock::new(None),
            trusted_root_key: RwLock::new(trusted),
            trusted_key_path,
            directory: None,
            region: None,
        })
    }

    /// Enable directory-based block fetching (used by gateway nodes).
    pub fn with_directory(
        mut self,
        directory: federate_directory::DirectoryClient,
        region: Option<String>,
    ) -> Self {
        self.directory = Some(directory);
        self.region = region;
        self
    }

    pub fn block_store(&self) -> &BlockStore {
        &self.blocks
    }

    pub fn bootstrap_url(&self) -> &str {
        self.client.base_url()
    }

    pub async fn trusted_root_key(&self) -> Option<String> {
        self.trusted_root_key.read().await.clone()
    }

    /// Verify a zone against the pinned root key. If no key is pinned yet,
    /// pin the zone's advertised key after checking the zone is
    /// self-consistently signed by it (trust-on-first-use).
    async fn verify_zone(&self, zone: &RootZone) -> Result<()> {
        let pinned = self.trusted_root_key.read().await.clone();
        match pinned {
            Some(key) => zone.verify(&key),
            None => {
                zone.verify(&zone.root_public_key)?;
                std::fs::write(&self.trusted_key_path, &zone.root_public_key)?;
                *self.trusted_root_key.write().await = Some(zone.root_public_key.clone());
                tracing::warn!(
                    "pinned Federate Root Key on first use: {}; pass --root-key to configure explicitly",
                    zone.root_public_key
                );
                Ok(())
            }
        }
    }

    /// The last verified zone we already hold (memory, then disk cache).
    /// Used for rollback protection when a node serves an older zone.
    async fn last_verified_zone(&self) -> Option<Arc<RootZone>> {
        if let Some(zone) = self.root.read().await.clone() {
            return Some(zone);
        }
        let cached = self.root_cache.load().ok()?;
        self.verify_zone(&cached).await.ok()?;
        Some(Arc::new(cached))
    }

    /// Fetch the root zone from Node 1, verify its signature chain, fall back
    /// to the (previously verified) disk cache when the network is down.
    /// Unverifiable zones are NEVER stored or used. A correctly signed but
    /// OLDER zone than one we already verified is also rejected (rollback /
    /// replay protection): Node 1 distributes signed data, it cannot rewind it.
    pub async fn refresh_root(&self) -> Result<Arc<RootZone>> {
        match self.client.fetch_root().await {
            Ok(zone) => {
                if let Err(e) = self.verify_zone(&zone).await {
                    // Server sent an unverifiable zone (tampering or key
                    // mismatch). Never use it; fall back to the last verified
                    // cached zone so legitimate cached sites stay up.
                    tracing::error!("REJECTED root zone from node: {e}");
                    if let Ok(cached) = self.root_cache.load() {
                        if self.verify_zone(&cached).await.is_ok() {
                            let arc = Arc::new(cached);
                            *self.root.write().await = Some(arc.clone());
                            return Ok(arc);
                        }
                    }
                    return Err(e);
                }
                if let Some(known) = self.last_verified_zone().await {
                    if zone.root_version < known.root_version {
                        tracing::error!(
                            fetched = zone.root_version,
                            known = known.root_version,
                            "REJECTED root zone older than the last verified one (possible replay); keeping the newer zone"
                        );
                        *self.root.write().await = Some(known.clone());
                        return Ok(known);
                    }
                }
                self.root_cache.store(&zone)?;
                let arc = Arc::new(zone);
                *self.root.write().await = Some(arc.clone());
                tracing::info!(
                    version = arc.root_version,
                    "root zone verified and refreshed"
                );
                Ok(arc)
            }
            Err(e) => {
                tracing::warn!("root fetch failed ({e}); trying local cache");
                let zone = self
                    .root_cache
                    .load()
                    .map_err(|_| FederateError::RootUnavailable)?;
                self.verify_zone(&zone).await?;
                let arc = Arc::new(zone);
                *self.root.write().await = Some(arc.clone());
                Ok(arc)
            }
        }
    }

    /// Current root zone: memory -> disk cache -> network. Always verified.
    pub async fn root(&self) -> Result<Arc<RootZone>> {
        if let Some(zone) = self.root.read().await.clone() {
            return Ok(zone);
        }
        self.refresh_root().await
    }

    async fn manifest(&self, hash: &str) -> Result<Manifest> {
        // Reject any hash that isn't a valid content address before it can be
        // used to build a cache path (blocks traversal like `../../`).
        if !federate_storage::is_valid_hash(hash) {
            return Err(FederateError::ManifestNotFound(hash.to_string()));
        }
        let cached = self.manifest_dir.join(hash);
        if let Ok(bytes) = std::fs::read(&cached) {
            // The manifest is content-addressed: cached bytes must hash to
            // `hash`, exactly as fetched. A mismatch means tampering/corruption.
            if federate_storage::verify(&bytes, hash).is_ok() {
                if let Ok(m) = serde_json::from_slice::<Manifest>(&bytes) {
                    if m.validate().is_ok() {
                        return Ok(m);
                    }
                }
            }
            std::fs::remove_file(&cached).ok();
        }
        // Fetch raw, hash-verified bytes and cache them verbatim so the next
        // read is a real cache hit (re-serializing would change the hash).
        let bytes = self.client.fetch_manifest_bytes(hash).await?;
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        manifest.validate()?;
        // Write-then-rename: a crash mid-write must not leave a truncated
        // manifest at the content-addressed cache path.
        let tmp = self.manifest_dir.join(format!("{hash}.tmp"));
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &cached)?;
        Ok(manifest)
    }

    async fn block(&self, hash: &str) -> Result<Vec<u8>> {
        // BlockStore::get re-verifies the hash and fails on tampered cache;
        // tampered entries are removed so the next fetch can repair them.
        match self.blocks.get(hash) {
            Ok(bytes) => return Ok(bytes),
            Err(FederateError::HashMismatch { .. }) => {
                tracing::warn!("cached block {hash} failed hash validation; evicting");
            }
            Err(_) => {}
        }
        // Try CDN/storage/origin providers from the node directory first.
        if let Some(dir) = &self.directory {
            if let Ok(providers) = dir.providers(hash, None).await {
                for provider in federate_cdn::rank_providers(&providers, self.region.as_deref()) {
                    match federate_cdn::fetch_block_from(provider, hash).await {
                        Ok(bytes) => {
                            self.blocks.put(hash, &bytes)?;
                            return Ok(bytes);
                        }
                        Err(e) => tracing::debug!(
                            "provider {} failed for block {hash}: {e}",
                            provider.registration.node_id
                        ),
                    }
                }
            }
        }
        // Fall back to Node 1 (origin of official content).
        let bytes = self.client.fetch_block(hash).await?;
        self.blocks.put(hash, &bytes)?;
        Ok(bytes)
    }

    /// Fetch a block by hash (providers first, then Node 1) and cache it.
    /// Used by CDN nodes for fetch-on-miss.
    pub async fn fetch_and_cache_block(&self, hash: &str) -> Result<Vec<u8>> {
        self.block(hash).await
    }

    /// Resolve a domain to its verified record (no content fetch). Entry
    /// point for the future DNS resolver / CLI diagnostics.
    pub async fn resolve_domain(&self, host: &str) -> Result<federate_naming::DomainRecord> {
        let domain = FederateDomain::parse(host)?;
        let root = self.root().await?;
        let tld_rec = root
            .lookup_tld(&domain.tld)
            .ok_or_else(|| FederateError::TldNotFound {
                tld: domain.tld.clone(),
            })?;
        if !tld_rec.status.is_resolvable() || tld_rec.is_expired() {
            return Err(FederateError::TldUnavailable {
                tld: domain.tld.clone(),
                status: if tld_rec.is_expired() {
                    "expired".into()
                } else {
                    tld_rec.status.as_str().into()
                },
            });
        }
        let record = root
            .lookup(&domain.fqdn())
            .ok_or_else(|| FederateError::DomainNotFound(domain.fqdn()))?;
        record.verify(&tld_rec.operator_public_key)?;
        if !record.status.is_resolvable() || record.is_expired() {
            return Err(FederateError::TldUnavailable {
                tld: domain.tld.clone(),
                status: if record.is_expired() {
                    "expired".into()
                } else {
                    record.status.as_str().into()
                },
            });
        }
        Ok(record.clone())
    }

    /// Verified list of file paths a domain publishes (manifest keys). Goes
    /// through the full root → TLD → domain → manifest verification chain, so
    /// unverifiable sites yield nothing. Used by the search indexer to walk
    /// every page instead of only "/".
    pub async fn site_files(&self, host: &str) -> Result<Vec<String>> {
        let record = self.resolve_domain(host).await?;
        let manifest = self.manifest(&record.manifest_hash).await?;
        manifest.verify(host, &record.owner_public_key)?;
        Ok(manifest.files.keys().cloned().collect())
    }

    /// Full verified content resolution used by the HTTP gateway.
    pub async fn resolve(&self, host: &str, path: &str) -> Result<Resolved> {
        // 1-2. Parse: syntax check only. Existence comes from the root zone.
        let domain = match FederateDomain::parse(host) {
            Ok(d) => d,
            Err(_) => {
                return Ok(Resolved::NotFederate {
                    host: host.to_string(),
                })
            }
        };
        let fqdn = domain.fqdn();

        // 3. Root zone (signature verified on load; hard fail if not).
        let root = match self.root().await {
            Ok(r) => r,
            Err(FederateError::VerificationFailed {
                layer,
                subject,
                reason,
            }) => {
                return Ok(Resolved::SecurityFailure {
                    domain: subject,
                    layer,
                    reason,
                })
            }
            Err(e) => return Err(e),
        };

        // 4. TLD exists?
        let tld_rec = match root.lookup_tld(&domain.tld) {
            Some(t) => t,
            None => {
                return Ok(Resolved::TldNotFound {
                    tld: domain.tld.clone(),
                })
            }
        };

        // TLD record signature (defense in depth; also checked in verify_zone).
        let trusted = self.trusted_root_key().await.unwrap_or_default();
        if let Err(FederateError::VerificationFailed { layer, reason, .. }) =
            tld_rec.verify(&trusted)
        {
            return Ok(Resolved::SecurityFailure {
                domain: fqdn,
                layer,
                reason,
            });
        }

        // 5. TLD active? Status AND expiry; an old signed record whose lease
        // has passed must stop resolving even before governance flips status.
        if !tld_rec.status.is_resolvable() {
            return Ok(Resolved::TldUnavailable {
                tld: domain.tld.clone(),
                status: tld_rec.status.as_str().to_string(),
            });
        }
        if tld_rec.is_expired() {
            return Ok(Resolved::TldUnavailable {
                tld: domain.tld.clone(),
                status: "expired".to_string(),
            });
        }

        // 6. Route by registry type.
        match tld_rec.registry_type {
            RegistryType::RootManaged => {}
            // Delegated registries: phase 6. Clear structured error for now.
            _ => {
                return Ok(Resolved::DelegatedNotImplemented {
                    domain: fqdn,
                    tld: domain.tld.clone(),
                });
            }
        }

        // 7. Domain record from the root-managed registry.
        let record = match root.lookup(&fqdn) {
            Some(r) => r,
            None => return Ok(Resolved::DomainNotFound { domain: fqdn }),
        };

        // Domain record signed by the authorized TLD operator + consistent.
        if let Err(FederateError::VerificationFailed { layer, reason, .. }) =
            record.verify(&tld_rec.operator_public_key)
        {
            return Ok(Resolved::SecurityFailure {
                domain: fqdn,
                layer,
                reason,
            });
        }
        if record.tld != domain.tld {
            return Ok(Resolved::SecurityFailure {
                domain: fqdn,
                layer: "domain".into(),
                reason: "domain record belongs to a different TLD".into(),
            });
        }
        if !record.status.is_resolvable() {
            return Ok(Resolved::DomainUnavailable {
                domain: fqdn,
                status: record.status.as_str().to_string(),
            });
        }
        if record.is_expired() {
            return Ok(Resolved::DomainUnavailable {
                domain: fqdn,
                status: "expired".to_string(),
            });
        }

        // 8. Manifest: content-addressed fetch + owner signature.
        let manifest = match self.manifest(&record.manifest_hash).await {
            Ok(m) => m,
            Err(FederateError::HashMismatch { expected, actual }) => {
                return Ok(Resolved::SecurityFailure {
                    domain: fqdn,
                    layer: "manifest".into(),
                    reason: format!("manifest hash mismatch (expected {expected}, got {actual})"),
                })
            }
            Err(e) => return Err(e),
        };
        if let Err(FederateError::VerificationFailed { layer, reason, .. }) =
            manifest.verify(&fqdn, &record.owner_public_key)
        {
            return Ok(Resolved::SecurityFailure {
                domain: fqdn,
                layer,
                reason,
            });
        }

        // 9. Content block, hash-verified (fetch AND cache read).
        let (file_name, file_hash) = match manifest.resolve_path(path) {
            Some((name, h)) => (name, h.to_string()),
            None => {
                return Ok(Resolved::PathNotFound {
                    domain: fqdn,
                    path: path.to_string(),
                })
            }
        };
        let bytes = match self.block(&file_hash).await {
            Ok(b) => b,
            Err(FederateError::HashMismatch { expected, actual }) => {
                return Ok(Resolved::SecurityFailure {
                    domain: fqdn,
                    layer: "content".into(),
                    reason: format!(
                        "content block hash mismatch (expected {expected}, got {actual})"
                    ),
                })
            }
            Err(e) => return Err(e),
        };

        let mime = mime_guess::from_path(&file_name)
            .first_or_octet_stream()
            .to_string();
        Ok(Resolved::Content {
            domain: fqdn,
            path: path.to_string(),
            bytes,
            mime,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use federate_identity::NodeIdentity;
    use federate_naming::{DomainRecord, DomainStatus, TargetType, TldMode, TldStatus};
    use federate_root::{RootZone, TldRecord, SIGNATURE_ALGORITHM};
    use std::collections::BTreeMap;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fed-resolution-{name}-{}-{}",
            std::process::id(),
            name.len()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Keys {
        root: NodeIdentity,
        operator: NodeIdentity,
    }

    fn keys(dir: &Path) -> Keys {
        Keys {
            root: NodeIdentity::load_or_create(&dir.join("root")).unwrap(),
            operator: NodeIdentity::load_or_create(&dir.join("op")).unwrap(),
        }
    }

    fn signed_tld(keys: &Keys, tld: &str) -> TldRecord {
        let mut rec = TldRecord {
            tld: tld.into(),
            status: TldStatus::Official,
            mode: TldMode::Official,
            owner_public_key: keys.root.node_id(),
            operator_public_key: keys.operator.node_id(),
            operator_name: "test".into(),
            registry_type: federate_naming::RegistryType::RootManaged,
            registry_endpoint: None,
            registry_manifest_hash: None,
            policy_hash: None,
            pricing: None,
            created_at: "t".into(),
            updated_at: "t".into(),
            expires_at: None,
            notes: None,
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        rec.signature = Some(keys.root.sign(&rec.signable_bytes().unwrap()));
        rec
    }

    fn signed_domain(keys: &Keys, fqdn: &str, expires_at: Option<String>) -> DomainRecord {
        let (label, tld) = fqdn.split_once('.').unwrap();
        let mut rec = DomainRecord {
            domain: fqdn.into(),
            tld: tld.into(),
            label: label.into(),
            owner_public_key: "00".repeat(32),
            target_type: TargetType::Manifest,
            manifest_hash: "0".repeat(64),
            service_id: None,
            node_id: None,
            status: DomainStatus::Active,
            created_at: "t".into(),
            updated_at: "t".into(),
            expires_at,
            renewal: None,
            pricing: None,
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        rec.signature = Some(keys.operator.sign(&rec.signable_bytes().unwrap()));
        rec
    }

    fn signed_zone(keys: &Keys, version: u64, domains: BTreeMap<String, DomainRecord>) -> RootZone {
        let mut tlds = BTreeMap::new();
        tlds.insert("fed".to_string(), signed_tld(keys, "fed"));
        let mut zone = RootZone {
            network: "federate".into(),
            root_version: version,
            generated_at: "t".into(),
            root_public_key: keys.root.node_id(),
            tlds,
            domains,
            audit: vec![],
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(keys.root.sign(&zone.signable_bytes().unwrap()));
        zone
    }

    /// Serve a fixed zone at /v1/root on an ephemeral port.
    async fn serve_zone(zone: RootZone) -> String {
        let app = axum::Router::new().route(
            "/v1/root",
            axum::routing::get(move || {
                let zone = zone.clone();
                async move { axum::Json(zone) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn rollback_zone_rejected_newer_cache_wins() {
        let dir = tmp("rollback");
        let keys = keys(&dir);
        let newer = signed_zone(&keys, 10, BTreeMap::new());
        let older = signed_zone(&keys, 5, BTreeMap::new());

        // Node serves a correctly signed but OLDER zone (replay).
        let base = serve_zone(older).await;
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        // Pre-seed the verified cache with the newer zone.
        RootCache::new(&data_dir).store(&newer).unwrap();

        let resolver =
            Resolver::new(NodeClient::new(&base), &data_dir, Some(keys.root.node_id())).unwrap();
        let zone = resolver.refresh_root().await.unwrap();
        assert_eq!(
            zone.root_version, 10,
            "replayed older zone must not displace the newer verified one"
        );
    }

    #[tokio::test]
    async fn newer_zone_from_node_accepted() {
        let dir = tmp("forward");
        let keys = keys(&dir);
        let old = signed_zone(&keys, 3, BTreeMap::new());
        let new = signed_zone(&keys, 4, BTreeMap::new());
        let base = serve_zone(new).await;
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        RootCache::new(&data_dir).store(&old).unwrap();
        let resolver =
            Resolver::new(NodeClient::new(&base), &data_dir, Some(keys.root.node_id())).unwrap();
        assert_eq!(resolver.refresh_root().await.unwrap().root_version, 4);
    }

    #[tokio::test]
    async fn zone_signed_by_wrong_key_rejected() {
        let dir = tmp("wrongkey");
        let keys = keys(&dir);
        let attacker = NodeIdentity::load_or_create(&dir.join("attacker")).unwrap();
        let forged = {
            let atk = Keys {
                root: attacker,
                operator: NodeIdentity::load_or_create(&dir.join("atk-op")).unwrap(),
            };
            signed_zone(&atk, 99, BTreeMap::new())
        };
        let base = serve_zone(forged).await;
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let resolver =
            Resolver::new(NodeClient::new(&base), &data_dir, Some(keys.root.node_id())).unwrap();
        // No cache to fall back to: refresh must fail, never accept.
        assert!(resolver.refresh_root().await.is_err());
    }

    #[tokio::test]
    async fn expired_domain_not_resolved_offline_from_verified_cache() {
        let dir = tmp("expired");
        let keys = keys(&dir);
        let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let future = (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339();
        let mut domains = BTreeMap::new();
        domains.insert(
            "old.fed".to_string(),
            signed_domain(&keys, "old.fed", Some(past)),
        );
        domains.insert(
            "live.fed".to_string(),
            signed_domain(&keys, "live.fed", Some(future)),
        );
        let zone = signed_zone(&keys, 1, domains);

        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        RootCache::new(&data_dir).store(&zone).unwrap();
        // Unreachable bootstrap: resolution runs from the verified disk cache.
        let resolver = Resolver::new(
            NodeClient::new("http://127.0.0.1:1"),
            &data_dir,
            Some(keys.root.node_id()),
        )
        .unwrap();

        match resolver.resolve("old.fed", "/").await.unwrap() {
            Resolved::DomainUnavailable { status, .. } => assert_eq!(status, "expired"),
            other => panic!("expired domain must not resolve, got {other:?}"),
        }
        // The live domain passes the record layer (fails later only because
        // its manifest isn't fetchable in this offline test).
        assert!(resolver.resolve_domain("live.fed").await.is_ok());
        assert!(resolver.resolve_domain("old.fed").await.is_err());

        // Unknown TLD and malformed hosts fail cleanly.
        assert!(matches!(
            resolver.resolve("x.doesnotexist", "/").await.unwrap(),
            Resolved::TldNotFound { .. }
        ));
        assert!(matches!(
            resolver.resolve("not a host!!", "/").await.unwrap(),
            Resolved::NotFederate { .. }
        ));
    }
}
