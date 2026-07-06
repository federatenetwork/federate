//! federate-directory: the Federate Node Directory.
//!
//! Tracks live nodes (id, key, IPs, region, roles, health, latency, capacity,
//! last_seen), verifies signed registrations, health-checks nodes, and
//! answers "give me healthy gateways / DNS nodes / providers for block X".
//!
//! The directory is *infrastructure discovery only*; it never decides what
//! names or content are valid. That authority stays with the signed root zone.

use federate_core::{FederateError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

/// Everything a Federate node can do. TLD authority is NOT a role anyone can
/// claim: `root-authority` registrations are rejected unless signed by the
/// pinned Federate Root Key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeRole {
    RootAuthority,
    RootMirror,
    Dns,
    Gateway,
    Storage,
    Cdn,
    Search,
    Bootstrap,
    Origin,
}

impl NodeRole {
    pub const ALL: &'static [NodeRole] = &[
        NodeRole::RootAuthority,
        NodeRole::RootMirror,
        NodeRole::Dns,
        NodeRole::Gateway,
        NodeRole::Storage,
        NodeRole::Cdn,
        NodeRole::Search,
        NodeRole::Bootstrap,
        NodeRole::Origin,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NodeRole::RootAuthority => "root-authority",
            NodeRole::RootMirror => "root-mirror",
            NodeRole::Dns => "dns",
            NodeRole::Gateway => "gateway",
            NodeRole::Storage => "storage",
            NodeRole::Cdn => "cdn",
            NodeRole::Search => "search",
            NodeRole::Bootstrap => "bootstrap",
            NodeRole::Origin => "origin",
        }
    }
}

impl std::str::FromStr for NodeRole {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        NodeRole::ALL
            .iter()
            .copied()
            .find(|r| r.as_str() == s.trim().to_ascii_lowercase())
            .ok_or_else(|| format!("unknown node role '{s}' (valid: root-authority root-mirror dns gateway storage cdn search bootstrap origin)"))
    }
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Online,
    Degraded,
    Offline,
}

impl NodeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeStatus::Online => "online",
            NodeStatus::Degraded => "degraded",
            NodeStatus::Offline => "offline",
        }
    }
}

// ---------------------------------------------------------------------------
// Registration (signed)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeCapacity {
    #[serde(default)]
    pub storage_gb: u64,
    #[serde(default)]
    pub bandwidth_mbps: u64,
}

/// A node's self-registration. Signed by the node's private key over
/// canonical JSON with `signature: null`; the directory verifies the
/// signature and that `node_id == public_key` before accepting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRegistration {
    pub node_id: String,
    pub public_key: String,
    pub roles: Vec<NodeRole>,
    /// Public IPs (v4/v6) browsers or peers can reach this node at.
    pub public_ips: Vec<String>,
    pub region: String,
    pub version: String,
    pub capacity: NodeCapacity,
    /// Base URL of the node's health API, e.g. "http://45.1.1.1:8080".
    /// The checker GETs `{health_endpoint}/health`.
    pub health_endpoint: String,
    pub registered_at: String,
    pub signature_algorithm: String,
    #[serde(default)]
    pub signature: Option<String>,
}

impl NodeRegistration {
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    /// Verify the registration is signed by the node's own key.
    pub fn verify(&self) -> Result<()> {
        let fail = |reason: &str| {
            Err(FederateError::VerificationFailed {
                layer: "node-registration".into(),
                subject: self.node_id.clone(),
                reason: reason.to_string(),
            })
        };
        if self.node_id != self.public_key {
            return fail("node_id must equal the node public key");
        }
        if self.roles.is_empty() {
            return fail("registration lists no roles");
        }
        let Some(sig) = &self.signature else {
            return fail("registration is unsigned");
        };
        if !federate_identity::verify_signature(&self.public_key, &self.signable_bytes()?, sig) {
            return fail("registration signature invalid");
        }
        // Every declared public IP must actually parse as an IP address.
        if self.public_ips.is_empty() || self.ips().len() != self.public_ips.len() {
            return fail("public_ips must all be valid IP addresses");
        }
        // The health endpoint must be http(s) and point at one of this node's
        // own declared IPs. Otherwise a node could aim the directory's health
        // checker and gateway block-fetches at an arbitrary host (SSRF) -
        // e.g. cloud metadata endpoints or someone else's server.
        let host = health_endpoint_host(&self.health_endpoint)
            .ok_or(())
            .map_err(|_| FederateError::VerificationFailed {
                layer: "node-registration".into(),
                subject: self.node_id.clone(),
                reason: "health_endpoint must be an http(s) URL".into(),
            })?;
        if !self
            .public_ips
            .iter()
            .any(|ip| ip.trim_matches(|c| c == '[' || c == ']') == host)
        {
            return fail("health_endpoint host must be one of the node's declared public_ips");
        }
        Ok(())
    }

    /// Parsed public IPs (invalid entries skipped).
    pub fn ips(&self) -> Vec<IpAddr> {
        self.public_ips
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect()
    }
}

/// A signed announcement that a node holds a set of content blocks. Signed by
/// the node's key so a stranger cannot poison another node's provider list.
/// Block hashes are still trust-but-verify; gateways re-hash every fetch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockAnnounce {
    pub node_id: String,
    pub blocks: Vec<String>,
    pub announced_at: String,
    #[serde(default)]
    pub signature: Option<String>,
}

impl BlockAnnounce {
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    /// Sign an announcement with the announcing node's identity.
    pub fn signed(identity: &federate_identity::NodeIdentity, blocks: Vec<String>) -> Result<Self> {
        let mut a = Self {
            node_id: identity.node_id(),
            blocks,
            announced_at: chrono::Utc::now().to_rfc3339(),
            signature: None,
        };
        a.signature = Some(identity.sign(&a.signable_bytes()?));
        Ok(a)
    }

    /// Verify the announcement is signed by `node_id`'s own key.
    pub fn verify(&self) -> Result<()> {
        let Some(sig) = &self.signature else {
            return Err(FederateError::VerificationFailed {
                layer: "block-announce".into(),
                subject: self.node_id.clone(),
                reason: "unsigned announcement".into(),
            });
        };
        if !federate_identity::verify_signature(&self.node_id, &self.signable_bytes()?, sig) {
            return Err(FederateError::VerificationFailed {
                layer: "block-announce".into(),
                subject: self.node_id.clone(),
                reason: "announcement signature invalid".into(),
            });
        }
        Ok(())
    }
}

/// Extract the host of an `http(s)://host[:port]` URL as a bare string
/// (brackets stripped for IPv6). Returns None for non-http(s) schemes or when
/// no host is present. Kept dependency-free (no url crate).
fn health_endpoint_host(endpoint: &str) -> Option<String> {
    let rest = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))?;
    // host is everything before the first '/', and for IPv6 is bracketed.
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.is_empty() {
        return None;
    }
    let host = if let Some(end) = authority
        .strip_prefix('[')
        .and_then(|a| a.split(']').next())
    {
        // [ipv6]:port
        end.to_string()
    } else {
        // host:port or host
        authority.split(':').next().unwrap_or(authority).to_string()
    };
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// A tracked node: registration + live health state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEntry {
    #[serde(flatten)]
    pub registration: NodeRegistration,
    pub status: NodeStatus,
    pub last_seen: String,
    #[serde(default)]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub consecutive_failures: u32,
}

// ---------------------------------------------------------------------------
// Directory
// ---------------------------------------------------------------------------

/// Hard cap on tracked nodes. Registrations are self-signed, so without a cap
/// anyone could grow the directory's memory without bound.
pub const MAX_NODES: usize = 5000;

/// Nodes that neither re-registered nor answered a health check for this long
/// are removed entirely (they can always re-register).
pub const STALE_NODE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

pub struct Directory {
    nodes: RwLock<HashMap<String, NodeEntry>>,
    /// block hash -> node_ids announcing they hold it
    providers: RwLock<HashMap<String, HashSet<String>>>,
    /// Only this key may register the `root-authority` role.
    root_public_key: Option<String>,
    /// When set, the node table is snapshotted here so registrations survive
    /// a directory restart (nodes also re-register every ~60s).
    persist_path: Option<std::path::PathBuf>,
}

impl Directory {
    pub fn new(root_public_key: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            nodes: RwLock::new(HashMap::new()),
            providers: RwLock::new(HashMap::new()),
            root_public_key,
            persist_path: None,
        })
    }

    /// A directory whose node table persists across restarts. Snapshot
    /// entries are re-verified on load; a tampered snapshot cannot inject
    /// unverifiable registrations.
    pub fn with_persistence(
        root_public_key: Option<String>,
        path: std::path::PathBuf,
    ) -> Arc<Self> {
        let mut nodes = HashMap::new();
        if let Ok(bytes) = std::fs::read(&path) {
            match serde_json::from_slice::<Vec<NodeEntry>>(&bytes) {
                Ok(entries) => {
                    for entry in entries {
                        if entry.registration.verify().is_ok() {
                            nodes.insert(entry.registration.node_id.clone(), entry);
                        }
                    }
                    tracing::info!(
                        "directory: restored {} node(s) from {}",
                        nodes.len(),
                        path.display()
                    );
                }
                Err(e) => tracing::warn!(
                    "directory: ignoring corrupt snapshot {}: {e}",
                    path.display()
                ),
            }
        }
        Arc::new(Self {
            nodes: RwLock::new(nodes),
            providers: RwLock::new(HashMap::new()),
            root_public_key,
            persist_path: Some(path),
        })
    }

    /// Best-effort snapshot (write-then-rename so a crash never truncates it).
    fn persist(&self, nodes: &HashMap<String, NodeEntry>) {
        let Some(path) = &self.persist_path else {
            return;
        };
        let entries: Vec<&NodeEntry> = nodes.values().collect();
        if let Ok(bytes) = serde_json::to_vec(&entries) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() {
                std::fs::rename(&tmp, path).ok();
            }
        }
    }

    /// Register (or refresh) a node. Signature-verified; `root-authority` is
    /// restricted to the Federate Root Key; total node count is capped.
    pub async fn register(&self, reg: NodeRegistration) -> Result<()> {
        reg.verify()?;
        if reg.roles.contains(&NodeRole::RootAuthority)
            && self.root_public_key.as_deref() != Some(reg.public_key.as_str())
        {
            return Err(FederateError::VerificationFailed {
                layer: "node-registration".into(),
                subject: reg.node_id.clone(),
                reason: "only the Federate Root Key may register the root-authority role".into(),
            });
        }
        let now = chrono::Utc::now().to_rfc3339();
        let mut nodes = self.nodes.write().await;
        if nodes.len() >= MAX_NODES && !nodes.contains_key(&reg.node_id) {
            return Err(FederateError::VerificationFailed {
                layer: "node-registration".into(),
                subject: reg.node_id.clone(),
                reason: format!("directory is full ({MAX_NODES} nodes)"),
            });
        }
        let entry = nodes
            .entry(reg.node_id.clone())
            .or_insert_with(|| NodeEntry {
                registration: reg.clone(),
                status: NodeStatus::Online,
                last_seen: now.clone(),
                latency_ms: None,
                consecutive_failures: 0,
            });
        entry.registration = reg;
        entry.last_seen = now;
        entry.status = NodeStatus::Online;
        entry.consecutive_failures = 0;
        self.persist(&nodes);
        Ok(())
    }

    /// Drop nodes not seen for `max_age` (no successful health check and no
    /// re-registration), and scrub them from provider lists. Returns how many
    /// were removed. An unparseable `last_seen` counts as stale (fail closed).
    pub async fn prune_stale(&self, max_age: std::time::Duration) -> usize {
        let now = chrono::Utc::now();
        let mut nodes = self.nodes.write().await;
        let stale: Vec<String> = nodes
            .iter()
            .filter(
                |(_, entry)| match chrono::DateTime::parse_from_rfc3339(&entry.last_seen) {
                    Ok(seen) => {
                        now.signed_duration_since(seen)
                            > chrono::Duration::from_std(max_age)
                                .unwrap_or_else(|_| chrono::Duration::days(1))
                    }
                    Err(_) => true,
                },
            )
            .map(|(id, _)| id.clone())
            .collect();
        for id in &stale {
            nodes.remove(id);
        }
        if !stale.is_empty() {
            let mut providers = self.providers.write().await;
            providers.retain(|_, ids| {
                for id in &stale {
                    ids.remove(id);
                }
                !ids.is_empty()
            });
            tracing::info!("directory: pruned {} stale node(s)", stale.len());
            self.persist(&nodes);
        }
        stale.len()
    }

    /// A storage/CDN node announces block hashes it can serve. The
    /// announcement must be signed by the node's own key and the node must
    /// already be registered; this stops anyone from stuffing another node's
    /// provider list. Hashes are still trust-but-verify (gateways re-hash on
    /// fetch); malformed hashes are dropped so they never reach a fetch URL.
    pub async fn announce_blocks(&self, announce: BlockAnnounce) -> Result<()> {
        announce.verify()?;
        if !self.nodes.read().await.contains_key(&announce.node_id) {
            return Err(FederateError::Network(format!(
                "unknown node {}",
                announce.node_id
            )));
        }
        let mut prov = self.providers.write().await;
        for hash in announce.blocks {
            if !federate_storage::is_valid_hash(&hash) {
                continue;
            }
            prov.entry(hash)
                .or_default()
                .insert(announce.node_id.clone());
        }
        Ok(())
    }

    pub async fn list(&self, role: Option<NodeRole>, only_healthy: bool) -> Vec<NodeEntry> {
        self.nodes
            .read()
            .await
            .values()
            .filter(|n| role.is_none_or(|r| n.registration.roles.contains(&r)))
            .filter(|n| !only_healthy || n.status != NodeStatus::Offline)
            .cloned()
            .collect()
    }

    /// Healthy nodes for a role, best first (online before degraded, then by
    /// latency). This is what DNS uses to answer with multiple gateway IPs.
    pub async fn healthy(&self, role: NodeRole) -> Vec<NodeEntry> {
        let mut nodes = self.list(Some(role), true).await;
        nodes.sort_by_key(|n| {
            (
                (n.status != NodeStatus::Online) as u8,
                n.latency_ms.unwrap_or(u64::MAX),
            )
        });
        nodes
    }

    pub async fn get(&self, node_id: &str) -> Option<NodeEntry> {
        self.nodes.read().await.get(node_id).cloned()
    }

    /// Nodes announcing a block, filtered by role (storage/cdn/origin).
    pub async fn providers_for_block(&self, hash: &str, role: Option<NodeRole>) -> Vec<NodeEntry> {
        let ids = self
            .providers
            .read()
            .await
            .get(hash)
            .cloned()
            .unwrap_or_default();
        let nodes = self.nodes.read().await;
        let mut out: Vec<NodeEntry> = ids
            .iter()
            .filter_map(|id| nodes.get(id))
            .filter(|n| n.status != NodeStatus::Offline)
            .filter(|n| role.is_none_or(|r| n.registration.roles.contains(&r)))
            .cloned()
            .collect();
        out.sort_by_key(|n| n.latency_ms.unwrap_or(u64::MAX));
        out
    }

    async fn mark(&self, node_id: &str, ok: bool, latency_ms: Option<u64>) {
        let mut nodes = self.nodes.write().await;
        if let Some(entry) = nodes.get_mut(node_id) {
            if ok {
                entry.consecutive_failures = 0;
                entry.status = NodeStatus::Online;
                entry.latency_ms = latency_ms;
                entry.last_seen = chrono::Utc::now().to_rfc3339();
            } else {
                entry.consecutive_failures += 1;
                entry.status = match entry.consecutive_failures {
                    1 | 2 => NodeStatus::Degraded,
                    _ => NodeStatus::Offline,
                };
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Health checker
// ---------------------------------------------------------------------------

/// Poll every registered node's `{health_endpoint}/health` on an interval and
/// mark it online (200), degraded (1-2 consecutive failures), or offline (3+).
/// Also expires nodes not seen for [`STALE_NODE_MAX_AGE`].
pub async fn health_check_loop(dir: Arc<Directory>, interval: std::time::Duration) {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("reqwest client");
    loop {
        dir.prune_stale(STALE_NODE_MAX_AGE).await;
        let nodes = dir.list(None, false).await;
        for node in nodes {
            let url = format!(
                "{}/health",
                node.registration.health_endpoint.trim_end_matches('/')
            );
            let started = std::time::Instant::now();
            let ok = matches!(http.get(&url).send().await, Ok(r) if r.status().is_success());
            let latency = started.elapsed().as_millis() as u64;
            dir.mark(&node.registration.node_id, ok, ok.then_some(latency))
                .await;
            if !ok {
                tracing::debug!(
                    "health check failed for {} ({url})",
                    node.registration.node_id
                );
            }
        }
        tokio::time::sleep(interval).await;
    }
}

// ---------------------------------------------------------------------------
// HTTP API (mounted by federate-server and directory nodes)
// ---------------------------------------------------------------------------

pub fn router(dir: Arc<Directory>) -> axum::Router {
    use axum::extract::{Path, Query, State};
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::Json;

    async fn register(
        State(dir): State<Arc<Directory>>,
        Json(reg): Json<NodeRegistration>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        match dir.register(reg).await {
            Ok(()) => (
                StatusCode::OK,
                Json(serde_json::json!({ "registered": true })),
            ),
            Err(e) => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "registered": false, "error": e.to_string() })),
            ),
        }
    }

    async fn list_nodes(
        State(dir): State<Arc<Directory>>,
        Query(q): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let role = q.get("role").and_then(|r| r.parse::<NodeRole>().ok());
        let healthy = q.get("healthy").map(|v| v == "true").unwrap_or(false);
        let nodes = match (healthy, role) {
            (true, Some(r)) => dir.healthy(r).await,
            _ => dir.list(role, healthy).await,
        };
        Json(serde_json::json!({ "nodes": nodes }))
    }

    async fn get_node(
        State(dir): State<Arc<Directory>>,
        Path(id): Path<String>,
    ) -> std::result::Result<Json<NodeEntry>, StatusCode> {
        dir.get(&id).await.map(Json).ok_or(StatusCode::NOT_FOUND)
    }

    async fn announce(
        State(dir): State<Arc<Directory>>,
        Json(a): Json<BlockAnnounce>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        match dir.announce_blocks(a).await {
            Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
            Err(e) => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
            ),
        }
    }

    async fn providers(
        State(dir): State<Arc<Directory>>,
        Path(hash): Path<String>,
        Query(q): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let role = q.get("role").and_then(|r| r.parse::<NodeRole>().ok());
        Json(serde_json::json!({ "providers": dir.providers_for_block(&hash, role).await }))
    }

    axum::Router::new()
        .route("/v1/nodes/register", post(register))
        .route("/v1/nodes", get(list_nodes))
        .route("/v1/nodes/:id", get(get_node))
        .route("/v1/nodes/announce-blocks", post(announce))
        .route("/v1/providers/:hash", get(providers))
        .with_state(dir)
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// HTTP client for a node directory (Node 1 or any directory node).
#[derive(Clone)]
pub struct DirectoryClient {
    base: String,
    http: reqwest::Client,
}

impl DirectoryClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub async fn register(&self, reg: &NodeRegistration) -> Result<()> {
        let resp = self
            .http
            .post(format!("{}/v1/nodes/register", self.base))
            .json(reg)
            .send()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(FederateError::Network(format!(
                "registration rejected: {body}"
            )));
        }
        Ok(())
    }

    pub async fn list(&self, role: Option<NodeRole>, only_healthy: bool) -> Result<Vec<NodeEntry>> {
        let mut url = format!("{}/v1/nodes?healthy={only_healthy}", self.base);
        if let Some(r) = role {
            url.push_str(&format!("&role={}", r.as_str()));
        }
        let v: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?;
        Ok(serde_json::from_value(v["nodes"].clone())?)
    }

    pub async fn providers(&self, hash: &str, role: Option<NodeRole>) -> Result<Vec<NodeEntry>> {
        let mut url = format!("{}/v1/providers/{hash}", self.base);
        if let Some(r) = role {
            url.push_str(&format!("?role={}", r.as_str()));
        }
        let v: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?;
        Ok(serde_json::from_value(v["providers"].clone())?)
    }

    /// Sign and send a block announcement. The directory rejects unsigned or
    /// mis-signed announcements, so the announcing node's identity is required.
    pub async fn announce_blocks(
        &self,
        identity: &federate_identity::NodeIdentity,
        blocks: Vec<String>,
    ) -> Result<()> {
        let announce = BlockAnnounce::signed(identity, blocks)?;
        let resp = self
            .http
            .post(format!("{}/v1/nodes/announce-blocks", self.base))
            .json(&announce)
            .send()
            .await
            .map_err(|e| FederateError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(FederateError::Network(format!("announce rejected: {body}")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use federate_identity::NodeIdentity;

    fn make_reg(id: &NodeIdentity, roles: Vec<NodeRole>) -> NodeRegistration {
        let mut reg = NodeRegistration {
            node_id: id.node_id(),
            public_key: id.node_id(),
            roles,
            public_ips: vec!["45.1.1.1".into()],
            region: "br-sp".into(),
            version: "0.1.0".into(),
            capacity: NodeCapacity {
                storage_gb: 100,
                bandwidth_mbps: 500,
            },
            health_endpoint: "http://45.1.1.1:8080".into(),
            registered_at: "t".into(),
            signature_algorithm: "ed25519".into(),
            signature: None,
        };
        reg.signature = Some(id.sign(&reg.signable_bytes().unwrap()));
        reg
    }

    #[tokio::test]
    async fn signed_registration_accepted_unsigned_rejected() {
        let dir = std::env::temp_dir().join(format!("fed-dir-test-{}", std::process::id()));
        let id = NodeIdentity::load_or_create(&dir).unwrap();
        let directory = Directory::new(None);
        let reg = make_reg(&id, vec![NodeRole::Gateway, NodeRole::Cdn]);
        directory.register(reg.clone()).await.unwrap();
        // tampered registration rejected
        let mut bad = reg.clone();
        bad.region = "evil".into();
        assert!(directory.register(bad).await.is_err());
        // root-authority claim rejected for non-root key
        let claim = make_reg(&id, vec![NodeRole::RootAuthority]);
        assert!(directory.register(claim).await.is_err());
        // role listing
        assert_eq!(directory.healthy(NodeRole::Gateway).await.len(), 1);
        assert_eq!(directory.healthy(NodeRole::Dns).await.len(), 0);
        // block providers: signed announce accepted, valid hash tracked,
        // malformed hash dropped.
        let good_hash = federate_storage::hash_bytes(b"a block");
        directory
            .announce_blocks(
                BlockAnnounce::signed(&id, vec![good_hash.clone(), "../evil".into()]).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            directory
                .providers_for_block(&good_hash, Some(NodeRole::Cdn))
                .await
                .len(),
            1
        );
        assert!(directory
            .providers_for_block("../evil", None)
            .await
            .is_empty());

        // unsigned / wrong-key announce rejected
        let mut forged = BlockAnnounce::signed(&id, vec![good_hash.clone()]).unwrap();
        forged.signature = None;
        assert!(directory.announce_blocks(forged).await.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn reg_with(id: &NodeIdentity, ip: &str, roles: Vec<NodeRole>) -> NodeRegistration {
        let mut reg = NodeRegistration {
            node_id: id.node_id(),
            public_key: id.node_id(),
            roles,
            public_ips: vec![ip.into()],
            region: "br-sp".into(),
            version: "0.1.0".into(),
            capacity: NodeCapacity::default(),
            health_endpoint: format!("http://{ip}:8080"),
            registered_at: "t".into(),
            signature_algorithm: "ed25519".into(),
            signature: None,
        };
        reg.signature = Some(id.sign(&reg.signable_bytes().unwrap()));
        reg
    }

    #[tokio::test]
    async fn dns_gets_multiple_healthy_gateways_offline_excluded() {
        let base = std::env::temp_dir().join(format!("fed-gw-sel-{}", std::process::id()));
        let g1 = NodeIdentity::load_or_create(&base.join("g1")).unwrap();
        let g2 = NodeIdentity::load_or_create(&base.join("g2")).unwrap();
        let g3 = NodeIdentity::load_or_create(&base.join("g3")).unwrap();
        let directory = Directory::new(None);
        directory
            .register(reg_with(&g1, "45.0.0.1", vec![NodeRole::Gateway]))
            .await
            .unwrap();
        directory
            .register(reg_with(&g2, "45.0.0.2", vec![NodeRole::Gateway]))
            .await
            .unwrap();
        directory
            .register(reg_with(&g3, "45.0.0.3", vec![NodeRole::Gateway]))
            .await
            .unwrap();

        // All three healthy -> DNS would answer with all three IPs.
        assert_eq!(directory.healthy(NodeRole::Gateway).await.len(), 3);

        // Drive g3 to offline (3+ consecutive failures); it must be excluded.
        for _ in 0..3 {
            directory.mark(&g3.node_id(), false, None).await;
        }
        let healthy = directory.healthy(NodeRole::Gateway).await;
        assert_eq!(healthy.len(), 2, "offline gateway excluded");
        assert!(healthy
            .iter()
            .all(|n| n.registration.node_id != g3.node_id()));

        // Online ranks before degraded.
        directory.mark(&g2.node_id(), false, None).await; // 1 failure -> degraded
        let ranked = directory.healthy(NodeRole::Gateway).await;
        assert_eq!(ranked[0].registration.node_id, g1.node_id());
        std::fs::remove_dir_all(&base).ok();
    }

    #[tokio::test]
    async fn stale_nodes_pruned_and_scrubbed_from_providers() {
        let base = std::env::temp_dir().join(format!("fed-stale-{}", std::process::id()));
        let id = NodeIdentity::load_or_create(&base).unwrap();
        let directory = Directory::new(None);
        directory
            .register(reg_with(&id, "45.0.0.1", vec![NodeRole::Cdn]))
            .await
            .unwrap();
        let hash = federate_storage::hash_bytes(b"block");
        directory
            .announce_blocks(BlockAnnounce::signed(&id, vec![hash.clone()]).unwrap())
            .await
            .unwrap();
        // Fresh node: nothing pruned.
        assert_eq!(
            directory
                .prune_stale(std::time::Duration::from_secs(3600))
                .await,
            0
        );
        // Backdate last_seen: node + its provider entries must be removed.
        {
            let mut nodes = directory.nodes.write().await;
            nodes.get_mut(&id.node_id()).unwrap().last_seen =
                (chrono::Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        }
        assert_eq!(
            directory
                .prune_stale(std::time::Duration::from_secs(24 * 3600))
                .await,
            1
        );
        assert!(directory.get(&id.node_id()).await.is_none());
        assert!(directory.providers_for_block(&hash, None).await.is_empty());
        // Unparseable last_seen fails closed (pruned).
        directory
            .register(reg_with(&id, "45.0.0.1", vec![NodeRole::Cdn]))
            .await
            .unwrap();
        {
            let mut nodes = directory.nodes.write().await;
            nodes.get_mut(&id.node_id()).unwrap().last_seen = "garbage".into();
        }
        assert_eq!(
            directory
                .prune_stale(std::time::Duration::from_secs(3600))
                .await,
            1
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[tokio::test]
    async fn persistence_survives_restart_and_rejects_tampered_snapshot() {
        let base = std::env::temp_dir().join(format!("fed-persist-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let id = NodeIdentity::load_or_create(&base.join("id")).unwrap();
        let snapshot = base.join("nodes.json");

        let directory = Directory::with_persistence(None, snapshot.clone());
        directory
            .register(reg_with(&id, "45.0.0.7", vec![NodeRole::Gateway]))
            .await
            .unwrap();
        drop(directory);

        // "Restart": a fresh directory restores the signed registration.
        let restored = Directory::with_persistence(None, snapshot.clone());
        assert!(restored.get(&id.node_id()).await.is_some());

        // Tampered snapshot entries fail signature verification -> dropped.
        let mut entries: Vec<NodeEntry> =
            serde_json::from_slice(&std::fs::read(&snapshot).unwrap()).unwrap();
        entries[0].registration.region = "evil".into();
        std::fs::write(&snapshot, serde_json::to_vec(&entries).unwrap()).unwrap();
        let poisoned = Directory::with_persistence(None, snapshot.clone());
        assert!(poisoned.get(&id.node_id()).await.is_none());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn health_endpoint_ssrf_rejected() {
        let dir = std::env::temp_dir().join(format!("fed-ssrf-{}", std::process::id()));
        let id = NodeIdentity::load_or_create(&dir).unwrap();
        // health_endpoint points at a host NOT in public_ips (metadata svc).
        let mut reg = reg_with(&id, "45.0.0.9", vec![NodeRole::Gateway]);
        reg.health_endpoint = "http://169.254.169.254:80".into();
        reg.signature = Some(id.sign(&reg.signable_bytes().unwrap()));
        assert!(reg.verify().is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
