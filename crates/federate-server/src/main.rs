//! federate-server: Node 1, the public bootstrap/control-plane server and
//! home of the Federate Root Registry.
//!
//! The registry is PERSISTENT and RUNTIME-MUTABLE:
//!   - on FIRST boot only, seed data (official TLDs, sites/, seed delegated
//!     TLDs) initializes the persistent registry under data_dir/registry/;
//!   - on every later boot the persistent registry is the source of truth,
//!     re-verified against the root key (fail closed);
//!   - at runtime, state changes ONLY through signed mutation requests
//!     (nonce challenge-response, timestamp window, per-target monotonic
//!     versions, signed audit log, root zone snapshots) and the site
//!     package ingest endpoint.
//!
//! Node 1 distributes signed data; daemons verify signatures, they do not
//! trust this server.

use axum::extract::{DefaultBodyLimit, Path as AxPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use federate_identity::NodeIdentity;
use federate_mutation::{MutationContext, MutationRequest, RegistryStore, SitePackage};
use federate_naming::{validate_tld_name, DomainRecord, RegistryType};
use federate_root::{Blocklists, RootZone, TldRecord};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "federate-server", about = "Federate Network Node 1 server")]
struct Args {
    /// Listen address (dev: 127.0.0.1:9000; production sits behind Caddy/Nginx)
    #[arg(long, default_value = federate_core::DEFAULT_SERVER_ADDR)]
    listen: SocketAddr,
    /// Data dir for keys and the persistent registry. Private keys live
    /// here and are never served; registry state lives in data_dir/registry.
    #[arg(long, default_value = ".federate-server")]
    data_dir: PathBuf,
    /// Authoritative IANA/public TLD blocklist file.
    #[arg(long, default_value = "blocked_tlds.txt")]
    blocked_tlds: PathBuf,
    /// Directory with reserved/policy/brand-safety blocklists (created with
    /// defaults when missing).
    #[arg(long, default_value = "data/blocked")]
    blocked_dir: PathBuf,
    /// Native Federate protocol listen address (framed TCP, port 0xFED).
    /// Node 1 is a Federate node first: it serves the signed root zone,
    /// manifests, and blocks over the native protocol; the HTTP routes are
    /// the compatibility surface.
    #[arg(long, default_value = "0.0.0.0:4077")]
    native_listen: SocketAddr,
}

struct Store {
    /// The persistent, runtime-mutable Federate Root Registry (embedded
    /// redb database; nonces live in it too, so replay protection
    /// survives restarts).
    registry: RwLock<RegistryStore>,
    blocklists: Blocklists,
    /// Federate Root Key: signs the zone, TLD records, and audit events.
    root_key: NodeIdentity,
    /// Operator key of the root-managed official TLDs.
    operator_key: NodeIdentity,
    node_id: String,
    started_at: String,
    /// TCP port of this node's native Federate protocol listener,
    /// advertised via `/v1/bootstrap` so clients can go native immediately.
    native_port: u16,
    /// The official node directory (registered nodes, roles, health).
    directory: Arc<federate_directory::Directory>,
}

impl Store {
    fn mutation_ctx<'a>(&'a self, now: chrono::DateTime<chrono::Utc>) -> MutationContext<'a> {
        MutationContext {
            root: &self.root_key,
            official_operator: &self.operator_key,
            blocklists: &self.blocklists,
            now,
        }
    }
}

/// Native-protocol face of Node 1: the same signed root zone, manifests, and
/// blocks the HTTP compatibility routes serve, over the Federate protocol.
/// Trust still never comes from this node; receivers verify everything.
struct NativeService(Arc<Store>);

#[federate_transport::async_trait]
impl federate_transport::NodeService for NativeService {
    fn node_id(&self) -> String {
        self.0.node_id.clone()
    }

    fn capabilities(&self) -> Vec<federate_protocol::Capability> {
        vec![
            federate_protocol::Capability::Root,
            federate_protocol::Capability::Manifests,
            federate_protocol::Capability::Blocks,
            federate_protocol::Capability::TldRegistries,
        ]
    }

    async fn handle(&self, request: federate_protocol::Message) -> federate_protocol::Message {
        use federate_protocol::{ErrorCode, Message};
        let not_found = |detail: &str| Message::Error {
            code: ErrorCode::NotFound,
            detail: detail.to_string(),
        };
        let registry = self.0.registry.read().await;
        match request {
            Message::GetRoot => match serde_json::to_vec(registry.zone()) {
                Ok(zone_json) => Message::Root { zone_json },
                Err(e) => Message::Error {
                    code: ErrorCode::Unavailable,
                    detail: e.to_string(),
                },
            },
            Message::GetManifest { hash } => match registry.manifest(&hash) {
                Some(bytes) => Message::Manifest {
                    hash,
                    bytes: bytes.clone(),
                },
                None => not_found("no such manifest"),
            },
            Message::GetBlock { hash } => match registry.block(&hash) {
                Some(bytes) => Message::Block { hash, bytes },
                None => not_found("no such block"),
            },
            // v1: delegated TLD registries. This node distributes signed
            // registries; receivers verify the operator signature.
            Message::GetTldRegistry { tld } => match registry.registry(&tld) {
                Some((bytes, _)) => Message::TldRegistry {
                    tld,
                    registry_json: bytes.clone(),
                },
                None => not_found("no delegated registry for this TLD here"),
            },
            Message::GetDomainRecord { fqdn } => match registry.lookup_domain(&fqdn) {
                Some(record) => match serde_json::to_vec(record) {
                    Ok(record_json) => Message::DomainRecord { fqdn, record_json },
                    Err(e) => Message::Error {
                        code: ErrorCode::Unavailable,
                        detail: e.to_string(),
                    },
                },
                None => not_found("no such domain record here"),
            },
            Message::GetStatus => Message::Status {
                node_id: self.0.node_id.clone(),
                roles: vec!["root-authority".into(), "origin".into()],
                region: String::new(),
                agent: concat!("federate-server/", env!("CARGO_PKG_VERSION")).into(),
                root_version: Some(registry.zone().root_version),
            },
            _ => Message::Error {
                code: ErrorCode::Unsupported,
                detail: "this node answers GetRoot, GetManifest, GetBlock, and GetStatus".into(),
            },
        }
    }
}

fn build_store(args: &Args) -> anyhow::Result<Store> {
    let now = chrono::Utc::now().to_rfc3339();

    // --- Keys. Private halves stay in data_dir; only public hex leaves.
    // Keys are NEVER part of registry records. ---
    let root_key = NodeIdentity::load_or_create(&args.data_dir.join("root"))?;
    let operator_key = NodeIdentity::load_or_create(&args.data_dir.join("official-operator"))?;
    let node_identity = NodeIdentity::load_or_create(&args.data_dir)?;

    // --- Blocklists ---
    let blocklists = Blocklists::load(&args.blocked_tlds, &args.blocked_dir)?;
    tracing::info!(
        "blocklists loaded: {} IANA, {} reserved, {} policy, {} brand-safety",
        blocklists.iana.len(),
        blocklists.reserved.len(),
        blocklists.policy.len(),
        blocklists.brand_safety.len()
    );

    // --- Persistent registry: the database is the ONLY source of truth.
    // The server never seeds TLDs from code. A missing registry is
    // initialized EMPTY (zero TLDs); the TLD set arrives exclusively via
    // `federate root seed --file <seed.toml>` / `federate tld create`
    // (signed, audited mutations).
    let registry_dir = args.data_dir.join("registry");
    let registry = if RegistryStore::exists(&registry_dir) {
        let store = RegistryStore::open(&registry_dir, &root_key.node_id())?;
        tracing::info!(
            "persistent registry loaded from {} (root zone v{}, {} TLDs, {} domains, {} mutations applied)",
            registry_dir.display(),
            store.zone().root_version,
            store.zone().tlds.len(),
            store.zone().domains.len(),
            store.mutation_count()
        );
        store
    } else {
        let store = federate_mutation::init_empty_registry(&registry_dir, &root_key)?;
        tracing::warn!(
            "first boot: EMPTY registry initialized at {}; no TLDs exist yet. \
             Seed them with `federate root seed --file seeds/official-tlds.toml \
             --data-dir {}` (server stopped) or create them at runtime with \
             `federate tld create`",
            registry_dir.display(),
            args.data_dir.display()
        );
        store
    };

    // Node registrations survive restarts (and nodes re-register every ~60s).
    let directory = federate_directory::Directory::with_persistence(
        Some(registry.zone().root_public_key.clone()),
        args.data_dir.join("directory-nodes.json"),
    );

    Ok(Store {
        registry: RwLock::new(registry),
        blocklists,
        root_key,
        operator_key,
        node_id: node_identity.node_id(),
        started_at: now,
        native_port: args.native_listen.port(),
        directory,
    })
}

mod anyhow {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    let store = Arc::new(build_store(&args)?);
    {
        let registry = store.registry.read().await;
        tracing::info!(
            "root zone v{}: {} TLDs, {} domains, {} blocks (root key {})",
            registry.zone().root_version,
            registry.zone().tlds.len(),
            registry.zone().domains.len(),
            registry.block_count(),
            registry.zone().root_public_key
        );
    }

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(|| async { "ok" }))
        .route("/v1/status", get(status))
        .route("/v1/bootstrap", get(bootstrap))
        .route("/v1/root", get(root_zone))
        .route("/v1/tlds", get(tlds))
        .route("/v1/tld/:tld", get(tld_record))
        .route("/v1/tld-check/:tld", get(tld_check))
        .route("/v1/blocked", get(blocked))
        .route("/v1/reserved", get(reserved))
        .route("/v1/domains", get(domains_by_tld))
        .route("/v1/domain/:fqdn", get(domain_record))
        .route("/v1/tld-registry/:tld", get(tld_registry))
        .route("/v1/manifest/:hash", get(manifest))
        .route("/v1/block/:hash", get(block))
        // Runtime mutation surface: nonce challenge, signed mutations, site
        // package ingest, mutation/audit inspection, registry operations.
        .route("/v1/mutations/nonce", post(mutation_nonce))
        .route("/v1/mutations", post(submit_mutation))
        .route("/v1/mutations/:id", get(mutation_inspect))
        .route("/v1/mutations/target/:kind/:id", get(mutation_target))
        .route(
            "/v1/ingest/package",
            post(ingest_package).layer(DefaultBodyLimit::max(
                // hex encoding doubles the payload; leave headroom on top of
                // the decoded package cap.
                federate_mutation::MAX_PACKAGE_BYTES * 2 + 1024 * 1024,
            )),
        )
        .route("/v1/registry/status", get(registry_status))
        .route("/v1/registry/audit", get(registry_audit))
        .route("/v1/registry/verify", get(registry_verify))
        .route("/v1/registry/snapshot", post(registry_snapshot))
        // Future hooks (documented, intentionally stubbed):
        .route("/v1/peers", get(peers_stub))
        .route("/v1/applications", get(applications_stub))
        .with_state(store.clone())
        // Node directory: registration, role/health listing, block providers.
        .merge(federate_directory::router(store.directory.clone()));

    // Health-check registered nodes every 15s.
    tokio::spawn(federate_directory::health_check_loop(
        store.directory.clone(),
        std::time::Duration::from_secs(15),
    ));

    // Native Federate protocol listener. This is the primary surface of the
    // network; a bind failure only degrades Node 1 to compatibility-only.
    match tokio::net::TcpListener::bind(args.native_listen).await {
        Ok(listener) => {
            tracing::info!(
                "federate native protocol on fed-tcp://{}",
                args.native_listen
            );
            tokio::spawn(federate_transport::serve(
                listener,
                Arc::new(NativeService(store.clone())),
                concat!("federate-server/", env!("CARGO_PKG_VERSION")).into(),
            ));
        }
        Err(e) => tracing::warn!(
            "cannot bind native protocol listener {}: {e}; serving HTTP compatibility only",
            args.native_listen
        ),
    }

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!(
        "federate-server (Node 1) listening on http://{}",
        args.listen
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> impl IntoResponse {
    axum::response::Html(
        "<h1>Federate Network: Node 1</h1>\
         <p>Bootstrap/control-plane node and Federate Root Registry \
         (persistent, runtime-mutable, signed).</p>\
         <p>Read endpoints: /health /v1/status /v1/bootstrap /v1/root /v1/tlds /v1/tld/:tld \
         /v1/tld-check/:tld /v1/blocked /v1/reserved /v1/domains?tld= /v1/domain/:fqdn \
         /v1/manifest/:hash /v1/block/:hash</p>\
         <p>Mutation endpoints: POST /v1/mutations/nonce, POST /v1/mutations, \
         POST /v1/ingest/package, GET /v1/mutations/:id, GET /v1/registry/status \
         /v1/registry/audit /v1/registry/verify, POST /v1/registry/snapshot</p>\
         <p>All registry data is signed; daemons verify, they do not trust this server. \
         All mutations are signed, nonce-protected, versioned, and audited.</p>",
    )
}

async fn status(State(s): State<Arc<Store>>) -> Json<serde_json::Value> {
    let registry = s.registry.read().await;
    Json(serde_json::json!({
        "node": "node-1",
        "node_id": s.node_id,
        "native_port": s.native_port,
        "started_at": s.started_at,
        "root_version": registry.zone().root_version,
        "root_public_key": registry.zone().root_public_key,
        "tlds": registry.zone().tlds.len(),
        "domains": registry.zone().domains.len(),
        "manifests": registry.manifest_count(),
        "blocks": registry.block_count(),
        "mutations_applied": registry.mutation_count(),
    }))
}

async fn bootstrap(State(s): State<Arc<Store>>) -> Json<serde_json::Value> {
    use federate_directory::NodeRole;
    let endpoint = |n: &federate_directory::NodeEntry| n.registration.health_endpoint.clone();
    let mirrors: Vec<String> = s
        .directory
        .healthy(NodeRole::RootMirror)
        .await
        .iter()
        .map(endpoint)
        .collect();
    let dns: Vec<String> = s
        .directory
        .healthy(NodeRole::Dns)
        .await
        .iter()
        .flat_map(|n| n.registration.public_ips.clone())
        .collect();
    let gateways: Vec<String> = s
        .directory
        .healthy(NodeRole::Gateway)
        .await
        .iter()
        .map(endpoint)
        .collect();
    let bootstraps: Vec<String> = s
        .directory
        .healthy(NodeRole::Bootstrap)
        .await
        .iter()
        .map(endpoint)
        .collect();
    // Every healthy node that declared a native listener, as host:port. New
    // clients use these (plus this node's own native_port) to speak the
    // Federate protocol immediately instead of staying on HTTP.
    let native_nodes: Vec<String> = {
        let mut addrs: Vec<String> = s
            .directory
            .list(None, true)
            .await
            .iter()
            .filter_map(|n| n.native_addr().map(|a| a.to_string()))
            .collect();
        addrs.sort();
        addrs.dedup();
        addrs
    };
    let registry = s.registry.read().await;
    Json(serde_json::json!({
        "network": registry.zone().network,
        "root_url": "/v1/root",
        "root_version": registry.zone().root_version,
        "root_public_key": registry.zone().root_public_key,
        "native_port": s.native_port,
        "native_nodes": native_nodes,
        "root_mirrors": mirrors,
        "dns_nodes": dns,
        "gateway_nodes": gateways,
        "directory_nodes": [],
        "bootstrap_nodes": bootstraps,
    }))
}

async fn root_zone(State(s): State<Arc<Store>>) -> Json<RootZone> {
    Json(s.registry.read().await.zone().clone())
}

async fn tlds(State(s): State<Arc<Store>>) -> Json<serde_json::Value> {
    let registry = s.registry.read().await;
    Json(serde_json::json!(registry
        .zone()
        .tlds
        .values()
        .collect::<Vec<_>>()))
}

async fn tld_record(
    State(s): State<Arc<Store>>,
    AxPath(tld): AxPath<String>,
) -> Result<Json<TldRecord>, StatusCode> {
    let tld = tld.trim_start_matches('.').to_ascii_lowercase();
    s.registry
        .read()
        .await
        .zone()
        .lookup_tld(&tld)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Availability/eligibility check for a TLD name: naming rules, registry
/// state, and every blocklist. This is what `federate tld check` uses.
async fn tld_check(
    State(s): State<Arc<Store>>,
    AxPath(input): AxPath<String>,
) -> Json<serde_json::Value> {
    let tld = match validate_tld_name(&input) {
        Ok(t) => t,
        Err(e) => {
            return Json(serde_json::json!({
                "tld": input, "available": false, "verdict": "invalid",
                "reason": e.to_string(),
            }))
        }
    };
    let registry = s.registry.read().await;
    if let Some(rec) = registry.zone().lookup_tld(&tld) {
        return Json(serde_json::json!({
            "tld": tld, "available": false,
            "verdict": rec.status.as_str(),
            "reason": format!(".{tld} already exists in the Federate root registry (status: {}, operator: {})",
                rec.status.as_str(), rec.operator_name),
        }));
    }
    if let Some(reason) = s.blocklists.check(&tld) {
        let verdict = match reason {
            federate_root::BlockReason::Reserved => "reserved",
            _ => "blocked",
        };
        return Json(serde_json::json!({
            "tld": tld, "available": false, "verdict": verdict,
            "reason": format!(".{tld} cannot be created because {}", reason.describe()),
        }));
    }
    Json(serde_json::json!({
        "tld": tld, "available": true, "verdict": "available",
        "reason": format!(".{tld} is available; the root can delegate it at runtime with `federate tld delegate`"),
    }))
}

async fn blocked(State(s): State<Arc<Store>>) -> Json<serde_json::Value> {
    let mut iana: Vec<_> = s.blocklists.iana.iter().cloned().collect();
    let mut policy: Vec<_> = s.blocklists.policy.iter().cloned().collect();
    let mut brand: Vec<_> = s.blocklists.brand_safety.iter().cloned().collect();
    iana.sort();
    policy.sort();
    brand.sort();
    Json(serde_json::json!({ "iana": iana, "policy": policy, "brand_safety": brand }))
}

async fn reserved(State(s): State<Arc<Store>>) -> Json<serde_json::Value> {
    let mut reserved: Vec<_> = s.blocklists.reserved.iter().cloned().collect();
    reserved.sort();
    Json(serde_json::json!({ "reserved": reserved }))
}

async fn domains_by_tld(
    State(s): State<Arc<Store>>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let filter = q
        .get("tld")
        .map(|t| t.trim_start_matches('.').to_ascii_lowercase());
    let registry = s.registry.read().await;
    let list: Vec<_> = registry
        .zone()
        .domains
        .values()
        .filter(|d| filter.as_deref().is_none_or(|t| d.tld == t))
        .collect();
    Json(serde_json::json!(list))
}

async fn domain_record(
    State(s): State<Arc<Store>>,
    AxPath(fqdn): AxPath<String>,
) -> Result<Json<DomainRecord>, StatusCode> {
    s.registry
        .read()
        .await
        .lookup_domain(&fqdn.to_ascii_lowercase())
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// The operator-signed registry of a delegated TLD, as the exact signed
/// bytes (JSON). HTTP compatibility twin of the native `GetTldRegistry`.
async fn tld_registry(
    State(s): State<Arc<Store>>,
    AxPath(tld): AxPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let tld = tld.trim_start_matches('.').to_ascii_lowercase();
    s.registry
        .read()
        .await
        .registry(&tld)
        .map(|(bytes, _)| ([(header::CONTENT_TYPE, "application/json")], bytes.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

/// Headers for content-addressed responses: the URL is the BLAKE3 hash of
/// the bytes, so the response can never change and caches may keep it forever.
const CONTENT_ADDRESSED_CACHE: (header::HeaderName, &str) =
    (header::CACHE_CONTROL, "public, max-age=31536000, immutable");

async fn manifest(
    State(s): State<Arc<Store>>,
    AxPath(hash): AxPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    s.registry
        .read()
        .await
        .manifest(&hash)
        .cloned()
        .map(|b| {
            (
                [
                    (header::CONTENT_TYPE, "application/json"),
                    CONTENT_ADDRESSED_CACHE,
                ],
                b,
            )
        })
        .ok_or(StatusCode::NOT_FOUND)
}

async fn block(
    State(s): State<Arc<Store>>,
    AxPath(hash): AxPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    s.registry
        .read()
        .await
        .block(&hash)
        .map(|b| {
            (
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    CONTENT_ADDRESSED_CACHE,
                ],
                b,
            )
        })
        .ok_or(StatusCode::NOT_FOUND)
}

// ---------------------------------------------------------------------------
// mutation surface
// ---------------------------------------------------------------------------

/// Map a mutation failure to an HTTP status: authorization failures are 403,
/// replay/rollback conflicts are 409, missing targets 404, everything else
/// (malformed, bad transition, policy) 400.
fn mutation_status(e: &federate_core::FederateError) -> StatusCode {
    use federate_core::FederateError::*;
    match e {
        Unauthorized(_) | InvalidSignature => StatusCode::FORBIDDEN,
        Replay(_) => StatusCode::CONFLICT,
        DomainNotFound(_) | TldNotFound { .. } | ManifestNotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    }
}

/// Consume the nonce, then run the mutation under the write lock. Content
/// (blocks + manifest) from a package ingest is stored first so `PublishSite`
/// finds it; content is content-addressed, so a rejected mutation leaves no
/// dangling authority, only unreferenced bytes.
async fn run_mutation(
    s: &Store,
    req: MutationRequest,
    content: Option<(Vec<u8>, Vec<federate_mutation::ContentBlock>)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let now = chrono::Utc::now();
    let mut registry = s.registry.write().await;
    // Nonce consumption is durable (nonces table), so a used challenge
    // stays used across restarts.
    match registry.consume_nonce(&req.nonce, now.timestamp()) {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "accepted": false,
                    "error": "unknown, expired, or already-used nonce; request a fresh challenge at POST /v1/mutations/nonce",
                })),
            )
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "accepted": false, "error": e.to_string() })),
            )
        }
    }
    if let Some((manifest_bytes, blocks)) = content {
        let manifest_hash = federate_storage::hash_bytes(&manifest_bytes);
        if let Err(e) = registry
            .store_blocks(&blocks)
            .and_then(|()| registry.store_manifest(&manifest_hash, &manifest_bytes))
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "accepted": false, "error": e.to_string() })),
            );
        }
    }
    match registry.apply(&req, &s.mutation_ctx(now)) {
        Ok(event) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "accepted": true,
                "mutation_id": req.mutation_id,
                "root_version": registry.zone().root_version,
                "audit_event": event,
            })),
        ),
        Err(e) => (
            mutation_status(&e),
            Json(serde_json::json!({ "accepted": false, "error": e.to_string() })),
        ),
    }
}

/// Challenge half of challenge-response: a single-use nonce the next signed
/// mutation must embed.
async fn mutation_nonce(State(s): State<Arc<Store>>) -> (StatusCode, Json<serde_json::Value>) {
    let now = chrono::Utc::now().timestamp();
    match s.registry.read().await.issue_nonce(now) {
        Ok((nonce, expires_at)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "nonce": nonce,
                "expires_at_unix": expires_at,
                "ttl_secs": federate_mutation::NONCE_TTL_SECS,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn submit_mutation(
    State(s): State<Arc<Store>>,
    Json(req): Json<MutationRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    run_mutation(&s, req, None).await
}

/// Site package ingest: content blocks + exact owner-signed manifest bytes +
/// a signed `PublishSite` mutation. Hashes are verified before anything is
/// stored; the mutation then authorizes the domain record update.
async fn ingest_package(
    State(s): State<Arc<Store>>,
    Json(pkg): Json<SitePackage>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (manifest_bytes, blocks) = match pkg.decode() {
        Ok(decoded) => decoded,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "accepted": false, "error": e.to_string() })),
            )
        }
    };
    run_mutation(&s, pkg.mutation, Some((manifest_bytes, blocks))).await
}

async fn mutation_inspect(
    State(s): State<Arc<Store>>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let registry = s.registry.read().await;
    let applied = registry.applied(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!(applied)))
}

/// Current and next per-target mutation version, so clients can build a
/// request that advances the target ("domain:eu.pagina", "tld:femboy").
async fn mutation_target(
    State(s): State<Arc<Store>>,
    AxPath((kind, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if kind != "domain" && kind != "tld" {
        return Err(StatusCode::BAD_REQUEST);
    }
    let key = format!("{kind}:{}", id.to_ascii_lowercase());
    let current = s.registry.read().await.target_version(&key);
    Ok(Json(serde_json::json!({
        "target": key,
        "current_version": current,
        "next_version": current + 1,
    })))
}

async fn registry_status(State(s): State<Arc<Store>>) -> Json<serde_json::Value> {
    let registry = s.registry.read().await;
    Json(serde_json::json!({
        "root_version": registry.zone().root_version,
        "generated_at": registry.zone().generated_at,
        "root_public_key": registry.zone().root_public_key,
        "tlds": registry.zone().tlds.len(),
        "domains": registry.zone().domains.len(),
        "delegated_registries": registry.zone().tlds.values()
            .filter(|t| t.registry_type != RegistryType::RootManaged).count(),
        "manifests": registry.manifest_count(),
        "blocks": registry.block_count(),
        "mutations_applied": registry.mutation_count(),
        "audit_events": registry.audit_count(),
        "registry_dir": registry.dir().display().to_string(),
        "persistent": true,
        "seed_is_first_boot_only": true,
    }))
}

async fn registry_audit(
    State(s): State<Arc<Store>>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let limit = q
        .get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    let registry = s.registry.read().await;
    Json(serde_json::json!({
        "total": registry.audit_count(),
        "events": registry.audit_tail(limit),
    }))
}

/// Full self-check of the persistent registry: zone signature, delegated
/// registries, every manifest hash, every block, every audit signature.
async fn registry_verify(State(s): State<Arc<Store>>) -> (StatusCode, Json<serde_json::Value>) {
    let registry = s.registry.read().await;
    match registry.verify_all(&s.root_key.node_id()) {
        Ok(report) => (StatusCode::OK, Json(report)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "verified": false, "error": e.to_string() })),
        ),
    }
}

async fn registry_snapshot(State(s): State<Arc<Store>>) -> (StatusCode, Json<serde_json::Value>) {
    let registry = s.registry.read().await;
    match registry.write_snapshot() {
        Ok(path) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "root_version": registry.zone().root_version,
                "snapshot": path.display().to_string(),
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn peers_stub() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "peers": [], "note": "peer/CDN discovery arrives in phase 5" }))
}

async fn applications_stub() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "applications": [],
        "note": "TLD applications/approval arrive in marketplace phase 2; runtime delegation already works via signed mutations (federate tld delegate)"
    }))
}
