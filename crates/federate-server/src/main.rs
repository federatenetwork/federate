//! federate-server: Node 1, the public bootstrap/control-plane server and
//! home of the Federate Root Registry.
//!
//! At startup it:
//!   - loads blocklists (blocked_tlds.txt + data/blocked/*)
//!   - loads/creates the Federate Root Key, official operator key, and dev
//!     domain-owner key (private keys stay on disk, never exposed via API)
//!   - validates and signs official TLD records with the root key
//!   - scans sites/, content-addresses files (BLAKE3), signs manifests with
//!     the owner key and domain records with the operator key
//!   - signs the assembled root zone with the root key
//!
//! Node 1 distributes signed data; daemons verify signatures, they do not
//! trust this server.

use axum::extract::{Path as AxPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use clap::Parser;
use federate_identity::NodeIdentity;
use federate_manifest::Manifest;
use federate_naming::{
    validate_tld_name, DomainRecord, DomainStatus, RegistryType, TargetType, TldMode, TldStatus,
};
use federate_root::{AuditEvent, Blocklists, RootZone, TldRecord, SIGNATURE_ALGORITHM};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "federate-server", about = "Federate Network Node 1 server")]
struct Args {
    /// Listen address (dev: 127.0.0.1:9000; production sits behind Caddy/Nginx)
    #[arg(long, default_value = federate_core::DEFAULT_SERVER_ADDR)]
    listen: SocketAddr,
    /// Directory of site directories (home-fed/, joao-pagina/, ...)
    #[arg(long, default_value = "sites")]
    sites_dir: PathBuf,
    /// Data dir for keys (root key, operator key, owner key). Private keys
    /// live here and are never served.
    #[arg(long, default_value = ".federate-server")]
    data_dir: PathBuf,
    /// Authoritative IANA/public TLD blocklist file.
    #[arg(long, default_value = "blocked_tlds.txt")]
    blocked_tlds: PathBuf,
    /// Directory with reserved/policy/brand-safety blocklists (created with
    /// defaults when missing).
    #[arg(long, default_value = "data/blocked")]
    blocked_dir: PathBuf,
}

struct Store {
    root: RootZone,
    blocklists: Blocklists,
    /// manifest hash -> canonical manifest JSON bytes
    manifests: HashMap<String, Vec<u8>>,
    /// block hash -> bytes
    blocks: HashMap<String, Vec<u8>>,
    node_id: String,
    started_at: String,
    /// The official node directory (registered nodes, roles, health).
    directory: Arc<federate_directory::Directory>,
}

/// Turn a site dir name like "home-fed" into a domain "home.fed"
/// (last hyphen separates label and TLD).
fn dir_to_domain(dir: &str) -> Option<(String, String)> {
    let (label, tld) = dir.rsplit_once('-')?;
    Some((label.to_string(), tld.to_string()))
}

fn build_store(args: &Args) -> anyhow::Result<Store> {
    let now = chrono::Utc::now().to_rfc3339();

    // --- Keys. Private halves stay in data_dir; only public hex leaves. ---
    let root_key = NodeIdentity::load_or_create(&args.data_dir.join("root"))?;
    let operator_key = NodeIdentity::load_or_create(&args.data_dir.join("official-operator"))?;
    let owner_key = NodeIdentity::load_or_create(&args.data_dir.join("dev-owner"))?;
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

    let mut audit = Vec::new();
    let mut audit_push = |action: &str, subject: &str, detail: Option<String>| {
        audit.push(AuditEvent {
            at: now.clone(),
            actor: "root".into(),
            action: action.into(),
            subject: subject.into(),
            detail,
        });
    };

    // --- Official TLD records (root-managed) ---
    let mut tlds = BTreeMap::new();
    let sign_tld = |mut rec: TldRecord| -> anyhow::Result<TldRecord> {
        rec.signature = Some(root_key.sign(&rec.signable_bytes()?));
        Ok(rec)
    };
    for (tld, purpose) in federate_naming::FEDERATE_TLDS {
        // Official TLDs may use reserved names (e.g. .fed) but must never
        // collide with public IANA DNS or policy blocks.
        let name = blocklists.validate_new_tld(tld, true)?;
        let rec = sign_tld(TldRecord {
            tld: name.clone(),
            status: TldStatus::Official,
            mode: TldMode::Official,
            owner_public_key: root_key.node_id(),
            operator_public_key: operator_key.node_id(),
            operator_name: "Federate Network (root-managed)".into(),
            registry_type: RegistryType::RootManaged,
            registry_endpoint: None,
            registry_manifest_hash: None,
            policy_hash: None,
            pricing: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            expires_at: None,
            notes: Some(purpose.to_string()),
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        })?;
        audit_push("tld.official.create", &format!(".{name}"), None);
        tlds.insert(name, rec);
    }

    // Example delegated TLD (seed data; marketplace/payment arrive later).
    // Demonstrates root → operator delegation; delegated resolution itself
    // is phase 6, so resolving under it returns DelegatedRegistryNotImplemented.
    let femboy_operator = NodeIdentity::load_or_create(&args.data_dir.join("op-femboy"))?;
    let femboy = blocklists.validate_new_tld("femboy", false)?;
    tlds.insert(
        femboy.clone(),
        sign_tld(TldRecord {
            tld: femboy.clone(),
            status: TldStatus::Delegated,
            mode: TldMode::Delegated,
            owner_public_key: femboy_operator.node_id(),
            operator_public_key: femboy_operator.node_id(),
            operator_name: "example delegated operator".into(),
            registry_type: RegistryType::DelegatedHttp,
            registry_endpoint: Some("https://registry.femboy.example (placeholder)".into()),
            registry_manifest_hash: None,
            policy_hash: None,
            pricing: Some(serde_json::json!({ "note": "pricing metadata placeholder; no payments in this phase" })),
            created_at: now.clone(),
            updated_at: now.clone(),
            expires_at: Some("2027-07-03T00:00:00Z".into()),
            notes: Some("Seed example of a delegated TLD. Domains under it are issued by the operator, not by Federate.".into()),
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        })?,
    );
    audit_push(
        "tld.delegate",
        ".femboy",
        Some("seed example delegation".into()),
    );

    // --- Sites: manifests (owner-signed) + domain records (operator-signed) ---
    let mut manifests = HashMap::new();
    let mut blocks = HashMap::new();
    let mut domains = BTreeMap::new();

    for entry in std::fs::read_dir(&args.sites_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let Some((label, tld)) = dir_to_domain(&dir_name) else {
            tracing::warn!("skipping sites/{dir_name}: not label-tld shaped");
            continue;
        };
        let Ok(tld) = validate_tld_name(&tld) else {
            tracing::warn!("skipping sites/{dir_name}: invalid TLD");
            continue;
        };
        if !tlds.contains_key(&tld) {
            tracing::warn!("skipping sites/{dir_name}: TLD .{tld} not in root registry");
            continue;
        }
        let label = federate_naming::validate_label(&label)?;
        let domain = format!("{label}.{tld}");

        let mut files = BTreeMap::new();
        for file in walkdir::WalkDir::new(entry.path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let rel = file
                .path()
                .strip_prefix(entry.path())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(file.path())?;
            let hash = federate_storage::hash_bytes(&bytes);
            blocks.insert(hash.clone(), bytes);
            files.insert(rel, hash);
        }
        if !files.contains_key("index.html") {
            tracing::warn!("skipping {domain}: no index.html");
            continue;
        }

        // Manifest signed by the domain owner key.
        let mut manifest = Manifest {
            domain: domain.clone(),
            version: 1,
            entry: "index.html".into(),
            files,
            owner_public_key: owner_key.node_id(),
            created_at: now.clone(),
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        manifest.signature = Some(owner_key.sign(&manifest.signable_bytes()?));
        let bytes = serde_json::to_vec(&manifest)?;
        let manifest_hash = federate_storage::hash_bytes(&bytes);
        manifests.insert(manifest_hash.clone(), bytes);

        // Domain record signed by the official TLD operator key.
        let mut record = DomainRecord {
            domain: domain.clone(),
            tld: tld.clone(),
            label,
            owner_public_key: owner_key.node_id(),
            target_type: TargetType::Manifest,
            manifest_hash,
            service_id: None,
            node_id: None,
            status: DomainStatus::Active,
            created_at: now.clone(),
            updated_at: now.clone(),
            expires_at: None,
            renewal: None,
            pricing: None,
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        record.signature = Some(operator_key.sign(&record.signable_bytes()?));
        audit_push("domain.register", &domain, None);
        domains.insert(domain.clone(), record);
        tracing::info!("published {domain}");
    }

    // --- Signed root zone ---
    // Version must be monotonic across restarts (daemons reject a zone older
    // than one they already verified; that is the rollback protection), so derive it
    // from the wall clock instead of hardcoding.
    let root_version = chrono::Utc::now().timestamp().max(0) as u64;
    let mut root = RootZone {
        network: federate_core::NETWORK_NAME.into(),
        root_version,
        generated_at: now.clone(),
        root_public_key: root_key.node_id(),
        tlds,
        domains,
        audit,
        signature_algorithm: SIGNATURE_ALGORITHM.into(),
        signature: None,
    };
    root.signature = Some(root_key.sign(&root.signable_bytes()?));
    root.verify(&root_key.node_id())?; // self-check before serving

    // Node registrations survive restarts (and nodes re-register every ~60s).
    let directory = federate_directory::Directory::with_persistence(
        Some(root.root_public_key.clone()),
        args.data_dir.join("directory-nodes.json"),
    );

    Ok(Store {
        root,
        blocklists,
        manifests,
        blocks,
        node_id: node_identity.node_id(),
        started_at: now,
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
    tracing::info!(
        "root zone signed: {} TLDs, {} domains, {} blocks (root key {})",
        store.root.tlds.len(),
        store.root.domains.len(),
        store.blocks.len(),
        store.root.root_public_key
    );

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
        .route("/v1/manifest/:hash", get(manifest))
        .route("/v1/block/:hash", get(block))
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
         <p>Bootstrap/control-plane node and Federate Root Registry.</p>\
         <p>Endpoints: /health /v1/status /v1/bootstrap /v1/root /v1/tlds /v1/tld/:tld \
         /v1/tld-check/:tld /v1/blocked /v1/reserved /v1/domains?tld= /v1/domain/:fqdn \
         /v1/manifest/:hash /v1/block/:hash</p>\
         <p>All registry data is signed; daemons verify, they do not trust this server.</p>",
    )
}

async fn status(State(s): State<Arc<Store>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "node": "node-1",
        "node_id": s.node_id,
        "started_at": s.started_at,
        "root_version": s.root.root_version,
        "root_public_key": s.root.root_public_key,
        "tlds": s.root.tlds.len(),
        "domains": s.root.domains.len(),
        "manifests": s.manifests.len(),
        "blocks": s.blocks.len(),
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
    Json(serde_json::json!({
        "network": s.root.network,
        "root_url": "/v1/root",
        "root_version": s.root.root_version,
        "root_public_key": s.root.root_public_key,
        "root_mirrors": mirrors,
        "dns_nodes": dns,
        "gateway_nodes": gateways,
        "directory_nodes": [],
        "bootstrap_nodes": bootstraps,
    }))
}

async fn root_zone(State(s): State<Arc<Store>>) -> Json<RootZone> {
    Json(s.root.clone())
}

async fn tlds(State(s): State<Arc<Store>>) -> Json<serde_json::Value> {
    Json(serde_json::json!(s.root.tlds.values().collect::<Vec<_>>()))
}

async fn tld_record(
    State(s): State<Arc<Store>>,
    AxPath(tld): AxPath<String>,
) -> Result<Json<TldRecord>, StatusCode> {
    let tld = tld.trim_start_matches('.').to_ascii_lowercase();
    s.root
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
    if let Some(rec) = s.root.lookup_tld(&tld) {
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
        "reason": format!(".{tld} is available for a future TLD application (marketplace not implemented yet)"),
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
    let list: Vec<_> = s
        .root
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
    s.root
        .lookup(&fqdn.to_ascii_lowercase())
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn manifest(
    State(s): State<Arc<Store>>,
    AxPath(hash): AxPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    s.manifests
        .get(&hash)
        .cloned()
        .map(|b| ([(header::CONTENT_TYPE, "application/json")], b))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn block(
    State(s): State<Arc<Store>>,
    AxPath(hash): AxPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    s.blocks
        .get(&hash)
        .cloned()
        .map(|b| ([(header::CONTENT_TYPE, "application/octet-stream")], b))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn peers_stub() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "peers": [], "note": "peer/CDN discovery arrives in phase 5" }))
}

async fn applications_stub() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "applications": [],
        "note": "TLD applications/approval arrive in marketplace phase 2; see docs/tld-marketplace-roadmap.md"
    }))
}
