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
//! This crate is transport-agnostic and consumer-agnostic: the SAME engine
//! serves the native `fed://` path (resolve_uri), the HTTP gateway adapter,
//! the CLI, DNS decisions, and any future Federate browser. Compatibility
//! layers translate into a `FederateUri` and call in here; nothing resolves
//! names on its own.

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
        /// Content address of the served block (BLAKE3 hex). Stable per
        /// content version, so gateways can use it as a strong ETag.
        hash: String,
    },
    /// Host is not even a syntactically valid Federate name.
    NotFederate { host: String },
    /// TLD not present in the Federate root registry.
    TldNotFound { tld: String },
    /// TLD exists but is not resolvable (reserved/blocked/disabled/pending/expired/revoked).
    TldUnavailable { tld: String, status: String },
    /// TLD is delegated but its registry cannot be used right now: every
    /// provider unreachable and nothing cached, or an unsupported registry
    /// mode. Signature failures are `SecurityFailure`, never this.
    DelegatedUnavailable {
        domain: String,
        tld: String,
        reason: String,
    },
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
    /// Last verified delegated TLD registries (`<tld>.json`), so delegated
    /// sites keep resolving when their registry providers are offline.
    registries_dir: PathBuf,
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
    /// Default native-protocol providers (`host:port`), tried for root
    /// zones and manifests, and for blocks after any directory-discovered
    /// native providers. Usually the bootstrap node's own native listener
    /// plus any peers learned from `/v1/bootstrap`.
    native_providers: Vec<String>,
    /// This client's identity for native protocol handshakes (identity, not
    /// authority: what we fetch is still verified by hash/signature).
    identity: federate_identity::NodeIdentity,
}

/// Ordered fetch trace for diagnostics (`federate fetch --trace`). Collects
/// one line per meaningful step; cheap no-op when absent.
#[derive(Default)]
pub struct Trace(std::sync::Mutex<Vec<String>>);

impl Trace {
    pub fn push(&self, event: impl Into<String>) {
        self.0.lock().unwrap().push(event.into());
    }
    pub fn events(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

fn trace(t: Option<&Trace>, event: impl Into<String>) {
    if let Some(t) = t {
        t.push(event);
    }
}

fn short(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

/// Agent string sent in native-protocol handshakes (diagnostic only).
const AGENT: &str = concat!("federate-resolution/", env!("CARGO_PKG_VERSION"));

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
        let registries_dir = data_dir.join("registries");
        std::fs::create_dir_all(&registries_dir)?;
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
            registries_dir,
            root: RwLock::new(None),
            trusted_root_key: RwLock::new(trusted),
            trusted_key_path,
            directory: None,
            region: None,
            native_providers: Vec::new(),
            identity: federate_identity::NodeIdentity::load_or_create(data_dir)?,
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

    /// Default native-protocol providers (`host:port`). Used for the native
    /// pass of every fetch: root zones and manifests are tried here before
    /// the HTTP compatibility endpoint; blocks try these after any
    /// directory-discovered native providers.
    pub fn with_native_providers(mut self, providers: Vec<String>) -> Self {
        self.native_providers = providers;
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

    /// Fetch the root zone from the network with the native protocol first:
    /// each configured native provider is asked `GetRoot`; only when none
    /// answers does the fetch fall back to the bootstrap node's HTTP
    /// compatibility endpoint. The transport never matters for trust: the
    /// caller verifies the zone signature either way.
    async fn fetch_root_network(&self) -> Result<RootZone> {
        for addr in &self.native_providers {
            match self.fetch_root_native(addr).await {
                Ok(zone) => {
                    tracing::debug!("root zone fetched over the native protocol from {addr}");
                    return Ok(zone);
                }
                Err(e) => {
                    tracing::debug!("native root fetch from {addr} failed: {e}; trying next")
                }
            }
        }
        self.client.fetch_root().await
    }

    /// One native-protocol root fetch: connect, handshake, GetRoot. The
    /// returned zone is structurally validated only; signature verification
    /// (and rollback protection) happens in the caller, same as HTTP.
    async fn fetch_root_native(&self, addr: &str) -> Result<RootZone> {
        let (mut conn, _welcome) =
            federate_transport::Connection::connect(addr, &self.identity, AGENT).await?;
        match conn.request(&federate_protocol::Message::GetRoot).await? {
            federate_protocol::Message::Root { zone_json } => {
                if zone_json.len() as u64 > federate_client::MAX_ROOT_BYTES {
                    return Err(FederateError::Network(format!(
                        "native root zone exceeds {} bytes",
                        federate_client::MAX_ROOT_BYTES
                    )));
                }
                let zone: RootZone = serde_json::from_slice(&zone_json)?;
                zone.validate()?;
                Ok(zone)
            }
            federate_protocol::Message::Error { code, detail } => Err(FederateError::Network(
                format!("native provider answered {code:?}: {detail}"),
            )),
            other => Err(FederateError::Network(format!(
                "native provider answered unexpectedly: {other:?}"
            ))),
        }
    }

    /// Fetch the root zone from the network (native first, HTTP fallback),
    /// verify its signature chain, fall back to the (previously verified)
    /// disk cache when the network is down.
    /// Unverifiable zones are NEVER stored or used. A correctly signed but
    /// OLDER zone than one we already verified is also rejected (rollback /
    /// replay protection): a node distributes signed data, it cannot rewind it.
    pub async fn refresh_root(&self) -> Result<Arc<RootZone>> {
        match self.fetch_root_network().await {
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

    /// Raw, content-addressed manifest bytes: local cache, then native
    /// providers, then the HTTP compatibility endpoint. Every source is
    /// untrusted; bytes count only after they hash to `hash`. The exact bytes
    /// are cached verbatim (re-serializing would change the hash) and served
    /// verbatim, so nodes can relay manifests they cannot even parse.
    pub async fn manifest_bytes(&self, hash: &str) -> Result<Vec<u8>> {
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
                return Ok(bytes);
            }
            std::fs::remove_file(&cached).ok();
        }
        let bytes = self.fetch_manifest_network(hash).await?;
        // Write-then-rename: a crash mid-write must not leave a truncated
        // manifest at the content-addressed cache path.
        let tmp = self.manifest_dir.join(format!("{hash}.tmp"));
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &cached)?;
        Ok(bytes)
    }

    /// Manifest fetch from the network: native providers first, HTTP last.
    async fn fetch_manifest_network(&self, hash: &str) -> Result<Vec<u8>> {
        for addr in &self.native_providers {
            match self.fetch_manifest_native(addr, hash).await {
                Ok(bytes) => {
                    tracing::debug!("manifest {hash} fetched over the native protocol from {addr}");
                    return Ok(bytes);
                }
                Err(e) => {
                    tracing::debug!("native manifest fetch from {addr} failed: {e}; trying next")
                }
            }
        }
        self.client.fetch_manifest_bytes(hash).await
    }

    /// One native-protocol manifest fetch, hash-verified before returning.
    async fn fetch_manifest_native(&self, addr: &str, hash: &str) -> Result<Vec<u8>> {
        let (mut conn, _welcome) =
            federate_transport::Connection::connect(addr, &self.identity, AGENT).await?;
        match conn
            .request(&federate_protocol::Message::GetManifest {
                hash: hash.to_string(),
            })
            .await?
        {
            federate_protocol::Message::Manifest { bytes, .. } => {
                if bytes.len() as u64 > federate_client::MAX_MANIFEST_BYTES {
                    return Err(FederateError::Network(format!(
                        "native manifest exceeds {} bytes",
                        federate_client::MAX_MANIFEST_BYTES
                    )));
                }
                federate_storage::verify(&bytes, hash)?;
                Ok(bytes)
            }
            federate_protocol::Message::Error { code, detail } => Err(FederateError::Network(
                format!("native provider answered {code:?}: {detail}"),
            )),
            other => Err(FederateError::Network(format!(
                "native provider answered unexpectedly: {other:?}"
            ))),
        }
    }

    async fn manifest(&self, hash: &str) -> Result<Manifest> {
        let bytes = self.manifest_bytes(hash).await?;
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    // -----------------------------------------------------------------
    // Delegated TLD registries
    // -----------------------------------------------------------------

    /// The operator-signed domain record for `fqdn`, located through its
    /// TLD's registry: the signed root zone for root-managed TLDs, the
    /// operator's signed registry for delegated TLDs. The record's own
    /// signature and status are verified by the caller, identically for
    /// both paths.
    async fn locate_record(
        &self,
        root: &RootZone,
        tld_rec: &federate_root::TldRecord,
        fqdn: &str,
        t: Option<&Trace>,
    ) -> Result<federate_naming::DomainRecord> {
        match tld_rec.registry_type {
            RegistryType::RootManaged => {
                trace(
                    t,
                    format!(
                        "registry for .{}: root-managed (records live in the signed root zone)",
                        tld_rec.tld
                    ),
                );
                root.lookup(fqdn)
                    .cloned()
                    .ok_or_else(|| FederateError::DomainNotFound(fqdn.to_string()))
            }
            RegistryType::DelegatedManifest
            | RegistryType::DelegatedNative
            | RegistryType::DelegatedHttp => {
                let registry = self.tld_registry(tld_rec, t).await?;
                registry
                    .lookup(fqdn)
                    .cloned()
                    .ok_or_else(|| FederateError::DomainNotFound(fqdn.to_string()))
            }
            RegistryType::OfflineManual => Err(FederateError::DelegatedRegistryUnavailable {
                tld: tld_rec.tld.clone(),
                reason: "registry is administered offline and cannot be resolved".into(),
            }),
        }
    }

    /// The verified signed registry of a delegated TLD.
    ///
    /// Fail-closed rules: an operator-signature failure is returned as
    /// `VerificationFailed` and never falls back to anything; only
    /// reachability failures fall back (to the last verified cached
    /// registry, then to `DelegatedRegistryUnavailable`).
    ///
    /// Modes:
    /// - `delegated_manifest`: content-addressed bytes pinned by the
    ///   root-signed TLD record; fetched like any manifest (cache, native
    ///   providers, HTTP fallback), then operator-signature verified.
    /// - `delegated_native` / `delegated_http`: live registry from the TLD's
    ///   own providers; verified, then version rollback protection against
    ///   the last verified cached copy (an operator or host cannot rewind
    ///   the namespace we already saw).
    pub async fn tld_registry(
        &self,
        tld_rec: &federate_root::TldRecord,
        t: Option<&Trace>,
    ) -> Result<federate_registry::TldRegistry> {
        let tld = &tld_rec.tld;
        let operator = &tld_rec.operator_public_key;
        match tld_rec.registry_type {
            RegistryType::DelegatedManifest => {
                let Some(hash) = tld_rec.registry_manifest_hash.as_deref() else {
                    return Err(FederateError::DelegatedRegistryUnavailable {
                        tld: tld.clone(),
                        reason: "TLD record declares delegated_manifest but carries no registry_manifest_hash".into(),
                    });
                };
                let bytes = self.manifest_bytes(hash).await.map_err(|e| match e {
                    // Tampering stays a hard failure; only reachability
                    // becomes the structured unavailable error.
                    FederateError::HashMismatch { .. }
                    | FederateError::VerificationFailed { .. } => e,
                    other => FederateError::DelegatedRegistryUnavailable {
                        tld: tld.clone(),
                        reason: format!("registry manifest {hash} not fetchable: {other}"),
                    },
                })?;
                let registry: federate_registry::TldRegistry = serde_json::from_slice(&bytes)
                    .map_err(|e| FederateError::VerificationFailed {
                        layer: "tld-registry".into(),
                        subject: format!(".{tld}"),
                        reason: format!("registry manifest is not a valid registry document: {e}"),
                    })?;
                registry.verify(tld, operator)?;
                trace(
                    t,
                    format!(
                        "registry for .{tld}: content-addressed registry manifest {} verified (operator-signed, v{}, {} domains)",
                        short(hash),
                        registry.version,
                        registry.domains.len()
                    ),
                );
                Ok(registry)
            }
            RegistryType::DelegatedNative | RegistryType::DelegatedHttp => {
                match self.fetch_registry_live(tld_rec, t).await {
                    Ok(registry) => {
                        if let Some(cached) = self.cached_registry(tld_rec) {
                            if registry.version < cached.version {
                                tracing::warn!(
                                    tld = %tld,
                                    fetched = registry.version,
                                    cached = cached.version,
                                    "REJECTED delegated registry older than the last verified one (possible replay); keeping the newer registry"
                                );
                                trace(
                                    t,
                                    format!(
                                        "registry for .{tld}: fetched v{} is OLDER than verified cache v{}; keeping cache (rollback protection)",
                                        registry.version, cached.version
                                    ),
                                );
                                return Ok(cached);
                            }
                        }
                        self.store_registry(&registry);
                        Ok(registry)
                    }
                    // Signature failures fail closed; never fall back past them.
                    Err(e @ FederateError::VerificationFailed { .. }) => Err(e),
                    Err(e) => {
                        if let Some(cached) = self.cached_registry(tld_rec) {
                            trace(
                                t,
                                format!(
                                    "registry for .{tld}: no provider reachable ({e}); using last verified cached registry v{}",
                                    cached.version
                                ),
                            );
                            return Ok(cached);
                        }
                        Err(e)
                    }
                }
            }
            other => Err(FederateError::DelegatedRegistryUnavailable {
                tld: tld.clone(),
                reason: format!("registry type {other:?} is not resolvable"),
            }),
        }
    }

    /// Verified signed registry of a delegated TLD by name: root zone →
    /// TLD record → registry. Relay surface for nodes answering
    /// `GetTldRegistry` and for CLI inspection; the registry returned here
    /// has already passed operator-signature verification.
    pub async fn tld_registry_by_name(&self, tld: &str) -> Result<federate_registry::TldRegistry> {
        let root = self.root().await?;
        let tld_rec = root
            .lookup_tld(tld)
            .ok_or_else(|| FederateError::TldNotFound { tld: tld.into() })?;
        self.tld_registry(tld_rec, None).await
    }

    /// Fetch a live delegated registry: the TLD's native registry providers
    /// first, then its HTTP compatibility endpoint. Every answer is
    /// operator-signature verified before it counts; a forged answer fails
    /// the whole fetch closed (it is not a reachability problem).
    async fn fetch_registry_live(
        &self,
        tld_rec: &federate_root::TldRecord,
        t: Option<&Trace>,
    ) -> Result<federate_registry::TldRegistry> {
        let tld = &tld_rec.tld;
        for addr in &tld_rec.registry_providers {
            match self
                .fetch_registry_native(addr, tld, &tld_rec.operator_public_key)
                .await
            {
                Ok(registry) => {
                    trace(
                        t,
                        format!(
                            "registry for .{tld}: fetched over NATIVE protocol from {addr}, operator signature verified (v{}, {} domains)",
                            registry.version,
                            registry.domains.len()
                        ),
                    );
                    return Ok(registry);
                }
                Err(e @ FederateError::VerificationFailed { .. }) => return Err(e),
                Err(e) => trace(
                    t,
                    format!("registry for .{tld}: native registry provider {addr} failed ({e}); trying next"),
                ),
            }
        }
        if let Some(endpoint) = tld_rec.registry_endpoint.as_deref() {
            match federate_registry::DelegatedRegistryClient::new(
                endpoint,
                &tld_rec.operator_public_key,
            )
            .fetch_registry(tld)
            .await
            {
                Ok(registry) => {
                    trace(
                        t,
                        format!(
                            "registry for .{tld}: fetched over HTTP compatibility from {endpoint}, operator signature verified (v{}, {} domains)",
                            registry.version,
                            registry.domains.len()
                        ),
                    );
                    return Ok(registry);
                }
                Err(e @ FederateError::VerificationFailed { .. }) => return Err(e),
                Err(e) => trace(
                    t,
                    format!("registry for .{tld}: http registry endpoint failed ({e})"),
                ),
            }
        }
        Err(FederateError::DelegatedRegistryUnavailable {
            tld: tld.clone(),
            reason: "no registry provider answered".into(),
        })
    }

    /// One native-protocol registry fetch. Requires a session negotiated at
    /// protocol v1 or newer (v0 peers do not know the registry messages).
    async fn fetch_registry_native(
        &self,
        addr: &str,
        tld: &str,
        operator_public_key: &str,
    ) -> Result<federate_registry::TldRegistry> {
        let (mut conn, _welcome) =
            federate_transport::Connection::connect(addr, &self.identity, AGENT).await?;
        if conn.version() < federate_protocol::VERSION_DELEGATED_REGISTRIES {
            return Err(FederateError::Network(format!(
                "provider speaks protocol v{} (< v{}); no delegated registry support",
                conn.version(),
                federate_protocol::VERSION_DELEGATED_REGISTRIES
            )));
        }
        match conn
            .request(&federate_protocol::Message::GetTldRegistry {
                tld: tld.to_string(),
            })
            .await?
        {
            federate_protocol::Message::TldRegistry { registry_json, .. } => {
                if registry_json.len() as u64 > federate_registry::MAX_REGISTRY_BYTES {
                    return Err(FederateError::Network(format!(
                        "registry exceeds {} bytes",
                        federate_registry::MAX_REGISTRY_BYTES
                    )));
                }
                let registry: federate_registry::TldRegistry =
                    serde_json::from_slice(&registry_json).map_err(|e| {
                        FederateError::Network(format!("provider sent malformed registry: {e}"))
                    })?;
                registry.verify(tld, operator_public_key)?;
                Ok(registry)
            }
            federate_protocol::Message::Error { code, detail } => Err(FederateError::Network(
                format!("registry provider answered {code:?}: {detail}"),
            )),
            other => Err(FederateError::Network(format!(
                "registry provider answered unexpectedly: {other:?}"
            ))),
        }
    }

    /// Cache path for a TLD's last verified registry. TLD names come from
    /// the verified root zone, but re-validate anyway so nothing unvetted
    /// ever forms a filesystem path.
    fn registry_cache_path(&self, tld: &str) -> Option<PathBuf> {
        federate_naming::validate_tld_name(tld)
            .ok()
            .map(|t| self.registries_dir.join(format!("{t}.json")))
    }

    /// Last verified cached registry for this TLD, re-verified on load
    /// against the CURRENT operator key from the root-signed TLD record: a
    /// registry cached under a rotated-out or revoked operator key is
    /// silently discarded.
    fn cached_registry(
        &self,
        tld_rec: &federate_root::TldRecord,
    ) -> Option<federate_registry::TldRegistry> {
        let bytes = std::fs::read(self.registry_cache_path(&tld_rec.tld)?).ok()?;
        let registry: federate_registry::TldRegistry = serde_json::from_slice(&bytes).ok()?;
        registry
            .verify(&tld_rec.tld, &tld_rec.operator_public_key)
            .ok()?;
        Some(registry)
    }

    /// Best-effort snapshot (write-then-rename so a crash never truncates
    /// the last good registry).
    fn store_registry(&self, registry: &federate_registry::TldRegistry) {
        let Some(path) = self.registry_cache_path(&registry.tld) else {
            return;
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(registry) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() {
                std::fs::rename(&tmp, &path).ok();
            }
        }
    }

    async fn block(&self, hash: &str) -> Result<Vec<u8>> {
        self.block_traced(hash, None).await
    }

    /// Block fetch with the provider preference order of the overlay:
    ///
    ///   1. local cache (hash re-verified on read)
    ///   2. native-protocol providers (directory-discovered, then defaults)
    ///   3. HTTP providers (CDN/storage/origin compatibility endpoints)
    ///   4. Node 1 over HTTP (compatibility fallback of last resort)
    ///
    /// Every source is untrusted: bytes count only after they hash to the
    /// requested content address. Transports carry bytes, not trust.
    async fn block_traced(&self, hash: &str, t: Option<&Trace>) -> Result<Vec<u8>> {
        // 1. Local cache. BlockStore::get re-verifies the hash and fails on
        // tampered cache; tampered entries are evicted for repair on refetch.
        match self.blocks.get(hash) {
            Ok(bytes) => {
                trace(
                    t,
                    format!("block {}: local cache hit (hash verified)", short(hash)),
                );
                return Ok(bytes);
            }
            Err(FederateError::HashMismatch { .. }) => {
                tracing::warn!("cached block {hash} failed hash validation; evicting");
                trace(
                    t,
                    format!("block {}: tampered cache entry evicted", short(hash)),
                );
            }
            Err(_) => trace(t, format!("block {}: not in local cache", short(hash))),
        }

        // Discover providers through the directory (advisory data only).
        let entries: Vec<federate_directory::NodeEntry> = match &self.directory {
            Some(dir) => dir.providers(hash, None).await.unwrap_or_default(),
            None => Vec::new(),
        };
        let ranked = federate_cdn::rank_providers(&entries, self.region.as_deref());

        // 2. Native pass: providers that speak the Federate protocol, best
        // ranked first, then the configured default native providers.
        let mut native: Vec<(String, String)> = ranked
            .iter()
            .filter_map(|p| {
                p.native_addr()
                    .map(|a| (p.registration.node_id.clone(), a.to_string()))
            })
            .collect();
        for addr in &self.native_providers {
            if !native.iter().any(|(_, a)| a == addr) {
                native.push(("default-provider".into(), addr.clone()));
            }
        }
        for (node_id, addr) in &native {
            match self.fetch_block_native(addr, hash).await {
                Ok(bytes) => {
                    self.blocks.put(hash, &bytes)?;
                    trace(
                        t,
                        format!(
                            "block {}: fetched over NATIVE protocol from {} at {addr}, hash verified",
                            short(hash),
                            short(node_id),
                        ),
                    );
                    return Ok(bytes);
                }
                Err(e) => {
                    tracing::debug!("native provider {node_id} ({addr}) failed for {hash}: {e}");
                    trace(
                        t,
                        format!(
                            "block {}: native provider {addr} failed ({e}); trying next",
                            short(hash)
                        ),
                    );
                }
            }
        }

        // 3. HTTP provider pass (compatibility surface of the same nodes).
        for provider in &ranked {
            match federate_cdn::fetch_block_from(provider, hash).await {
                Ok(bytes) => {
                    self.blocks.put(hash, &bytes)?;
                    trace(
                        t,
                        format!(
                            "block {}: fetched over HTTP compatibility from provider {}, hash verified",
                            short(hash),
                            short(&provider.registration.node_id),
                        ),
                    );
                    return Ok(bytes);
                }
                Err(e) => {
                    tracing::debug!(
                        "provider {} failed for block {hash}: {e}",
                        provider.registration.node_id
                    );
                    trace(
                        t,
                        format!(
                            "block {}: http provider failed ({e}); trying next",
                            short(hash)
                        ),
                    );
                }
            }
        }

        // 4. Node 1 over HTTP (origin of official content, last resort).
        trace(
            t,
            format!(
                "block {}: falling back to origin over HTTP compatibility",
                short(hash)
            ),
        );
        let bytes = self.client.fetch_block(hash).await?;
        self.blocks.put(hash, &bytes)?;
        trace(
            t,
            format!("block {}: fetched from origin, hash verified", short(hash)),
        );
        Ok(bytes)
    }

    /// One native-protocol block fetch: connect, handshake, GetBlock, verify
    /// the bytes against the content address. The provider is untrusted; a
    /// wrong-bytes answer fails verification here and the caller moves on.
    async fn fetch_block_native(&self, addr: &str, hash: &str) -> Result<Vec<u8>> {
        if !federate_storage::is_valid_hash(hash) {
            return Err(FederateError::BlockNotFound(hash.to_string()));
        }
        let (mut conn, _welcome) =
            federate_transport::Connection::connect(addr, &self.identity, AGENT).await?;
        match conn
            .request(&federate_protocol::Message::GetBlock {
                hash: hash.to_string(),
            })
            .await?
        {
            federate_protocol::Message::Block { bytes, .. } => {
                federate_storage::verify(&bytes, hash)?;
                Ok(bytes)
            }
            federate_protocol::Message::Error { code, detail } => Err(FederateError::Network(
                format!("native provider answered {code:?}: {detail}"),
            )),
            other => Err(FederateError::Network(format!(
                "native provider answered unexpectedly: {other:?}"
            ))),
        }
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
        let record = self
            .locate_record(&root, tld_rec, &domain.fqdn(), None)
            .await?;
        record.verify(&tld_rec.operator_public_key)?;
        if record.tld != domain.tld {
            return Err(FederateError::VerificationFailed {
                layer: "domain".into(),
                subject: domain.fqdn(),
                reason: "domain record belongs to a different TLD".into(),
            });
        }
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
        Ok(record)
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

    /// Resolve a native Federate URI. This is the canonical entry point:
    /// `fed://joao.pagina/about` and an HTTP request with Host
    /// `joao.pagina` + path `/about` resolve through exactly this call.
    pub async fn resolve_uri(&self, uri: &federate_uri::FederateUri) -> Result<Resolved> {
        self.resolve_traced_inner(&uri.fqdn(), &uri.path, None)
            .await
    }

    /// [`Resolver::resolve_uri`] with step-by-step diagnostics collected
    /// into `trace` (used by `federate fetch --trace`). Same verification,
    /// same outcomes; the trace only observes.
    pub async fn resolve_uri_traced(
        &self,
        uri: &federate_uri::FederateUri,
        trace: &Trace,
    ) -> Result<Resolved> {
        trace.push(format!(
            "uri parsed: {uri} (domain {}, path {})",
            uri.fqdn(),
            uri.path
        ));
        self.resolve_traced_inner(&uri.fqdn(), &uri.path, Some(trace))
            .await
    }

    /// Full verified content resolution by raw host + path. Prefer
    /// [`Resolver::resolve_uri`]; this exists for callers that already
    /// validated a `FederateUri` (its fields feed straight in) and for
    /// tolerant compatibility surfaces that want structured outcomes
    /// (`NotFederate`, ...) instead of parse errors.
    pub async fn resolve(&self, host: &str, path: &str) -> Result<Resolved> {
        self.resolve_traced_inner(host, path, None).await
    }

    async fn resolve_traced_inner(
        &self,
        host: &str,
        path: &str,
        t: Option<&Trace>,
    ) -> Result<Resolved> {
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

        trace(
            t,
            format!(
                "root zone v{} verified against pinned root key",
                root.root_version
            ),
        );

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

        trace(
            t,
            format!(
                "TLD .{} record verified (root-signed, status {})",
                domain.tld,
                tld_rec.status.as_str()
            ),
        );

        // 6-7. Domain record through the TLD's registry: the signed root
        // zone for root-managed TLDs, the operator's signed registry for
        // delegated TLDs. Same verification either way.
        let record = match self.locate_record(&root, tld_rec, &fqdn, t).await {
            Ok(r) => r,
            Err(FederateError::DomainNotFound(_)) => {
                return Ok(Resolved::DomainNotFound { domain: fqdn })
            }
            Err(FederateError::VerificationFailed { layer, reason, .. }) => {
                return Ok(Resolved::SecurityFailure {
                    domain: fqdn,
                    layer,
                    reason,
                })
            }
            Err(FederateError::HashMismatch { expected, actual }) => {
                return Ok(Resolved::SecurityFailure {
                    domain: fqdn,
                    layer: "tld-registry".into(),
                    reason: format!(
                        "registry manifest hash mismatch (expected {expected}, got {actual})"
                    ),
                })
            }
            Err(FederateError::DelegatedRegistryUnavailable { tld, reason }) => {
                return Ok(Resolved::DelegatedUnavailable {
                    domain: fqdn,
                    tld,
                    reason,
                })
            }
            Err(e) => return Err(e),
        };
        let record = &record;

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

        trace(
            t,
            format!("domain record for {fqdn} verified (signed by TLD operator key)"),
        );

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

        trace(
            t,
            format!(
                "manifest {} verified (owner-signed, {} files)",
                short(&record.manifest_hash),
                manifest.files.len()
            ),
        );

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
        trace(
            t,
            format!(
                "path {path} -> file '{file_name}' -> block {}",
                short(&file_hash)
            ),
        );
        let bytes = match self.block_traced(&file_hash, t).await {
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
            hash: file_hash,
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
            registry_providers: Vec::new(),
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

    // -----------------------------------------------------------------
    // Native provider fetching
    // -----------------------------------------------------------------

    /// Test-only native node serving a fixed set of blocks.
    struct BlockService(std::collections::HashMap<String, Vec<u8>>);

    #[federate_transport::async_trait]
    impl federate_transport::NodeService for BlockService {
        fn node_id(&self) -> String {
            "ff".repeat(32)
        }
        fn capabilities(&self) -> Vec<federate_protocol::Capability> {
            vec![federate_protocol::Capability::Blocks]
        }
        async fn handle(&self, req: federate_protocol::Message) -> federate_protocol::Message {
            match req {
                federate_protocol::Message::GetBlock { hash } => match self.0.get(&hash) {
                    Some(bytes) => federate_protocol::Message::Block {
                        hash,
                        bytes: bytes.clone(),
                    },
                    None => federate_protocol::Message::Error {
                        code: federate_protocol::ErrorCode::NotFound,
                        detail: "not held".into(),
                    },
                },
                _ => federate_protocol::Message::Error {
                    code: federate_protocol::ErrorCode::Unsupported,
                    detail: "blocks only".into(),
                },
            }
        }
    }

    /// Test-only native node serving the whole chain: signed root zone,
    /// content-addressed manifests, content blocks, and delegated TLD
    /// registries. What Node 1 (or any full node) looks like natively.
    #[derive(Default)]
    struct FullService {
        zone: Option<RootZone>,
        manifests: std::collections::HashMap<String, Vec<u8>>,
        blocks: std::collections::HashMap<String, Vec<u8>>,
        /// tld -> signed registry JSON bytes
        registries: std::collections::HashMap<String, Vec<u8>>,
    }

    #[federate_transport::async_trait]
    impl federate_transport::NodeService for FullService {
        fn node_id(&self) -> String {
            "aa".repeat(32)
        }
        fn capabilities(&self) -> Vec<federate_protocol::Capability> {
            vec![
                federate_protocol::Capability::Root,
                federate_protocol::Capability::Manifests,
                federate_protocol::Capability::Blocks,
                federate_protocol::Capability::TldRegistries,
            ]
        }
        async fn handle(&self, req: federate_protocol::Message) -> federate_protocol::Message {
            use federate_protocol::{ErrorCode, Message};
            let not_found = |detail: &str| Message::Error {
                code: ErrorCode::NotFound,
                detail: detail.into(),
            };
            match req {
                Message::GetRoot => match &self.zone {
                    Some(zone) => Message::Root {
                        zone_json: serde_json::to_vec(zone).unwrap(),
                    },
                    None => not_found("no zone here"),
                },
                Message::GetManifest { hash } => match self.manifests.get(&hash) {
                    Some(bytes) => Message::Manifest {
                        hash,
                        bytes: bytes.clone(),
                    },
                    None => not_found("no such manifest"),
                },
                Message::GetBlock { hash } => match self.blocks.get(&hash) {
                    Some(bytes) => Message::Block {
                        hash,
                        bytes: bytes.clone(),
                    },
                    None => not_found("no such block"),
                },
                Message::GetTldRegistry { tld } => match self.registries.get(&tld) {
                    Some(bytes) => Message::TldRegistry {
                        tld,
                        registry_json: bytes.clone(),
                    },
                    None => not_found("no registry for this TLD here"),
                },
                _ => Message::Error {
                    code: ErrorCode::Unsupported,
                    detail: "root/manifests/blocks/registries only".into(),
                },
            }
        }
    }

    /// A hostile native node: answers every GetBlock with wrong bytes.
    struct LiarService;

    #[federate_transport::async_trait]
    impl federate_transport::NodeService for LiarService {
        fn node_id(&self) -> String {
            "bd".repeat(32)
        }
        fn capabilities(&self) -> Vec<federate_protocol::Capability> {
            vec![federate_protocol::Capability::Blocks]
        }
        async fn handle(&self, req: federate_protocol::Message) -> federate_protocol::Message {
            match req {
                federate_protocol::Message::GetBlock { hash } => {
                    federate_protocol::Message::Block {
                        hash,
                        bytes: b"forged bytes that do not match the hash".to_vec(),
                    }
                }
                _ => federate_protocol::Message::Error {
                    code: federate_protocol::ErrorCode::Unsupported,
                    detail: "blocks only".into(),
                },
            }
        }
    }

    async fn spawn_native(service: impl federate_transport::NodeService) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(federate_transport::serve(
            listener,
            std::sync::Arc::new(service),
            "test-node/0".into(),
        ));
        addr
    }

    /// Build a fully resolvable site set offline: signed zone (cached),
    /// owner-signed manifests (cached), one distinct content block per fqdn.
    /// Returns (data_dir, keys, fqdn -> (block_hash, block_bytes)).
    fn content_sites(
        dir: &Path,
        fqdns: &[&str],
    ) -> (
        PathBuf,
        std::collections::HashMap<String, (String, Vec<u8>)>,
    ) {
        let keys = keys(dir);
        let owner = NodeIdentity::load_or_create(&dir.join("owner")).unwrap();
        let data_dir = dir.join("data");
        let manifest_dir = data_dir.join("manifests");
        std::fs::create_dir_all(&manifest_dir).unwrap();

        let mut tlds = BTreeMap::new();
        let mut domains = BTreeMap::new();
        let mut blocks = std::collections::HashMap::new();
        for fqdn in fqdns {
            let (label, tld) = fqdn.split_once('.').unwrap();
            tlds.entry(tld.to_string())
                .or_insert_with(|| signed_tld(&keys, tld));

            let block = format!("<html>content of {fqdn}</html>").into_bytes();
            let block_hash = federate_storage::hash_bytes(&block);

            let mut manifest = Manifest {
                domain: fqdn.to_string(),
                version: 1,
                entry: "index.html".into(),
                files: BTreeMap::from([("index.html".to_string(), block_hash.clone())]),
                owner_public_key: owner.node_id(),
                created_at: "t".into(),
                signature_algorithm: SIGNATURE_ALGORITHM.into(),
                signature: None,
            };
            manifest.signature = Some(owner.sign(&manifest.signable_bytes().unwrap()));
            let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
            let manifest_hash = federate_storage::hash_bytes(&manifest_bytes);
            std::fs::write(manifest_dir.join(&manifest_hash), &manifest_bytes).unwrap();

            let mut rec = DomainRecord {
                domain: fqdn.to_string(),
                tld: tld.into(),
                label: label.into(),
                owner_public_key: owner.node_id(),
                target_type: TargetType::Manifest,
                manifest_hash,
                service_id: None,
                node_id: None,
                status: DomainStatus::Active,
                created_at: "t".into(),
                updated_at: "t".into(),
                expires_at: None,
                renewal: None,
                pricing: None,
                signature_algorithm: SIGNATURE_ALGORITHM.into(),
                signature: None,
            };
            rec.signature = Some(keys.operator.sign(&rec.signable_bytes().unwrap()));
            domains.insert(fqdn.to_string(), rec);
            blocks.insert(fqdn.to_string(), (block_hash, block));
        }

        let mut zone = RootZone {
            network: "federate".into(),
            root_version: 1,
            generated_at: "t".into(),
            root_public_key: keys.root.node_id(),
            tlds,
            domains,
            audit: vec![],
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(keys.root.sign(&zone.signable_bytes().unwrap()));
        RootCache::new(&data_dir).store(&zone).unwrap();
        std::fs::write(data_dir.join("trusted-root-key"), keys.root.node_id()).unwrap();
        (data_dir, blocks)
    }

    /// Native provider preferred, generically for multiple TLDs. Bootstrap
    /// is unreachable, so HTTP fallback CANNOT be the source: success proves
    /// the bytes came over the native protocol.
    #[tokio::test]
    async fn native_provider_serves_blocks_for_multiple_tlds() {
        let dir = tmp("native-multi");
        let fqdns = ["home.fed", "joao.pagina", "fotolia.rosa"];
        let (data_dir, blocks) = content_sites(&dir, &fqdns);

        let all: std::collections::HashMap<String, Vec<u8>> = blocks
            .values()
            .map(|(h, b)| (h.clone(), b.clone()))
            .collect();
        let provider = spawn_native(BlockService(all)).await;

        let resolver = Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None)
            .unwrap()
            .with_native_providers(vec![provider.to_string()]);

        for fqdn in fqdns {
            let uri = federate_uri::FederateUri::parse(&format!("fed://{fqdn}")).unwrap();
            let t = Trace::default();
            match resolver.resolve_uri_traced(&uri, &t).await.unwrap() {
                Resolved::Content { bytes, .. } => {
                    assert_eq!(bytes, blocks[fqdn].1, "{fqdn} content");
                }
                other => panic!("{fqdn} must resolve to content, got {other:?}"),
            }
            let log = t.events().join("\n");
            assert!(
                log.contains("NATIVE protocol"),
                "{fqdn} must fetch natively:\n{log}"
            );
            assert!(log.contains("manifest"), "trace records manifest step");
            assert!(log.contains("-> block"), "trace records selected block");
        }
    }

    /// The full resolution chain with NO HTTP anywhere: root zone, manifest,
    /// and block all arrive over the native Federate protocol from a peer.
    /// The bootstrap HTTP endpoint is unreachable and nothing is cached, so
    /// success proves the network can run natively end to end.
    #[tokio::test]
    async fn entire_chain_resolves_over_native_protocol_only() {
        let dir = tmp("native-e2e");
        let keys = keys(&dir);
        let owner = NodeIdentity::load_or_create(&dir.join("owner")).unwrap();

        let block = b"<html>served with zero HTTP</html>".to_vec();
        let block_hash = federate_storage::hash_bytes(&block);
        let mut manifest = Manifest {
            domain: "puro.fed".into(),
            version: 1,
            entry: "index.html".into(),
            files: BTreeMap::from([("index.html".to_string(), block_hash.clone())]),
            owner_public_key: owner.node_id(),
            created_at: "t".into(),
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        manifest.signature = Some(owner.sign(&manifest.signable_bytes().unwrap()));
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let manifest_hash = federate_storage::hash_bytes(&manifest_bytes);

        let mut record = signed_domain(&keys, "puro.fed", None);
        record.owner_public_key = owner.node_id();
        record.manifest_hash = manifest_hash.clone();
        record.signature = Some(keys.operator.sign(&record.signable_bytes().unwrap()));
        let zone = signed_zone(&keys, 1, BTreeMap::from([("puro.fed".to_string(), record)]));

        let provider = spawn_native(FullService {
            zone: Some(zone),
            manifests: std::collections::HashMap::from([(manifest_hash, manifest_bytes)]),
            blocks: std::collections::HashMap::from([(block_hash, block.clone())]),
            ..Default::default()
        })
        .await;

        // Fresh data dir: no cached zone, no cached manifest, no cached block.
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let resolver = Resolver::new(
            NodeClient::new("http://127.0.0.1:1"),
            &data_dir,
            Some(keys.root.node_id()),
        )
        .unwrap()
        .with_native_providers(vec![provider.to_string()]);

        let zone = resolver.refresh_root().await.unwrap();
        assert_eq!(zone.root_version, 1, "root zone arrived natively");

        let uri = federate_uri::FederateUri::parse("fed://puro.fed").unwrap();
        let t = Trace::default();
        match resolver.resolve_uri_traced(&uri, &t).await.unwrap() {
            Resolved::Content { bytes, .. } => assert_eq!(bytes, block),
            other => panic!("expected content, got {other:?}"),
        }
        let log = t.events().join("\n");
        assert!(log.contains("NATIVE protocol"), "block natively:\n{log}");
    }

    /// A native provider serving forged bytes is rejected by hash
    /// verification; the next provider answers correctly.
    #[tokio::test]
    async fn forged_native_block_rejected_next_provider_wins() {
        let dir = tmp("native-liar");
        let (data_dir, blocks) = content_sites(&dir, &["site.mosca"]);
        let (hash, bytes) = blocks["site.mosca"].clone();

        let liar = spawn_native(LiarService).await;
        let honest = spawn_native(BlockService(std::collections::HashMap::from([(
            hash,
            bytes.clone(),
        )])))
        .await;

        let resolver = Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None)
            .unwrap()
            .with_native_providers(vec![liar.to_string(), honest.to_string()]);

        let uri = federate_uri::FederateUri::parse("fed://site.mosca").unwrap();
        let t = Trace::default();
        match resolver.resolve_uri_traced(&uri, &t).await.unwrap() {
            Resolved::Content { bytes: got, .. } => assert_eq!(got, bytes),
            other => panic!("expected content, got {other:?}"),
        }
        let log = t.events().join("\n");
        assert!(
            log.contains("failed (hash mismatch") || log.contains("failed"),
            "liar must be rejected in trace:\n{log}"
        );
        assert!(
            log.contains("NATIVE protocol"),
            "honest native provider used"
        );
    }

    /// Dead native providers (connection refused) fall through to the HTTP
    /// compatibility origin.
    #[tokio::test]
    async fn dead_native_providers_fall_back_to_http_origin() {
        let dir = tmp("native-fallback");
        let (data_dir, blocks) = content_sites(&dir, &["fed.busca"]);
        let (hash, bytes) = blocks["fed.busca"].clone();

        // HTTP origin serving the block (compatibility surface).
        let app = axum::Router::new().route(
            "/v1/block/:h",
            axum::routing::get(move |axum::extract::Path(h): axum::extract::Path<String>| {
                let (hash, bytes) = (hash.clone(), bytes.clone());
                async move {
                    if h == hash {
                        Ok(bytes)
                    } else {
                        Err(axum::http::StatusCode::NOT_FOUND)
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        // A closed port: instant connection-refused native failure.
        let dead = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };

        let resolver = Resolver::new(NodeClient::new(&origin), &data_dir, None)
            .unwrap()
            .with_native_providers(vec![dead.to_string()]);

        let uri = federate_uri::FederateUri::parse("fed://fed.busca").unwrap();
        let t = Trace::default();
        match resolver.resolve_uri_traced(&uri, &t).await.unwrap() {
            Resolved::Content { bytes: got, .. } => {
                assert_eq!(got, blocks["fed.busca"].1);
            }
            other => panic!("expected content via HTTP fallback, got {other:?}"),
        }
        let log = t.events().join("\n");
        assert!(
            log.contains("native provider"),
            "dead native attempt traced:\n{log}"
        );
        assert!(
            log.contains("origin over HTTP compatibility"),
            "fallback traced:\n{log}"
        );
    }

    /// Local cache beats every network source.
    #[tokio::test]
    async fn local_cache_wins_before_any_network() {
        let dir = tmp("cache-first");
        let (data_dir, blocks) = content_sites(&dir, &["alguem.cara"]);
        let (hash, bytes) = blocks["alguem.cara"].clone();

        let dead = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let resolver = Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None)
            .unwrap()
            .with_native_providers(vec![dead.to_string()]);
        // Pre-seed the verified local cache.
        resolver.block_store().put(&hash, &bytes).unwrap();

        let uri = federate_uri::FederateUri::parse("fed://alguem.cara").unwrap();
        let t = Trace::default();
        match resolver.resolve_uri_traced(&uri, &t).await.unwrap() {
            Resolved::Content { bytes: got, .. } => assert_eq!(got, bytes),
            other => panic!("expected cached content, got {other:?}"),
        }
        let log = t.events().join("\n");
        assert!(log.contains("local cache hit"), "cache must win:\n{log}");
        assert!(
            !log.contains("NATIVE protocol"),
            "no network needed:\n{log}"
        );
    }

    /// The engine must treat every TLD generically: nothing anywhere may
    /// special-case `home.fed`. A zone with domains under several official
    /// TLDs plus one delegated TLD resolves each through the same path.
    #[tokio::test]
    async fn any_domain_under_any_tld_resolves_generically() {
        let dir = tmp("multi-tld");
        let keys = keys(&dir);

        let mut tlds = BTreeMap::new();
        let mut domains = BTreeMap::new();
        for (label, tld) in [
            ("home", "fed"),
            ("joao", "pagina"),
            ("fotolia", "rosa"),
            ("arcade", "mosca"),
            ("alguem", "cara"),
            ("fed", "busca"),
        ] {
            tlds.insert(tld.to_string(), signed_tld(&keys, tld));
            let fqdn = format!("{label}.{tld}");
            domains.insert(fqdn.clone(), signed_domain(&keys, &fqdn, None));
        }
        // A delegated TLD with no reachable registry provider anywhere:
        // resolution must answer with the structured unavailable outcome.
        let mut delegated = signed_tld(&keys, "femboy");
        delegated.registry_type = federate_naming::RegistryType::DelegatedHttp;
        delegated.registry_endpoint = Some("http://127.0.0.1:1".into());
        delegated.signature = Some(keys.root.sign(&delegated.signable_bytes().unwrap()));
        tlds.insert("femboy".into(), delegated);

        let mut zone = RootZone {
            network: "federate".into(),
            root_version: 1,
            generated_at: "t".into(),
            root_public_key: keys.root.node_id(),
            tlds,
            domains,
            audit: vec![],
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(keys.root.sign(&zone.signable_bytes().unwrap()));

        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        RootCache::new(&data_dir).store(&zone).unwrap();
        let resolver = Resolver::new(
            NodeClient::new("http://127.0.0.1:1"),
            &data_dir,
            Some(keys.root.node_id()),
        )
        .unwrap();

        for fqdn in [
            "home.fed",
            "joao.pagina",
            "fotolia.rosa",
            "arcade.mosca",
            "alguem.cara",
            "fed.busca",
        ] {
            // Native URI path: record resolves (content fetch would need the
            // network; the record layer proves the generic chain).
            let uri = federate_uri::FederateUri::parse(&format!("fed://{fqdn}")).unwrap();
            assert!(
                resolver.resolve_domain(&uri.fqdn()).await.is_ok(),
                "{fqdn} must resolve generically"
            );
        }

        // Delegated TLD whose registry is unreachable (and not cached):
        // structured unavailable outcome, not a panic, not silent success.
        let uri = federate_uri::FederateUri::parse("fed://store.femboy").unwrap();
        assert!(matches!(
            resolver.resolve_uri(&uri).await.unwrap(),
            Resolved::DelegatedUnavailable { tld, .. } if tld == "femboy"
        ));

        // Unknown TLD still fails cleanly through the same path.
        let uri = federate_uri::FederateUri::parse("fed://x.nowhere").unwrap();
        assert!(matches!(
            resolver.resolve_uri(&uri).await.unwrap(),
            Resolved::TldNotFound { .. }
        ));
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

    // -----------------------------------------------------------------
    // Delegated TLD registries
    // -----------------------------------------------------------------

    /// Operator-signed domain record for a delegated TLD.
    fn op_record(
        operator: &NodeIdentity,
        fqdn: &str,
        owner_pk: &str,
        manifest_hash: &str,
        expires_at: Option<String>,
    ) -> DomainRecord {
        let (label, tld) = fqdn.split_once('.').unwrap();
        let mut rec = DomainRecord {
            domain: fqdn.into(),
            tld: tld.into(),
            label: label.into(),
            owner_public_key: owner_pk.into(),
            target_type: TargetType::Manifest,
            manifest_hash: manifest_hash.into(),
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
        rec.signature = Some(operator.sign(&rec.signable_bytes().unwrap()));
        rec
    }

    /// Owner-signed one-page site: (manifest_bytes, manifest_hash, block_hash, block).
    fn owner_site(owner: &NodeIdentity, fqdn: &str) -> (Vec<u8>, String, String, Vec<u8>) {
        let block = format!("<html>delegated content of {fqdn}</html>").into_bytes();
        let block_hash = federate_storage::hash_bytes(&block);
        let mut manifest = Manifest {
            domain: fqdn.into(),
            version: 1,
            entry: "index.html".into(),
            files: BTreeMap::from([("index.html".to_string(), block_hash.clone())]),
            owner_public_key: owner.node_id(),
            created_at: "t".into(),
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        manifest.signature = Some(owner.sign(&manifest.signable_bytes().unwrap()));
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let manifest_hash = federate_storage::hash_bytes(&manifest_bytes);
        (manifest_bytes, manifest_hash, block_hash, block)
    }

    /// Root-signed TLD record delegating `tld` to `operator`.
    fn delegated_tld_record(
        keys: &Keys,
        operator: &NodeIdentity,
        tld: &str,
        registry_type: federate_naming::RegistryType,
    ) -> federate_root::TldRecord {
        let mut rec = signed_tld(keys, tld);
        rec.status = TldStatus::Delegated;
        rec.mode = TldMode::Delegated;
        rec.owner_public_key = operator.node_id();
        rec.operator_public_key = operator.node_id();
        rec.registry_type = registry_type;
        rec.signature = Some(keys.root.sign(&rec.signable_bytes().unwrap()));
        rec
    }

    /// Signed zone containing one delegated TLD plus the official `.fed`.
    fn zone_with_tld(keys: &Keys, tld_rec: federate_root::TldRecord) -> RootZone {
        let mut tlds = BTreeMap::new();
        tlds.insert("fed".to_string(), signed_tld(keys, "fed"));
        tlds.insert(tld_rec.tld.clone(), tld_rec);
        let mut zone = RootZone {
            network: "federate".into(),
            root_version: 1,
            generated_at: "t".into(),
            root_public_key: keys.root.node_id(),
            tlds,
            domains: BTreeMap::new(),
            audit: vec![],
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(keys.root.sign(&zone.signable_bytes().unwrap()));
        zone
    }

    fn fresh_data_dir(dir: &Path, keys: &Keys, zone: &RootZone) -> PathBuf {
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        RootCache::new(&data_dir).store(zone).unwrap();
        std::fs::write(data_dir.join("trusted-root-key"), keys.root.node_id()).unwrap();
        data_dir
    }

    /// delegated_manifest mode, end to end: the registry travels as a
    /// content-addressed manifest pinned by the root-signed TLD record; the
    /// domain resolves through the delegated path with HTTP dead. Uses a
    /// non-seed TLD name, so nothing anywhere special-cases the demo TLD.
    #[tokio::test]
    async fn delegated_manifest_tld_resolves_end_to_end() {
        let dir = tmp("dlg-manifest");
        let keys = keys(&dir);
        let operator = NodeIdentity::load_or_create(&dir.join("op")).unwrap();
        let owner = NodeIdentity::load_or_create(&dir.join("owner")).unwrap();

        let (manifest_bytes, manifest_hash, block_hash, block) = owner_site(&owner, "eu.livraria");
        let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let registry = federate_registry::TldRegistry::signed(
            &operator,
            "livraria",
            1,
            "t",
            BTreeMap::from([
                (
                    "eu.livraria".to_string(),
                    op_record(
                        &operator,
                        "eu.livraria",
                        &owner.node_id(),
                        &manifest_hash,
                        None,
                    ),
                ),
                (
                    "velho.livraria".to_string(),
                    op_record(
                        &operator,
                        "velho.livraria",
                        &owner.node_id(),
                        &manifest_hash,
                        Some(past),
                    ),
                ),
            ]),
        )
        .unwrap();
        let registry_bytes = serde_json::to_vec(&registry).unwrap();
        let registry_hash = federate_storage::hash_bytes(&registry_bytes);

        let mut tld_rec = delegated_tld_record(
            &keys,
            &operator,
            "livraria",
            federate_naming::RegistryType::DelegatedManifest,
        );
        tld_rec.registry_manifest_hash = Some(registry_hash.clone());
        tld_rec.signature = Some(keys.root.sign(&tld_rec.signable_bytes().unwrap()));
        let zone = zone_with_tld(&keys, tld_rec);
        let data_dir = fresh_data_dir(&dir, &keys, &zone);

        // Registry manifest, site manifest, and block all served natively.
        let provider = spawn_native(FullService {
            manifests: std::collections::HashMap::from([
                (registry_hash, registry_bytes),
                (manifest_hash, manifest_bytes),
            ]),
            blocks: std::collections::HashMap::from([(block_hash, block.clone())]),
            ..Default::default()
        })
        .await;

        let resolver = Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None)
            .unwrap()
            .with_native_providers(vec![provider.to_string()]);

        let uri = federate_uri::FederateUri::parse("fed://eu.livraria").unwrap();
        let t = Trace::default();
        match resolver.resolve_uri_traced(&uri, &t).await.unwrap() {
            Resolved::Content { bytes, .. } => assert_eq!(bytes, block),
            other => panic!("expected delegated content, got {other:?}"),
        }
        let log = t.events().join("\n");
        assert!(
            log.contains("registry for .livraria: content-addressed registry manifest"),
            "delegated registry step traced:\n{log}"
        );

        // Unknown domain under the delegated TLD: normal domain-not-found.
        assert!(matches!(
            resolver.resolve("nao.livraria", "/").await.unwrap(),
            Resolved::DomainNotFound { .. }
        ));
        // Expired delegated domain: fail closed even though the signature
        // is valid.
        match resolver.resolve("velho.livraria", "/").await.unwrap() {
            Resolved::DomainUnavailable { status, .. } => assert_eq!(status, "expired"),
            other => panic!("expired delegated domain must not resolve, got {other:?}"),
        }
    }

    /// delegated_native mode: the registry itself arrives over the native
    /// protocol (GetTldRegistry) from a provider listed in the root-signed
    /// TLD record.
    #[tokio::test]
    async fn delegated_native_registry_resolves_end_to_end() {
        let dir = tmp("dlg-native");
        let keys = keys(&dir);
        let operator = NodeIdentity::load_or_create(&dir.join("op")).unwrap();
        let owner = NodeIdentity::load_or_create(&dir.join("owner")).unwrap();

        let (manifest_bytes, manifest_hash, block_hash, block) = owner_site(&owner, "eu.copo");
        let registry = federate_registry::TldRegistry::signed(
            &operator,
            "copo",
            1,
            "t",
            BTreeMap::from([(
                "eu.copo".to_string(),
                op_record(&operator, "eu.copo", &owner.node_id(), &manifest_hash, None),
            )]),
        )
        .unwrap();

        let provider = spawn_native(FullService {
            manifests: std::collections::HashMap::from([(manifest_hash, manifest_bytes)]),
            blocks: std::collections::HashMap::from([(block_hash, block.clone())]),
            registries: std::collections::HashMap::from([(
                "copo".to_string(),
                serde_json::to_vec(&registry).unwrap(),
            )]),
            ..Default::default()
        })
        .await;

        let mut tld_rec = delegated_tld_record(
            &keys,
            &operator,
            "copo",
            federate_naming::RegistryType::DelegatedNative,
        );
        tld_rec.registry_providers = vec![provider.to_string()];
        tld_rec.signature = Some(keys.root.sign(&tld_rec.signable_bytes().unwrap()));
        let zone = zone_with_tld(&keys, tld_rec);
        let data_dir = fresh_data_dir(&dir, &keys, &zone);

        let resolver = Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None)
            .unwrap()
            .with_native_providers(vec![provider.to_string()]);

        let uri = federate_uri::FederateUri::parse("fed://eu.copo").unwrap();
        let t = Trace::default();
        match resolver.resolve_uri_traced(&uri, &t).await.unwrap() {
            Resolved::Content { bytes, .. } => assert_eq!(bytes, block),
            other => panic!("expected delegated content, got {other:?}"),
        }
        let log = t.events().join("\n");
        assert!(
            log.contains("registry for .copo: fetched over NATIVE protocol"),
            "registry must arrive natively:\n{log}"
        );
        // The live registry is now cached on disk for offline fallback.
        assert!(data_dir.join("registries").join("copo.json").exists());
    }

    /// Registry providers all dead: the last verified cached registry keeps
    /// delegated domains resolving; a tampered cache is discarded and the
    /// outcome degrades to the structured unavailable answer.
    #[tokio::test]
    async fn delegated_native_offline_uses_verified_cache_rejects_tampered() {
        let dir = tmp("dlg-offline");
        let keys = keys(&dir);
        let operator = NodeIdentity::load_or_create(&dir.join("op")).unwrap();
        let owner = NodeIdentity::load_or_create(&dir.join("owner")).unwrap();

        let (_, manifest_hash, _, _) = owner_site(&owner, "eu.arquivo");
        let registry = federate_registry::TldRegistry::signed(
            &operator,
            "arquivo",
            3,
            "t",
            BTreeMap::from([(
                "eu.arquivo".to_string(),
                op_record(
                    &operator,
                    "eu.arquivo",
                    &owner.node_id(),
                    &manifest_hash,
                    None,
                ),
            )]),
        )
        .unwrap();

        let dead = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let mut tld_rec = delegated_tld_record(
            &keys,
            &operator,
            "arquivo",
            federate_naming::RegistryType::DelegatedNative,
        );
        tld_rec.registry_providers = vec![dead.to_string()];
        tld_rec.signature = Some(keys.root.sign(&tld_rec.signable_bytes().unwrap()));
        let zone = zone_with_tld(&keys, tld_rec);
        let data_dir = fresh_data_dir(&dir, &keys, &zone);

        // Pre-seed the verified registry cache (as if fetched earlier).
        let reg_dir = data_dir.join("registries");
        std::fs::create_dir_all(&reg_dir).unwrap();
        std::fs::write(
            reg_dir.join("arquivo.json"),
            serde_json::to_vec(&registry).unwrap(),
        )
        .unwrap();

        let resolver =
            Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None).unwrap();

        // Record layer resolves offline from the verified cached registry.
        assert!(resolver.resolve_domain("eu.arquivo").await.is_ok());
        let t = Trace::default();
        let uri = federate_uri::FederateUri::parse("fed://nao.arquivo").unwrap();
        assert!(matches!(
            resolver.resolve_uri_traced(&uri, &t).await.unwrap(),
            Resolved::DomainNotFound { .. }
        ));
        assert!(
            t.events()
                .join("\n")
                .contains("using last verified cached registry"),
            "offline fallback traced"
        );

        // Tamper the cached registry: it must be discarded, and with every
        // provider dead the outcome is the structured unavailable answer.
        std::fs::write(reg_dir.join("arquivo.json"), b"{\"not\": \"a registry\"}").unwrap();
        match resolver.resolve("eu.arquivo", "/").await.unwrap() {
            Resolved::DelegatedUnavailable { tld, .. } => assert_eq!(tld, "arquivo"),
            other => panic!("tampered cache must not resolve, got {other:?}"),
        }
    }

    /// A provider replaying an OLDER (correctly signed) registry cannot
    /// rewind the namespace: the newer verified cached registry wins.
    #[tokio::test]
    async fn delegated_registry_rollback_rejected() {
        let dir = tmp("dlg-rollback");
        let keys = keys(&dir);
        let operator = NodeIdentity::load_or_create(&dir.join("op")).unwrap();
        let owner = NodeIdentity::load_or_create(&dir.join("owner")).unwrap();
        let (_, manifest_hash, _, _) = owner_site(&owner, "novo.selo");

        let newer = federate_registry::TldRegistry::signed(
            &operator,
            "selo",
            5,
            "t",
            BTreeMap::from([(
                "novo.selo".to_string(),
                op_record(
                    &operator,
                    "novo.selo",
                    &owner.node_id(),
                    &manifest_hash,
                    None,
                ),
            )]),
        )
        .unwrap();
        // Older registry (before novo.selo existed).
        let older =
            federate_registry::TldRegistry::signed(&operator, "selo", 3, "t", BTreeMap::new())
                .unwrap();

        let provider = spawn_native(FullService {
            registries: std::collections::HashMap::from([(
                "selo".to_string(),
                serde_json::to_vec(&older).unwrap(),
            )]),
            ..Default::default()
        })
        .await;

        let mut tld_rec = delegated_tld_record(
            &keys,
            &operator,
            "selo",
            federate_naming::RegistryType::DelegatedNative,
        );
        tld_rec.registry_providers = vec![provider.to_string()];
        tld_rec.signature = Some(keys.root.sign(&tld_rec.signable_bytes().unwrap()));
        let zone = zone_with_tld(&keys, tld_rec);
        let data_dir = fresh_data_dir(&dir, &keys, &zone);

        // The newer registry was verified earlier (cached).
        let reg_dir = data_dir.join("registries");
        std::fs::create_dir_all(&reg_dir).unwrap();
        std::fs::write(
            reg_dir.join("selo.json"),
            serde_json::to_vec(&newer).unwrap(),
        )
        .unwrap();

        let resolver =
            Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None).unwrap();
        // novo.selo only exists in the newer registry: the replayed older
        // one must not displace it.
        assert!(
            resolver.resolve_domain("novo.selo").await.is_ok(),
            "replayed older registry must not rewind the namespace"
        );
    }

    /// Forged registries fail closed: wrong claimed key, wrong signature.
    /// Never a fallback, never content.
    #[tokio::test]
    async fn forged_delegated_registry_fails_closed() {
        let dir = tmp("dlg-forged");
        let keys = keys(&dir);
        let operator = NodeIdentity::load_or_create(&dir.join("op")).unwrap();
        let attacker = NodeIdentity::load_or_create(&dir.join("atk")).unwrap();
        let owner = NodeIdentity::load_or_create(&dir.join("owner")).unwrap();
        let (_, manifest_hash, _, _) = owner_site(&owner, "eu.forja");

        // Signed by the attacker key end to end: valid document, wrong key.
        let forged = federate_registry::TldRegistry::signed(
            &attacker,
            "forja",
            9,
            "t",
            BTreeMap::from([(
                "eu.forja".to_string(),
                op_record(
                    &attacker,
                    "eu.forja",
                    &owner.node_id(),
                    &manifest_hash,
                    None,
                ),
            )]),
        )
        .unwrap();

        let provider = spawn_native(FullService {
            registries: std::collections::HashMap::from([(
                "forja".to_string(),
                serde_json::to_vec(&forged).unwrap(),
            )]),
            ..Default::default()
        })
        .await;

        let mut tld_rec = delegated_tld_record(
            &keys,
            &operator,
            "forja",
            federate_naming::RegistryType::DelegatedNative,
        );
        tld_rec.registry_providers = vec![provider.to_string()];
        tld_rec.signature = Some(keys.root.sign(&tld_rec.signable_bytes().unwrap()));
        let zone = zone_with_tld(&keys, tld_rec);
        let data_dir = fresh_data_dir(&dir, &keys, &zone);

        let resolver =
            Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None).unwrap();
        match resolver.resolve("eu.forja", "/").await.unwrap() {
            Resolved::SecurityFailure { layer, .. } => assert_eq!(layer, "tld-registry"),
            other => panic!("forged registry must fail closed, got {other:?}"),
        }
    }

    /// A correctly signed registry whose domain record is signed by the
    /// wrong key fails closed at the record layer.
    #[tokio::test]
    async fn delegated_domain_record_wrong_signer_fails_closed() {
        let dir = tmp("dlg-wrongsigner");
        let keys = keys(&dir);
        let operator = NodeIdentity::load_or_create(&dir.join("op")).unwrap();
        let attacker = NodeIdentity::load_or_create(&dir.join("atk")).unwrap();
        let owner = NodeIdentity::load_or_create(&dir.join("owner")).unwrap();
        let (_, manifest_hash, _, _) = owner_site(&owner, "eu.vitral");

        // Registry properly operator-signed, but the record inside is
        // attacker-signed.
        let registry = federate_registry::TldRegistry::signed(
            &operator,
            "vitral",
            1,
            "t",
            BTreeMap::from([(
                "eu.vitral".to_string(),
                op_record(
                    &attacker,
                    "eu.vitral",
                    &owner.node_id(),
                    &manifest_hash,
                    None,
                ),
            )]),
        )
        .unwrap();

        let provider = spawn_native(FullService {
            registries: std::collections::HashMap::from([(
                "vitral".to_string(),
                serde_json::to_vec(&registry).unwrap(),
            )]),
            ..Default::default()
        })
        .await;

        let mut tld_rec = delegated_tld_record(
            &keys,
            &operator,
            "vitral",
            federate_naming::RegistryType::DelegatedNative,
        );
        tld_rec.registry_providers = vec![provider.to_string()];
        tld_rec.signature = Some(keys.root.sign(&tld_rec.signable_bytes().unwrap()));
        let zone = zone_with_tld(&keys, tld_rec);
        let data_dir = fresh_data_dir(&dir, &keys, &zone);

        let resolver =
            Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None).unwrap();
        match resolver.resolve("eu.vitral", "/").await.unwrap() {
            Resolved::SecurityFailure { layer, .. } => assert_eq!(layer, "domain"),
            other => panic!("wrong-signer record must fail closed, got {other:?}"),
        }
    }

    /// Expired, revoked, and disabled delegated TLDs fail closed before any
    /// registry fetch is even attempted.
    #[tokio::test]
    async fn delegated_tld_bad_status_or_expiry_fails_closed() {
        let dir = tmp("dlg-status");
        let keys = keys(&dir);
        let operator = NodeIdentity::load_or_create(&dir.join("op")).unwrap();

        for (name, status, expires_at, expected) in [
            ("revogado", TldStatus::Revoked, None, "revoked"),
            ("desligado", TldStatus::Disabled, None, "disabled"),
            ("vencido", TldStatus::Expired, None, "expired"),
            (
                "lapso",
                TldStatus::Delegated,
                Some((chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339()),
                "expired",
            ),
        ] {
            let case_dir = dir.join(name);
            std::fs::create_dir_all(&case_dir).unwrap();
            let mut tld_rec = delegated_tld_record(
                &keys,
                &operator,
                name,
                federate_naming::RegistryType::DelegatedNative,
            );
            tld_rec.status = status;
            tld_rec.expires_at = expires_at;
            tld_rec.signature = Some(keys.root.sign(&tld_rec.signable_bytes().unwrap()));
            let zone = zone_with_tld(&keys, tld_rec);
            let data_dir = fresh_data_dir(&case_dir, &keys, &zone);

            let resolver =
                Resolver::new(NodeClient::new("http://127.0.0.1:1"), &data_dir, None).unwrap();
            match resolver.resolve(&format!("eu.{name}"), "/").await.unwrap() {
                Resolved::TldUnavailable { status, .. } => {
                    assert_eq!(status, expected, ".{name} must fail closed as {expected}")
                }
                other => panic!(".{name} must fail closed, got {other:?}"),
            }
        }
    }
}
