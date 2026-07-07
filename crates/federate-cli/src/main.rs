//! federate: CLI for the Federate Network.
//!
//! Registry/verification commands talk to Node 1 (bootstrap URL) and verify
//! signatures locally; the server is a distributor of signed data, not a
//! trusted authority. Daemon commands talk to the local daemon API.

use clap::{Parser, Subcommand};
use federate_core::{DaemonConfig, DEFAULT_API_ADDR, DEFAULT_BOOTSTRAP_URL};
use federate_identity::NodeIdentity;
use federate_manifest::Manifest;
use federate_root::RootZone;

#[derive(Parser)]
#[command(name = "federate", about = "Federate Network CLI", version)]
struct Cli {
    /// Daemon API address
    #[arg(long, global = true, default_value = DEFAULT_API_ADDR)]
    api: String,
    /// Node 1 bootstrap URL
    #[arg(long, global = true, default_value = DEFAULT_BOOTSTRAP_URL)]
    bootstrap: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show daemon status
    Status,
    /// Run full diagnostics
    Doctor,
    /// Resolve a Federate domain or URI (federate resolve fed://joao.pagina)
    Resolve { domain: String },
    /// Parse and explain a Federate URI (fed://domain/path?query)
    InspectUri { uri: String },
    /// Fetch verified content for a Federate URI (full signature/hash chain)
    Fetch {
        /// fed://domain/path or bare domain
        uri: String,
        /// Write the body to a file instead of stdout
        #[arg(long)]
        output: Option<std::path::PathBuf>,
        /// Pinned Federate Root public key (hex)
        #[arg(long)]
        root_key: Option<String>,
        /// Print each resolution/fetch step (providers, transport, checks)
        #[arg(long)]
        trace: bool,
        /// Native protocol provider to prefer, host:port (repeatable).
        /// Providers advertised by the bootstrap node are added automatically.
        #[arg(long = "provider")]
        providers: Vec<String>,
    },
    /// List known providers for a content block hash
    Providers { hash: String },
    /// Root registry commands
    Root {
        #[command(subcommand)]
        cmd: RootCmd,
    },
    /// TLD registry commands
    Tld {
        #[command(subcommand)]
        cmd: TldCmd,
    },
    /// List Federate TLDs and their purposes (alias: `federate tld list`)
    Tlds,
    /// Domain registry commands
    Domain {
        #[command(subcommand)]
        cmd: DomainCmd,
    },
    /// Manifest commands
    Manifest {
        #[command(subcommand)]
        cmd: ManifestCmd,
    },
    /// Key commands
    Key {
        #[command(subcommand)]
        cmd: KeyCmd,
    },
    /// Local cache commands
    Cache {
        #[command(subcommand)]
        cmd: CacheCmd,
    },
    /// Node commands (run/register/inspect Federate infrastructure nodes)
    Node {
        #[command(subcommand)]
        cmd: NodeCmd,
    },
    /// DNS node commands
    Dns {
        #[command(subcommand)]
        cmd: DnsCmd,
    },
    /// Gateway node commands
    Gateway {
        #[command(subcommand)]
        cmd: GatewayCmd,
    },
    /// Node directory commands
    Directory {
        #[command(subcommand)]
        cmd: DirectoryCmd,
    },
    /// Delegated TLD registry commands (inspect/verify operator registries)
    DelegatedRegistry {
        #[command(subcommand)]
        cmd: DelegatedRegistryCmd,
    },
    /// TLD operator commands: sign domain records and build/verify the
    /// signed registry of a delegated TLD from your own machine
    Operator {
        #[command(subcommand)]
        cmd: OperatorCmd,
    },
    /// Site owner commands: package a site into content-addressed blocks
    /// plus an owner-signed manifest
    Site {
        #[command(subcommand)]
        cmd: SiteCmd,
    },
    /// Publish a site to the network through Node 1's signed ingest API
    Publish {
        #[command(subcommand)]
        cmd: PublishCmd,
    },
    /// Persistent root registry commands (status, audit, snapshots, ingest)
    Registry {
        #[command(subcommand)]
        cmd: RegistryCmd,
    },
    /// Signed mutation commands (nonce challenges, inspection)
    Mutation {
        #[command(subcommand)]
        cmd: MutationCmd,
    },
    /// Register this machine to open fed:// links in the browser
    /// (macOS, Linux, Windows; per-user, no admin rights, no signing)
    Handler {
        #[command(subcommand)]
        cmd: HandlerCmd,
    },
    /// Open a Federate domain in the default browser (portless URL)
    Open { domain: String },
    /// Show local node identity
    Identity,
    /// Check whether port 80 can be bound
    PortCheck,
}

#[derive(Subcommand)]
enum RootCmd {
    /// Print the root zone
    Show,
    /// Fetch the root zone and verify the full signature chain
    Verify,
    /// Initialize an EMPTY persistent registry (explicit first step; creates
    /// zero TLDs). Run before starting federate-server for the first time.
    Init {
        /// Node data dir (keys live here; registry goes to <dir>/registry)
        #[arg(long, default_value = ".federate-server")]
        data_dir: std::path::PathBuf,
    },
    /// Create TLD records from an external seed file (e.g.
    /// seeds/official-tlds.toml) through signed, audited mutations. Refuses
    /// when the registry already holds TLDs; --force adds missing entries
    /// only, never overwrites. Run with federate-server STOPPED.
    Seed {
        /// TOML seed file with [[tlds]] entries (name, mode, purpose)
        #[arg(long)]
        file: std::path::PathBuf,
        #[arg(long, default_value = ".federate-server")]
        data_dir: std::path::PathBuf,
        /// Authoritative IANA/public TLD blocklist file
        #[arg(long, default_value = "blocked_tlds.txt")]
        blocked_tlds: std::path::PathBuf,
        /// Reserved/policy/brand-safety blocklist dir
        #[arg(long, default_value = "data/blocked")]
        blocked_dir: std::path::PathBuf,
        /// Add missing seed entries to an already-populated registry
        #[arg(long)]
        force: bool,
    },
    /// Registry status: the local data dir when --data-dir is given,
    /// otherwise the bootstrap node's /v1/registry/status
    Status {
        #[arg(long)]
        data_dir: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
enum TldCmd {
    /// List all TLDs in the root registry
    List,
    /// Check whether a TLD name is available / blocked / reserved / taken
    Check { tld: String },
    /// Show the full record for a TLD
    Whois { tld: String },
    /// Apply for a new TLD (marketplace phase 2; validates only for now)
    Apply { tld: String },
    /// Approve a pending TLD (admin/seed-data-only in this phase)
    Approve {
        tld: String,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        operator: String,
    },
    /// Create an official root-managed TLD at runtime (Federate Root Key;
    /// signed mutation). Delegated TLDs use `federate tld delegate`.
    Create {
        tld: String,
        /// Only "official" is valid here
        #[arg(long, default_value = "official")]
        mode: String,
        /// Human purpose stored in the record
        #[arg(long)]
        purpose: String,
        /// Directory holding the Federate Root Key
        #[arg(long)]
        key_dir: std::path::PathBuf,
    },
    /// Block a TLD name (Federate Root Key; signed mutation creating a
    /// non-resolvable blocked record)
    Block {
        tld: String,
        #[arg(long)]
        reason: String,
        /// Directory holding the Federate Root Key
        #[arg(long)]
        key_dir: std::path::PathBuf,
    },
    /// Reserve a TLD name (Federate Root Key; signed mutation creating a
    /// non-resolvable reserved record)
    Reserve {
        tld: String,
        #[arg(long)]
        reason: String,
        /// Directory holding the Federate Root Key
        #[arg(long)]
        key_dir: std::path::PathBuf,
    },
    /// Verify a TLD record's signature against the root key
    Verify { tld: String },
    /// Delegate a TLD to an operator at runtime (requires the Federate Root
    /// Key; sends a signed mutation to Node 1)
    Delegate {
        tld: String,
        /// TLD owner public key (hex, 64 chars)
        #[arg(long)]
        owner: String,
        /// TLD operator public key (hex, 64 chars)
        #[arg(long)]
        operator: String,
        /// Directory holding the Federate Root Key (identity.key inside)
        #[arg(long)]
        key_dir: std::path::PathBuf,
        /// Operator display name
        #[arg(long)]
        operator_name: Option<String>,
        /// Registry distribution: delegated_manifest | delegated_native |
        /// delegated_http
        #[arg(long, default_value = "delegated_manifest")]
        registry_type: String,
        /// Registry endpoint (delegated_native/delegated_http modes)
        #[arg(long)]
        endpoint: Option<String>,
        /// Delegation expiry (RFC 3339)
        #[arg(long)]
        expires: Option<String>,
    },
}

#[derive(Subcommand)]
enum DomainCmd {
    /// List domains (optionally filtered by TLD)
    List {
        #[arg(long)]
        tld: Option<String>,
    },
    /// Check whether a domain is available
    Check { domain: String },
    /// Show the full record for a domain
    Whois { domain: String },
    /// Register a domain (marketplace phase; validates only for now)
    Register { domain: String },
    /// Verify a domain record's signature chain
    Verify { domain: String },
    /// Point your domain at a new manifest hash (owner-signed mutation).
    /// The manifest must already be on Node 1 (publish the package first).
    Update {
        domain: String,
        /// New manifest hash (BLAKE3 hex)
        #[arg(long)]
        manifest: String,
        /// Domain owner key directory
        #[arg(long, default_value = ".federate-owner")]
        key_dir: std::path::PathBuf,
    },
    /// Suspend a domain (TLD operator key or Federate Root Key)
    Suspend {
        domain: String,
        /// Directory holding the operator or root key
        #[arg(long)]
        key_dir: std::path::PathBuf,
    },
    /// Reinstate a suspended domain (TLD operator key or Federate Root Key)
    Reinstate {
        domain: String,
        /// Directory holding the operator or root key
        #[arg(long)]
        key_dir: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
enum PublishCmd {
    /// Package a site directory and submit it to Node 1: blocks +
    /// owner-signed manifest + signed publish mutation, in one step
    Package {
        /// Site directory (must contain index.html)
        dir: std::path::PathBuf,
        /// Domain to publish under, e.g. joao.pagina
        #[arg(long)]
        domain: String,
        /// Owner key directory (identity.key inside; created on first use)
        #[arg(long, default_value = ".federate-owner")]
        key_dir: std::path::PathBuf,
        /// Manifest version
        #[arg(long, default_value_t = 1)]
        version: u64,
    },
}

#[derive(Subcommand)]
enum RegistryCmd {
    /// Submit a pre-built package directory (from `federate site package`)
    /// to Node 1's ingest endpoint, signing the publish mutation
    SubmitPackage {
        /// Package directory (blocks/ + manifest file named by its hash)
        package: std::path::PathBuf,
        /// Owner key directory (must be the key that signed the manifest)
        #[arg(long, default_value = ".federate-owner")]
        key_dir: std::path::PathBuf,
    },
    /// Show persistent registry status (version, counts, mutation history)
    Status,
    /// Show the signed audit log (most recent events)
    Audit {
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Ask Node 1 to write a root zone snapshot and report it
    Snapshot,
    /// Ask Node 1 to self-verify the whole persistent registry
    Verify,
    /// One-time migration of a legacy JSON registry (state.json + JSONL
    /// logs) into the redb database. Validates every signature first and
    /// refuses on any failure; old files move to legacy-json-backup/.
    /// Run with federate-server STOPPED.
    MigrateJsonToRedb {
        #[arg(long, default_value = ".federate-server")]
        data_dir: std::path::PathBuf,
    },
    /// Copy the registry database to a backup file (server stopped).
    /// Content stores (manifests/, blocks/) and private keys are separate;
    /// see docs/en-US/backups.md.
    Backup {
        #[arg(long)]
        output: std::path::PathBuf,
        #[arg(long, default_value = ".federate-server")]
        data_dir: std::path::PathBuf,
    },
    /// Restore the registry database from a backup file and fully
    /// re-verify it against the root key (server stopped).
    Restore {
        #[arg(long)]
        input: std::path::PathBuf,
        #[arg(long, default_value = ".federate-server")]
        data_dir: std::path::PathBuf,
        /// Replace an existing database
        #[arg(long)]
        force: bool,
    },
    /// Embedded database inspection (server stopped)
    Db {
        #[command(subcommand)]
        cmd: RegistryDbCmd,
    },
}

#[derive(Subcommand)]
enum RegistryDbCmd {
    /// Table counts and file size of registry.redb
    Stats {
        #[arg(long, default_value = ".federate-server")]
        data_dir: std::path::PathBuf,
    },
    /// Open the database and verify everything: zone signature, delegated
    /// registries, content hashes, audit signatures, table consistency
    Verify {
        #[arg(long, default_value = ".federate-server")]
        data_dir: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
enum MutationCmd {
    /// Request a single-use mutation nonce (challenge-response)
    Nonce,
    /// Inspect an applied mutation by id
    Inspect { id: String },
}

#[derive(Subcommand)]
enum HandlerCmd {
    /// Register the fed:// URL scheme for the current user
    Install,
    /// Remove the fed:// registration
    Uninstall,
    /// Show whether fed:// is registered
    Status,
}

#[derive(Subcommand)]
enum ManifestCmd {
    /// Fetch a domain's manifest and verify hash + owner signature
    Verify { domain: String },
}

#[derive(Subcommand)]
enum KeyCmd {
    /// Generate a new Ed25519 keypair in a directory
    Generate {
        #[arg(long, default_value = ".")]
        dir: std::path::PathBuf,
    },
    /// Inspect the local identity key
    Inspect,
}

#[derive(Subcommand)]
enum CacheCmd {
    List,
    Clear,
}

#[derive(Subcommand)]
enum NodeCmd {
    /// Sign and send a registration to the node directory (from a config file)
    Register {
        #[arg(long, default_value = "federate.toml")]
        config: std::path::PathBuf,
    },
    /// Query a node's /status endpoint
    Status {
        /// Node health API base URL, e.g. http://45.1.1.1:8080
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        node: String,
    },
    /// Handshake a node over the NATIVE Federate protocol and print its
    /// status (no HTTP involved)
    Ping {
        /// Native protocol address, host:port (default port is 4077)
        #[arg(long, default_value = "127.0.0.1:4077")]
        addr: String,
    },
    /// Query a node's /roles endpoint
    Roles {
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        node: String,
    },
    /// Query a node's /health endpoint
    Health {
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        node: String,
    },
    /// List all nodes known to the directory
    List {
        #[arg(long)]
        role: Option<federate_directory::NodeRole>,
    },
    /// Run a multi-role node (spawns federate-noded)
    Run {
        /// Roles to run, e.g. --roles gateway,dns,cdn
        #[arg(long, value_delimiter = ',')]
        roles: Vec<federate_directory::NodeRole>,
        #[arg(long, default_value = "federate.toml")]
        config: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
enum DnsCmd {
    /// Send a real DNS query to a Federate DNS node and print the answers
    Test {
        domain: String,
        /// DNS node address
        #[arg(long, default_value = "127.0.0.1:5353")]
        server: String,
    },
}

#[derive(Subcommand)]
enum GatewayCmd {
    /// Request a domain through a gateway node (Host-header test)
    Test {
        domain: String,
        /// Gateway base URL
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        gateway: String,
    },
}

#[derive(Subcommand)]
enum OperatorCmd {
    /// Sign a domain record with the TLD operator key
    SignRecord {
        /// Full domain, e.g. eu.femboy
        domain: String,
        /// Domain owner public key (hex, 64 chars): the key that signs the
        /// site manifest
        #[arg(long)]
        owner: String,
        /// Hash of the owner-signed site manifest (BLAKE3 hex, from
        /// `federate site package`)
        #[arg(long)]
        manifest_hash: String,
        /// Operator key directory (identity.key inside; created on first use)
        #[arg(long, default_value = ".federate-operator")]
        key_dir: std::path::PathBuf,
        /// Optional record expiry (RFC 3339)
        #[arg(long)]
        expires: Option<String>,
        /// Output file (default: <domain>.record.json)
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Assemble signed records into the signed registry of a delegated TLD
    BuildRegistry {
        /// The delegated TLD this registry is for
        tld: String,
        /// Directory of *.record.json files (from `operator sign-record`)
        #[arg(long, default_value = ".")]
        records: std::path::PathBuf,
        #[arg(long, default_value = ".federate-operator")]
        key_dir: std::path::PathBuf,
        /// Registry version; MUST increase on every change (clients enforce
        /// rollback protection). Default: current unix time.
        #[arg(long)]
        version: Option<u64>,
        /// Output file (default: <tld>.registry.json)
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Verify a registry file offline: operator signature plus every record
    VerifyRegistry {
        /// Registry file (from `operator build-registry`)
        file: std::path::PathBuf,
        /// Expected TLD
        #[arg(long)]
        tld: String,
        /// Expected operator public key (hex). Pass the key from the
        /// root-signed TLD record for a real verification; omitted, the key
        /// claimed inside the file is used (self-consistency check only).
        #[arg(long)]
        operator: Option<String>,
    },
}

#[derive(Subcommand)]
enum SiteCmd {
    /// Package a site directory: content-addressed blocks + owner-signed
    /// manifest. Prints the manifest hash the operator needs for the record.
    Package {
        /// Site directory (must contain index.html)
        dir: std::path::PathBuf,
        /// Domain this site is published under, e.g. eu.femboy
        #[arg(long)]
        domain: String,
        /// Owner key directory (identity.key inside; created on first use)
        #[arg(long, default_value = ".federate-owner")]
        key_dir: std::path::PathBuf,
        /// Output directory (default: <dir>.federate-package)
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Manifest version
        #[arg(long, default_value_t = 1)]
        version: u64,
        /// Also install blocks + manifest into a node data directory so a
        /// local federate-noded (storage/cdn role) serves them immediately
        #[arg(long)]
        install: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
enum DelegatedRegistryCmd {
    /// Show a delegated TLD's registry: delegation, distribution mode, domains
    Inspect { tld: String },
    /// Verify the whole delegation chain: root key -> TLD record -> operator
    /// registry -> every domain record
    Verify { tld: String },
}

#[derive(Subcommand)]
enum DirectoryCmd {
    /// List nodes in the directory, optionally by role
    List {
        #[arg(long)]
        role: Option<federate_directory::NodeRole>,
        /// Only healthy (online/degraded) nodes
        #[arg(long)]
        healthy: bool,
    },
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Shared HTTP client. Every CLI request gets a timeout so a dead daemon or
/// node makes the command fail with a message instead of hanging forever
/// (reqwest has no timeout by default).
fn http() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("reqwest client")
    })
}

async fn api_get(api: &str, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("http://{api}{path}");
    let resp = http().get(&url).send().await.map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

async fn api_delete(api: &str, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("http://{api}{path}");
    let resp = http()
        .delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

async fn node_get(bootstrap: &str, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}{path}", bootstrap.trim_end_matches('/'));
    let resp = http().get(&url).send().await.map_err(|e| e.to_string())?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("not found".into());
    }
    resp.json().await.map_err(|e| e.to_string())
}

async fn fetch_root(bootstrap: &str) -> Result<RootZone, String> {
    let v = node_get(bootstrap, "/v1/root").await?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

fn pretty(v: &serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap());
}

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

// ---------------------------------------------------------------------------

/// Connection context shared by subcommand handlers.
struct Ctx {
    api: String,
    bootstrap: String,
}

/// Accept both native URIs (`fed://x.y/path`) and compatibility spellings
/// (`x.y`, `http://x.y/path`); everything normalizes to a FederateUri, so
/// every subcommand works for any valid domain under any valid TLD.
fn parse_target(input: &str) -> federate_uri::FederateUri {
    let normalized = if input.starts_with("fed://") {
        input.to_string()
    } else {
        format!("fed://{}", input.trim_start_matches("http://"))
    };
    federate_uri::FederateUri::parse(&normalized)
        .unwrap_or_else(|e| die(&format!("invalid Federate address: {e}")))
}

/// Native-path fetch: full root -> TLD -> domain -> manifest -> block chain
/// with local verification, no daemon required. Body goes to stdout (or
/// --output); status goes to stderr so pipes stay clean.
/// Build a local verifying resolver (native-first, HTTP fallback). Native
/// providers are discovered from the bootstrap answer and merged with any
/// explicitly passed ones.
async fn build_resolver(
    cli: &Ctx,
    root_key: Option<String>,
    mut native_providers: Vec<String>,
) -> federate_resolution::Resolver {
    let data_dir = DaemonConfig::default_data_dir();
    std::fs::create_dir_all(&data_dir).ok();
    if let Ok(info) = federate_bootstrap::BootstrapClient::new()
        .fetch(&cli.bootstrap)
        .await
    {
        for provider in info.native_providers(&cli.bootstrap) {
            if !native_providers.contains(&provider) {
                native_providers.push(provider);
            }
        }
    }
    federate_resolution::Resolver::new(
        federate_client::NodeClient::new(&cli.bootstrap),
        &data_dir,
        root_key,
    )
    .unwrap_or_else(|e| die(&format!("cannot initialize resolver: {e}")))
    .with_directory(
        federate_directory::DirectoryClient::new(&cli.bootstrap),
        None,
    )
    .with_native_providers(native_providers)
}

async fn fetch_cmd(
    cli: &Ctx,
    uri: federate_uri::FederateUri,
    output: Option<std::path::PathBuf>,
    root_key: Option<String>,
    trace_on: bool,
    native_providers: Vec<String>,
) {
    use federate_resolution::{Resolved, Trace};
    let resolver = build_resolver(cli, root_key, native_providers).await;
    let trace = Trace::default();
    let outcome = if trace_on {
        resolver.resolve_uri_traced(&uri, &trace).await
    } else {
        resolver.resolve_uri(&uri).await
    };
    if trace_on {
        for step in trace.events() {
            eprintln!("  -> {step}");
        }
    }
    match outcome {
        Ok(Resolved::Content {
            bytes, mime, hash, ..
        }) => {
            eprintln!(
                "[ok] {uri}: {} bytes, {mime}, block {hash} (chain verified)",
                bytes.len()
            );
            match output {
                Some(path) => {
                    std::fs::write(&path, &bytes)
                        .unwrap_or_else(|e| die(&format!("cannot write {}: {e}", path.display())));
                    eprintln!("saved to {}", path.display());
                }
                None => {
                    use std::io::Write;
                    std::io::stdout().write_all(&bytes).ok();
                }
            }
        }
        Ok(other) => die(&format!("{uri} did not resolve to content: {other:?}")),
        Err(e) => die(&format!("fetch failed for {uri}: {e}")),
    }
}

#[tokio::main]
async fn main() {
    let Cli {
        api,
        bootstrap,
        cmd,
    } = Cli::parse();
    let cli = Ctx { api, bootstrap };
    match cmd {
        Cmd::Status => match api_get(&cli.api, "/status").await {
            Ok(v) => pretty(&v),
            Err(e) => die(&format!(
                "daemon not reachable at {} ({e})\nstart it with: federated",
                cli.api
            )),
        },
        Cmd::Doctor => doctor(&cli).await,
        Cmd::Resolve { domain } => {
            let uri = parse_target(&domain);
            match api_get(
                &cli.api,
                &format!("/resolve?domain={}&path={}", uri.fqdn(), uri.path),
            )
            .await
            {
                Ok(v) => pretty(&v),
                Err(e) => die(&format!(
                    "daemon not reachable ({e}); is federated running?\nno daemon? try: federate fetch {uri}"
                )),
            }
        }
        Cmd::InspectUri { uri } => {
            let parsed = parse_target(&uri);
            println!("canonical : {parsed}");
            println!("scheme    : fed");
            println!("domain    : {}", parsed.fqdn());
            println!("  label   : {}", parsed.domain.name);
            println!("  tld     : .{}", parsed.domain.tld);
            println!("path      : {}", parsed.path);
            println!(
                "query     : {}",
                parsed.query.as_deref().unwrap_or("(none)")
            );
            println!(
                "note      : syntax only; whether {} exists is decided by the signed root zone",
                parsed.fqdn()
            );
        }
        Cmd::Fetch {
            uri,
            output,
            root_key,
            trace,
            providers,
        } => fetch_cmd(&cli, parse_target(&uri), output, root_key, trace, providers).await,
        Cmd::Providers { hash } => providers_cmd(&cli, &hash).await,
        Cmd::Root { cmd } => root_cmd(&cli, cmd).await,
        Cmd::Tlds | Cmd::Tld { cmd: TldCmd::List } => match fetch_root(&cli.bootstrap).await {
            Ok(zone) => {
                for rec in zone.tlds.values() {
                    println!(
                        ".{:<10} {:<10} {:<18} operator: {}",
                        rec.tld,
                        rec.status.as_str(),
                        format!("{:?}", rec.registry_type),
                        rec.operator_name
                    );
                }
            }
            Err(e) => die(&format!(
                "cannot fetch root zone from {} ({e})",
                cli.bootstrap
            )),
        },
        Cmd::Tld { cmd } => tld_cmd(&cli, cmd).await,
        Cmd::Domain { cmd } => domain_cmd(&cli, cmd).await,
        Cmd::Manifest {
            cmd: ManifestCmd::Verify { domain },
        } => manifest_verify(&cli, &domain).await,
        Cmd::Key { cmd } => key_cmd(cmd),
        Cmd::Cache { cmd } => {
            let res = match cmd {
                CacheCmd::List => api_get(&cli.api, "/cache/list").await,
                CacheCmd::Clear => api_delete(&cli.api, "/cache/clear").await,
            };
            match res {
                Ok(v) => pretty(&v),
                Err(e) => die(&format!("daemon not reachable ({e})")),
            }
        }
        Cmd::Node { cmd } => node_cmd(&cli, cmd).await,
        Cmd::Dns {
            cmd: DnsCmd::Test { domain, server },
        } => dns_test(&domain, &server).await,
        Cmd::Gateway {
            cmd: GatewayCmd::Test { domain, gateway },
        } => gateway_test(&domain, &gateway).await,
        Cmd::Directory {
            cmd: DirectoryCmd::List { role, healthy },
        } => directory_list(&cli, role, healthy).await,
        Cmd::DelegatedRegistry { cmd } => delegated_registry_cmd(&cli, cmd).await,
        Cmd::Operator { cmd } => operator_cmd(cmd),
        Cmd::Site { cmd } => site_cmd(cmd),
        Cmd::Publish { cmd } => publish_cmd(&cli, cmd).await,
        Cmd::Registry { cmd } => registry_cmd(&cli, cmd).await,
        Cmd::Mutation { cmd } => mutation_cmd(&cli, cmd).await,
        Cmd::Handler { cmd } => handler_cmd(cmd),
        Cmd::Open { domain } => {
            // Accepts fed://... and bare domains; opens the browser through
            // the HTTP compatibility gateway (portless URL). This runs as a
            // registered URL-scheme handler, so the argument is attacker-shaped
            // by definition: reject anything flag-like before it can be read as
            // an option, then rebuild the URL only from validated URI parts.
            if domain.starts_with('-') {
                die("open expects a fed:// address or bare domain, not a flag");
            }
            let uri = parse_target(&domain);
            let url = format!(
                "http://{}{}",
                uri.fqdn(),
                if uri.path == "/" { "" } else { &uri.path }
            );
            println!("opening {url}");
            // Every launcher below receives the URL as a single argv element,
            // never a shell string, so URL contents cannot inject arguments.
            #[cfg(target_os = "macos")]
            let cmd = ("open", vec![url.clone()]);
            #[cfg(target_os = "linux")]
            let cmd = ("xdg-open", vec![url.clone()]);
            #[cfg(target_os = "windows")]
            let cmd = (
                "rundll32",
                vec!["url.dll,FileProtocolHandler".to_string(), url.clone()],
            );
            if let Err(e) = std::process::Command::new(cmd.0).args(&cmd.1).spawn() {
                eprintln!("could not open browser: {e}; open {url} manually");
            }
        }
        Cmd::Identity => {
            let data_dir = DaemonConfig::default_data_dir();
            match NodeIdentity::load_or_create(&data_dir) {
                Ok(id) => {
                    println!("node id : {}", id.node_id());
                    println!("key file: {}", id.key_path().display());
                }
                Err(e) => eprintln!("identity error: {e}"),
            }
        }
        Cmd::PortCheck => port_check(),
    }
}

// ---------------------------------------------------------------------------
// root
// ---------------------------------------------------------------------------

async fn root_cmd(cli: &Ctx, cmd: RootCmd) {
    match cmd {
        RootCmd::Show => match api_get(&cli.api, "/root").await {
            Ok(v) => pretty(&v),
            Err(_) => match fetch_root(&cli.bootstrap).await {
                Ok(zone) => pretty(&serde_json::to_value(&zone).unwrap()),
                Err(e) => die(&format!("neither daemon nor Node 1 reachable ({e})")),
            },
        },
        RootCmd::Verify => {
            let zone = fetch_root(&cli.bootstrap)
                .await
                .unwrap_or_else(|e| die(&format!("cannot fetch root zone ({e})")));
            println!("root key : {}", zone.root_public_key);
            println!("version  : {}", zone.root_version);
            match zone.verify(&zone.root_public_key) {
                Ok(()) => {
                    println!(
                        "[ok] root zone signature valid; {} TLD records verified against root key",
                        zone.tlds.len()
                    );
                    // verify all root-managed domain records too
                    let mut bad = 0;
                    for (fqdn, rec) in &zone.domains {
                        let Some(tld) = zone.tlds.get(&rec.tld) else {
                            println!("[!!] {fqdn}: TLD missing");
                            bad += 1;
                            continue;
                        };
                        if let Err(e) = rec.verify(&tld.operator_public_key) {
                            println!("[!!] {fqdn}: {e}");
                            bad += 1;
                        }
                    }
                    println!(
                        "[{}] {} domain records verified against their TLD operator keys",
                        if bad == 0 { "ok" } else { "!!" },
                        zone.domains.len() - bad
                    );
                    println!(
                        "note: this verifies self-consistency of the served zone. Your daemon \
                         additionally pins the root key as a trust anchor (see docs/en-US/signatures.md)."
                    );
                    if bad > 0 {
                        std::process::exit(1);
                    }
                }
                Err(e) => die(&format!("[!!] root zone verification FAILED: {e}")),
            }
        }
        RootCmd::Init { data_dir } => {
            let root = NodeIdentity::load_or_create(&data_dir.join("root"))
                .unwrap_or_else(|e| die(&format!("cannot load/create root key: {e}")));
            match federate_mutation::init_empty_registry(&data_dir.join("registry"), &root) {
                Ok(store) => {
                    println!("[ok] empty registry initialized");
                    println!("     dir          : {}", store.dir().display());
                    println!("     root key     : {}", root.node_id());
                    println!(
                        "     root zone    : v{} (0 TLDs, 0 domains)",
                        store.zone().root_version
                    );
                    println!(
                        "\nnext: federate root seed --file seeds/official-tlds.toml --data-dir {}",
                        data_dir.display()
                    );
                }
                Err(e) => die(&format!("[!!] init failed: {e}")),
            }
        }
        RootCmd::Seed {
            file,
            data_dir,
            blocked_tlds,
            blocked_dir,
            force,
        } => root_seed(&file, &data_dir, &blocked_tlds, &blocked_dir, force),
        RootCmd::Status {
            data_dir: Some(dir),
        } => {
            let registry_dir = dir.join("registry");
            if !federate_mutation::RegistryStore::exists(&registry_dir) {
                println!("registry: NOT initialized at {}", registry_dir.display());
                println!(
                    "initialize it with: federate root init --data-dir {}",
                    dir.display()
                );
                return;
            }
            let root = NodeIdentity::load_or_create(&dir.join("root"))
                .unwrap_or_else(|e| die(&format!("cannot load root key: {e}")));
            let store = federate_mutation::RegistryStore::open(&registry_dir, &root.node_id())
                .unwrap_or_else(|e| die(&format!("[!!] cannot open registry: {e}")));
            println!("registry     : {}", registry_dir.display());
            println!("root key     : {}", store.zone().root_public_key);
            println!("root zone    : v{}", store.zone().root_version);
            println!("tlds         : {}", store.zone().tlds.len());
            println!("domains      : {}", store.zone().domains.len());
            println!("mutations    : {}", store.mutation_count());
            println!("audit events : {}", store.audit_count());
        }
        RootCmd::Status { data_dir: None } => {
            match node_get(&cli.bootstrap, "/v1/registry/status").await {
                Ok(v) => pretty(&v),
                Err(e) => die(&format!("cannot fetch registry status ({e})")),
            }
        }
    }
}

/// Offline seed: apply an external TOML seed file to the local registry
/// through the normal signed, audited mutation path. Never runs implicitly;
/// never overwrites existing records.
fn root_seed(
    file: &std::path::Path,
    data_dir: &std::path::Path,
    blocked_tlds: &std::path::Path,
    blocked_dir: &std::path::Path,
    force: bool,
) {
    eprintln!(
        "note: offline seeding writes the registry directly; run it with federate-server STOPPED."
    );
    let seed = federate_mutation::SeedFile::load(file)
        .unwrap_or_else(|e| die(&format!("cannot load seed file {}: {e}", file.display())));
    let root = NodeIdentity::load_or_create(&data_dir.join("root"))
        .unwrap_or_else(|e| die(&format!("cannot load root key: {e}")));
    let operator = NodeIdentity::load_or_create(&data_dir.join("official-operator"))
        .unwrap_or_else(|e| die(&format!("cannot load official operator key: {e}")));
    let blocklists = federate_root::Blocklists::load(blocked_tlds, blocked_dir)
        .unwrap_or_else(|e| die(&format!("cannot load blocklists: {e}")));
    let registry_dir = data_dir.join("registry");
    let mut store = if federate_mutation::RegistryStore::exists(&registry_dir) {
        federate_mutation::RegistryStore::open(&registry_dir, &root.node_id())
            .unwrap_or_else(|e| die(&format!("[!!] cannot open registry: {e}")))
    } else {
        federate_mutation::init_empty_registry(&registry_dir, &root)
            .unwrap_or_else(|e| die(&format!("[!!] cannot initialize registry: {e}")))
    };
    let ctx = federate_mutation::MutationContext {
        root: &root,
        official_operator: &operator,
        blocklists: &blocklists,
        now: chrono::Utc::now(),
    };
    match federate_mutation::apply_seed(&mut store, &seed, &ctx, force) {
        Ok(outcome) => {
            println!(
                "[ok] seed applied: {} TLD(s) created, {} already existed",
                outcome.created.len(),
                outcome.skipped_existing.len()
            );
            for name in &outcome.created {
                println!("  + .{name}");
            }
            for name in &outcome.skipped_existing {
                println!("  = .{name} (kept as-is)");
            }
            println!(
                "root zone now v{} ({} TLDs)",
                store.zone().root_version,
                store.zone().tlds.len()
            );
        }
        Err(e) => die(&format!("[!!] seed refused: {e}")),
    }
}

// ---------------------------------------------------------------------------
// tld
// ---------------------------------------------------------------------------

async fn tld_cmd(cli: &Ctx, cmd: TldCmd) {
    match cmd {
        TldCmd::List => unreachable!("handled above"),
        TldCmd::Check { tld } => {
            match node_get(&cli.bootstrap, &format!("/v1/tld-check/{tld}")).await {
                Ok(v) => {
                    let available = v["available"].as_bool().unwrap_or(false);
                    println!(
                        "[{}] .{} - {}",
                        if available {
                            "available"
                        } else {
                            v["verdict"].as_str().unwrap_or("?")
                        },
                        v["tld"].as_str().unwrap_or(&tld),
                        v["reason"].as_str().unwrap_or("")
                    );
                }
                Err(e) => die(&format!("cannot check TLD via {} ({e})", cli.bootstrap)),
            }
        }
        TldCmd::Whois { tld } => match node_get(&cli.bootstrap, &format!("/v1/tld/{tld}")).await {
            Ok(v) => pretty(&v),
            Err(_) => die(&format!(".{tld} not found in the Federate root registry")),
        },
        TldCmd::Verify { tld } => {
            let zone = fetch_root(&cli.bootstrap)
                .await
                .unwrap_or_else(|e| die(&format!("cannot fetch root zone ({e})")));
            let name = tld.trim_start_matches('.').to_ascii_lowercase();
            match zone.tlds.get(&name) {
                Some(rec) => match rec.verify(&zone.root_public_key) {
                    Ok(()) => println!(
                        "[ok] .{name} record signature valid (signed by Federate Root Key, status: {})",
                        rec.status.as_str()
                    ),
                    Err(e) => die(&format!("[!!] .{name} verification FAILED: {e}")),
                },
                None => die(&format!(".{name} not found in the root registry")),
            }
        }
        TldCmd::Apply { tld } => {
            match node_get(&cli.bootstrap, &format!("/v1/tld-check/{tld}")).await {
                Ok(v) if v["available"].as_bool() == Some(true) => {
                    println!(
                        "[ok] .{} passes naming rules and all blocklists.",
                        v["tld"].as_str().unwrap_or(&tld)
                    );
                    println!("TLD applications are not open yet (marketplace phase 2).");
                    println!(
                        "See docs/en-US/tld-marketplace-roadmap.md; no payments are implemented."
                    );
                }
                Ok(v) => die(&format!(
                    "cannot apply for .{}: {}",
                    v["tld"].as_str().unwrap_or(&tld),
                    v["reason"].as_str().unwrap_or("unavailable")
                )),
                Err(e) => die(&format!("cannot reach root registry ({e})")),
            }
        }
        TldCmd::Approve {
            tld,
            owner,
            operator,
        } => {
            println!("`federate tld approve` is superseded by runtime delegation.");
            println!("Run (with the Federate Root Key):");
            println!(
                "  federate tld delegate {tld} --owner {owner} --operator {operator} --key-dir <root-key-dir>"
            );
        }
        TldCmd::Delegate {
            tld,
            owner,
            operator,
            key_dir,
            operator_name,
            registry_type,
            endpoint,
            expires,
        } => {
            let tld = federate_naming::validate_tld_name(&tld)
                .unwrap_or_else(|e| die(&format!("invalid TLD: {e}")));
            let registry_type = match registry_type.as_str() {
                "delegated_manifest" => federate_naming::RegistryType::DelegatedManifest,
                "delegated_native" => federate_naming::RegistryType::DelegatedNative,
                "delegated_http" => federate_naming::RegistryType::DelegatedHttp,
                other => die(&format!(
                    "unsupported registry type '{other}' (use delegated_manifest, delegated_native, or delegated_http)"
                )),
            };
            let action = federate_mutation::MutationAction::DelegateTld {
                tld: tld.clone(),
                owner_public_key: owner,
                operator_public_key: operator,
                operator_name: operator_name.unwrap_or_else(|| format!("operator of .{tld}")),
                registry_type,
                registry_endpoint: endpoint,
                expires_at: expires,
            };
            sign_and_submit_mutation(cli, &key_dir, action).await;
        }
        TldCmd::Create {
            tld,
            mode,
            purpose,
            key_dir,
        } => {
            if mode != "official" {
                die("`federate tld create` only creates official TLDs; use `federate tld delegate` for delegated ones");
            }
            let tld = federate_naming::validate_tld_name(&tld)
                .unwrap_or_else(|e| die(&format!("invalid TLD: {e}")));
            let action = federate_mutation::MutationAction::CreateTld { tld, purpose };
            sign_and_submit_mutation(cli, &key_dir, action).await;
        }
        TldCmd::Block {
            tld,
            reason,
            key_dir,
        } => {
            let tld = federate_naming::validate_tld_name(&tld)
                .unwrap_or_else(|e| die(&format!("invalid TLD: {e}")));
            let action = federate_mutation::MutationAction::BlockTld { tld, reason };
            sign_and_submit_mutation(cli, &key_dir, action).await;
        }
        TldCmd::Reserve {
            tld,
            reason,
            key_dir,
        } => {
            let tld = federate_naming::validate_tld_name(&tld)
                .unwrap_or_else(|e| die(&format!("invalid TLD: {e}")));
            let action = federate_mutation::MutationAction::ReserveTld { tld, reason };
            sign_and_submit_mutation(cli, &key_dir, action).await;
        }
    }
}

// ---------------------------------------------------------------------------
// domain
// ---------------------------------------------------------------------------

async fn domain_cmd(cli: &Ctx, cmd: DomainCmd) {
    match cmd {
        DomainCmd::List { tld } => {
            let path = match &tld {
                Some(t) => format!("/v1/domains?tld={t}"),
                None => "/v1/domains".to_string(),
            };
            match node_get(&cli.bootstrap, &path).await {
                Ok(v) => {
                    for d in v.as_array().unwrap_or(&vec![]) {
                        println!(
                            "{:<20} {:<10} owner: {}…",
                            d["domain"].as_str().unwrap_or("?"),
                            d["status"].as_str().unwrap_or("?"),
                            &d["owner_public_key"].as_str().unwrap_or("?")
                                [..16.min(d["owner_public_key"].as_str().unwrap_or("?").len())]
                        );
                    }
                }
                Err(e) => die(&format!("cannot list domains ({e})")),
            }
        }
        DomainCmd::Check { domain } => {
            let parsed = federate_naming::FederateDomain::parse(&domain)
                .unwrap_or_else(|e| die(&format!("invalid domain: {e}")));
            let zone = fetch_root(&cli.bootstrap)
                .await
                .unwrap_or_else(|e| die(&format!("cannot fetch root zone ({e})")));
            match zone.tlds.get(&parsed.tld) {
                None => die(&format!(".{} does not exist in the Federate root registry", parsed.tld)),
                Some(t) if !t.status.is_resolvable() => die(&format!(
                    ".{} exists but is {}, so domains cannot be registered under it",
                    parsed.tld,
                    t.status.as_str()
                )),
                Some(_) => match zone.lookup(&parsed.fqdn()) {
                    Some(rec) => println!(
                        "[taken] {} is registered (status: {})",
                        parsed.fqdn(),
                        rec.status.as_str()
                    ),
                    None => println!(
                        "[available] {} is not registered (registration opens with the marketplace phases)",
                        parsed.fqdn()
                    ),
                },
            }
        }
        DomainCmd::Whois { domain } => {
            match node_get(&cli.bootstrap, &format!("/v1/domain/{domain}")).await {
                Ok(v) => {
                    pretty(&v);
                    // add operator context from the TLD record
                    if let Some(tld) = v["tld"].as_str() {
                        if let Ok(t) = node_get(&cli.bootstrap, &format!("/v1/tld/{tld}")).await {
                            println!(
                                "\nTLD .{tld} operator: {} ({})",
                                t["operator_name"].as_str().unwrap_or("?"),
                                t["operator_public_key"].as_str().unwrap_or("?")
                            );
                        }
                    }
                }
                Err(_) => die(&format!("{domain} not found in the Federate registry")),
            }
        }
        DomainCmd::Register { domain } => {
            let parsed = federate_naming::FederateDomain::parse(&domain)
                .unwrap_or_else(|e| die(&format!("invalid domain: {e}")));
            println!("[ok] {} passes naming rules.", parsed.fqdn());
            println!("Self-service registration is not open yet (marketplace phases).");
            println!("Official-TLD sites are published via sites/ on Node 1 for now.");
        }
        DomainCmd::Verify { domain } => {
            // Full local verification through the resolution engine: root
            // zone signature, TLD record, registry routing (root-managed or
            // delegated), and the operator signature on the domain record.
            let resolver = build_resolver(cli, None, vec![]).await;
            let rec = resolver
                .resolve_domain(&domain)
                .await
                .unwrap_or_else(|e| die(&format!("[!!] {domain}: {e}")));
            let registry_kind = resolver
                .root()
                .await
                .ok()
                .and_then(|z| {
                    z.lookup_tld(&rec.tld)
                        .map(|t| format!("{:?}", t.registry_type))
                })
                .unwrap_or_else(|| "?".into());
            println!(
                "[ok] {domain}: root zone, TLD record, registry, and domain record signatures all valid"
            );
            println!("     registry: {registry_kind} (.{})", rec.tld);
            println!("     owner: {}", rec.owner_public_key);
            println!("     manifest: {}", rec.manifest_hash);
        }
        DomainCmd::Update {
            domain,
            manifest,
            key_dir,
        } => {
            if !federate_storage::is_valid_hash(&manifest) {
                die("--manifest must be a 64-char BLAKE3 hex content address");
            }
            let action = federate_mutation::MutationAction::UpdateDomainManifest {
                domain,
                manifest_hash: manifest,
            };
            sign_and_submit_mutation(cli, &key_dir, action).await;
        }
        DomainCmd::Suspend { domain, key_dir } => {
            let action = federate_mutation::MutationAction::SetDomainStatus {
                domain,
                status: federate_naming::DomainStatus::Suspended,
            };
            sign_and_submit_mutation(cli, &key_dir, action).await;
        }
        DomainCmd::Reinstate { domain, key_dir } => {
            let action = federate_mutation::MutationAction::SetDomainStatus {
                domain,
                status: federate_naming::DomainStatus::Active,
            };
            sign_and_submit_mutation(cli, &key_dir, action).await;
        }
    }
}

// ---------------------------------------------------------------------------
// delegated registries
// ---------------------------------------------------------------------------

async fn delegated_registry_cmd(cli: &Ctx, cmd: DelegatedRegistryCmd) {
    let (tld, verify_mode) = match cmd {
        DelegatedRegistryCmd::Inspect { tld } => (tld, false),
        DelegatedRegistryCmd::Verify { tld } => (tld, true),
    };
    let tld = tld.trim_start_matches('.').to_ascii_lowercase();
    let resolver = build_resolver(cli, None, vec![]).await;
    let root = resolver
        .root()
        .await
        .unwrap_or_else(|e| die(&format!("cannot load a verified root zone ({e})")));
    let Some(tld_rec) = root.lookup_tld(&tld) else {
        die(&format!(".{tld} not found in the Federate root registry"));
    };

    println!("TLD            : .{tld}");
    println!("status         : {}", tld_rec.status.as_str());
    println!("registry type  : {:?}", tld_rec.registry_type);
    println!("owner key      : {}", tld_rec.owner_public_key);
    println!(
        "operator       : {} ({})",
        tld_rec.operator_name, tld_rec.operator_public_key
    );
    if let Some(hash) = &tld_rec.registry_manifest_hash {
        println!("registry hash  : {hash}");
    }
    if !tld_rec.registry_providers.is_empty() {
        println!("registry nodes : {}", tld_rec.registry_providers.join(", "));
    }
    if let Some(endpoint) = &tld_rec.registry_endpoint {
        println!("registry http  : {endpoint}");
    }
    if let Some(expires) = &tld_rec.expires_at {
        println!("expires        : {expires}");
    }

    if matches!(
        tld_rec.registry_type,
        federate_naming::RegistryType::RootManaged
    ) {
        println!(
            "\n.{tld} is root-managed: its domain records live in the signed root zone; \
             there is no delegated registry to inspect."
        );
        return;
    }

    if verify_mode {
        let trusted = resolver.trusted_root_key().await.unwrap_or_default();
        match tld_rec.verify(&trusted) {
            Ok(()) => {
                println!("\n[ok] TLD record signature valid (signed by the Federate Root Key)")
            }
            Err(e) => die(&format!("\n[!!] TLD record verification FAILED: {e}")),
        }
    }

    match resolver.tld_registry_by_name(&tld).await {
        Ok(registry) => {
            println!("\n[ok] registry signature valid (signed by the .{tld} operator key)");
            println!(
                "registry v{}, generated {}, {} domain(s):",
                registry.version,
                registry.generated_at,
                registry.domains.len()
            );
            let mut failed = 0usize;
            for (fqdn, rec) in &registry.domains {
                if verify_mode {
                    match rec.verify(&tld_rec.operator_public_key) {
                        Ok(()) => println!(
                            "  [ok] {fqdn:<24} {:<10} owner {}",
                            rec.status.as_str(),
                            rec.owner_public_key
                        ),
                        Err(e) => {
                            failed += 1;
                            println!("  [!!] {fqdn:<24} record verification FAILED: {e}");
                        }
                    }
                } else {
                    println!(
                        "  {fqdn:<24} {:<10} owner {}",
                        rec.status.as_str(),
                        rec.owner_public_key
                    );
                }
            }
            if verify_mode {
                if failed > 0 {
                    die(&format!(
                        "[!!] {failed} domain record(s) under .{tld} failed verification"
                    ));
                }
                println!(
                    "\n[ok] full chain valid: Federate Root Key -> .{tld} delegation -> operator registry -> domain records"
                );
            }
        }
        Err(e) => die(&format!("\n[!!] cannot load the .{tld} registry: {e}")),
    }
}

// ---------------------------------------------------------------------------
// operator tooling (runs entirely on the operator's machine)
// ---------------------------------------------------------------------------

fn operator_cmd(cmd: OperatorCmd) {
    match cmd {
        OperatorCmd::SignRecord {
            domain,
            owner,
            manifest_hash,
            key_dir,
            expires,
            out,
        } => {
            let parsed = federate_naming::FederateDomain::parse(&domain)
                .unwrap_or_else(|e| die(&format!("invalid domain: {e}")));
            if owner.len() != 64 || !owner.bytes().all(|b| b.is_ascii_hexdigit()) {
                die("--owner must be a 64-char hex public key");
            }
            if !federate_storage::is_valid_hash(&manifest_hash) {
                die("--manifest-hash must be a 64-char BLAKE3 hex content address");
            }
            let operator = NodeIdentity::load_or_create(&key_dir)
                .unwrap_or_else(|e| die(&format!("cannot load operator key: {e}")));
            let now = chrono::Utc::now().to_rfc3339();
            let mut record = federate_naming::DomainRecord {
                domain: parsed.fqdn(),
                tld: parsed.tld.clone(),
                label: parsed.name.clone(),
                owner_public_key: owner,
                target_type: federate_naming::TargetType::Manifest,
                manifest_hash,
                service_id: None,
                node_id: None,
                status: federate_naming::DomainStatus::Active,
                created_at: now.clone(),
                updated_at: now,
                expires_at: expires,
                renewal: None,
                pricing: None,
                signature_algorithm: federate_root::SIGNATURE_ALGORITHM.into(),
                signature: None,
            };
            record.signature = Some(
                operator.sign(
                    &record
                        .signable_bytes()
                        .unwrap_or_else(|e| die(&format!("cannot canonicalize record: {e}"))),
                ),
            );
            let path = out.unwrap_or_else(|| format!("{}.record.json", parsed.fqdn()).into());
            std::fs::write(&path, serde_json::to_vec_pretty(&record).unwrap())
                .unwrap_or_else(|e| die(&format!("cannot write {}: {e}", path.display())));
            println!("[ok] signed domain record for {}", parsed.fqdn());
            println!("     operator key: {}", operator.node_id());
            println!("     written to  : {}", path.display());
            println!(
                "next: put it with your other records and run\n      federate operator build-registry {} --records <dir>",
                parsed.tld
            );
        }
        OperatorCmd::BuildRegistry {
            tld,
            records,
            key_dir,
            version,
            out,
        } => {
            let tld = federate_naming::validate_tld_name(&tld)
                .unwrap_or_else(|e| die(&format!("invalid TLD: {e}")));
            let operator = NodeIdentity::load_or_create(&key_dir)
                .unwrap_or_else(|e| die(&format!("cannot load operator key: {e}")));
            let mut domains = std::collections::BTreeMap::new();
            let entries = std::fs::read_dir(&records)
                .unwrap_or_else(|e| die(&format!("cannot read {}: {e}", records.display())));
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|x| x.to_str()) != Some("json")
                    || path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(".registry.json"))
                {
                    continue;
                }
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|e| die(&format!("cannot read {}: {e}", path.display())));
                let record: federate_naming::DomainRecord = match serde_json::from_slice(&bytes) {
                    Ok(r) => r,
                    Err(_) => continue, // not a domain record; skip quietly
                };
                if record.tld != tld {
                    die(&format!(
                        "{} is a record for .{}, not .{tld}; a registry never mixes TLDs",
                        path.display(),
                        record.tld
                    ));
                }
                if let Err(e) = record.verify(&operator.node_id()) {
                    die(&format!(
                        "{} does not verify against YOUR operator key ({e}); re-sign it with `federate operator sign-record`",
                        path.display()
                    ));
                }
                println!("  + {} ({})", record.domain, record.status.as_str());
                domains.insert(record.domain.clone(), record);
            }
            if domains.is_empty() {
                die(&format!(
                    "no valid .{tld} domain records found in {}",
                    records.display()
                ));
            }
            let version = version.unwrap_or_else(|| chrono::Utc::now().timestamp().max(0) as u64);
            let registry = federate_registry::TldRegistry::signed(
                &operator,
                &tld,
                version,
                &chrono::Utc::now().to_rfc3339(),
                domains,
            )
            .unwrap_or_else(|e| die(&format!("cannot sign registry: {e}")));
            let bytes = serde_json::to_vec(&registry).unwrap();
            let registry_hash = federate_storage::hash_bytes(&bytes);
            let path = out.unwrap_or_else(|| format!("{tld}.registry.json").into());
            std::fs::write(&path, &bytes)
                .unwrap_or_else(|e| die(&format!("cannot write {}: {e}", path.display())));
            println!(
                "\n[ok] signed .{tld} registry v{version} ({} domains)",
                registry.domains.len()
            );
            println!("     operator key : {}", operator.node_id());
            println!("     written to   : {}", path.display());
            println!("     registry hash: {registry_hash}");
            println!("\nto publish it:");
            println!(
                "  delegated_native : serve the file from your node:  [node] registry_files = [\"{}\"]",
                path.display()
            );
            println!(
                "  delegated_manifest: give the registry hash above to the Federate Root to pin in the .{tld} TLD record"
            );
            println!(
                "  remember: version must INCREASE on every change (clients reject rollbacks)"
            );
        }
        OperatorCmd::VerifyRegistry {
            file,
            tld,
            operator,
        } => {
            let tld = federate_naming::validate_tld_name(&tld)
                .unwrap_or_else(|e| die(&format!("invalid TLD: {e}")));
            let (bytes, registry) = federate_registry::load_registry_file(&file)
                .unwrap_or_else(|e| die(&format!("[!!] {e}")));
            let self_check = operator.is_none();
            let operator_key = operator.unwrap_or_else(|| registry.operator_public_key.clone());
            registry
                .verify(&tld, &operator_key)
                .unwrap_or_else(|e| die(&format!("[!!] registry verification FAILED: {e}")));
            let mut failed = 0usize;
            for (fqdn, rec) in &registry.domains {
                if let Err(e) = rec.verify(&operator_key) {
                    failed += 1;
                    println!("  [!!] {fqdn}: {e}");
                } else {
                    println!("  [ok] {fqdn} ({})", rec.status.as_str());
                }
            }
            if failed > 0 {
                die(&format!("[!!] {failed} record(s) failed verification"));
            }
            println!(
                "\n[ok] .{tld} registry v{} verifies ({} domains, hash {})",
                registry.version,
                registry.domains.len(),
                federate_storage::hash_bytes(&bytes)
            );
            if self_check {
                println!(
                    "note: verified against the key claimed INSIDE the file (self-consistency). \
                     For a real verification pass --operator with the key from the root-signed TLD record."
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// site owner tooling
// ---------------------------------------------------------------------------

fn site_cmd(cmd: SiteCmd) {
    let SiteCmd::Package {
        dir,
        domain,
        key_dir,
        out,
        version,
        install,
    } = cmd;
    let parsed = federate_naming::FederateDomain::parse(&domain)
        .unwrap_or_else(|e| die(&format!("invalid domain: {e}")));
    let owner = NodeIdentity::load_or_create(&key_dir)
        .unwrap_or_else(|e| die(&format!("cannot load owner key: {e}")));

    // Content-address every file, exactly like Node 1 does for sites/.
    let mut files = std::collections::BTreeMap::new();
    let mut blocks: Vec<(String, Vec<u8>)> = Vec::new();
    for file in walkdir::WalkDir::new(&dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel = file
            .path()
            .strip_prefix(&dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(file.path())
            .unwrap_or_else(|e| die(&format!("cannot read {}: {e}", file.path().display())));
        let hash = federate_storage::hash_bytes(&bytes);
        files.insert(rel, hash.clone());
        blocks.push((hash, bytes));
    }
    if !files.contains_key("index.html") {
        die(&format!("{} has no index.html", dir.display()));
    }

    let mut manifest = Manifest {
        domain: parsed.fqdn(),
        version,
        entry: "index.html".into(),
        files,
        owner_public_key: owner.node_id(),
        created_at: chrono::Utc::now().to_rfc3339(),
        signature_algorithm: federate_root::SIGNATURE_ALGORITHM.into(),
        signature: None,
    };
    manifest.signature = Some(
        owner.sign(
            &manifest
                .signable_bytes()
                .unwrap_or_else(|e| die(&format!("cannot canonicalize manifest: {e}"))),
        ),
    );
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let manifest_hash = federate_storage::hash_bytes(&manifest_bytes);

    // Write the package: blocks under their content address + the manifest
    // under its own (exact bytes; re-serializing would change the hash).
    let out_dir = out.unwrap_or_else(|| {
        let mut p = dir.clone().into_os_string();
        p.push(".federate-package");
        p.into()
    });
    let blocks_dir = out_dir.join("blocks");
    std::fs::create_dir_all(&blocks_dir)
        .unwrap_or_else(|e| die(&format!("cannot create {}: {e}", blocks_dir.display())));
    for (hash, bytes) in &blocks {
        std::fs::write(blocks_dir.join(hash), bytes)
            .unwrap_or_else(|e| die(&format!("cannot write block {hash}: {e}")));
    }
    std::fs::write(out_dir.join(&manifest_hash), &manifest_bytes)
        .unwrap_or_else(|e| die(&format!("cannot write manifest: {e}")));

    println!(
        "[ok] packaged {} ({} files, {} blocks)",
        parsed.fqdn(),
        manifest.files.len(),
        blocks.len()
    );
    println!("     owner key    : {}", owner.node_id());
    println!("     package dir  : {}", out_dir.display());
    println!("     manifest hash: {manifest_hash}");

    // Optional install into a node's stores: blocks into the cdn block
    // store, manifest bytes into the resolver's content-addressed manifest
    // cache. A local federate-noded (storage/cdn role) then serves both.
    if let Some(data_dir) = install {
        let store = federate_storage::BlockStore::new(&data_dir.join("cdn"))
            .unwrap_or_else(|e| die(&format!("cannot open node block store: {e}")));
        for (hash, bytes) in &blocks {
            store
                .put(hash, bytes)
                .unwrap_or_else(|e| die(&format!("cannot install block {hash}: {e}")));
        }
        let manifest_dir = data_dir.join("manifests");
        std::fs::create_dir_all(&manifest_dir)
            .unwrap_or_else(|e| die(&format!("cannot create {}: {e}", manifest_dir.display())));
        std::fs::write(manifest_dir.join(&manifest_hash), &manifest_bytes)
            .unwrap_or_else(|e| die(&format!("cannot install manifest: {e}")));
        println!(
            "     installed to : {} (blocks + manifest)",
            data_dir.display()
        );
    }

    println!("\nnext (give these to your TLD operator):");
    println!("  domain       : {}", parsed.fqdn());
    println!("  owner key    : {}", owner.node_id());
    println!("  manifest hash: {manifest_hash}");
    println!(
        "the operator runs: federate operator sign-record {} --owner {} --manifest-hash {manifest_hash}",
        parsed.fqdn(),
        owner.node_id()
    );
}

// ---------------------------------------------------------------------------
// signed mutations / publishing / persistent registry
// ---------------------------------------------------------------------------

async fn node_post(
    bootstrap: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<(u16, serde_json::Value), String> {
    let url = format!("{}{path}", bootstrap.trim_end_matches('/'));
    let resp = http()
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let v = resp.json().await.map_err(|e| e.to_string())?;
    Ok((status, v))
}

/// Challenge-response step 1: get a single-use nonce from Node 1.
async fn fetch_mutation_nonce(cli: &Ctx) -> String {
    match node_post(
        &cli.bootstrap,
        "/v1/mutations/nonce",
        &serde_json::json!({}),
    )
    .await
    {
        Ok((200, v)) => match v["nonce"].as_str() {
            Some(nonce) if !nonce.is_empty() => nonce.to_string(),
            _ => die("node answered the nonce request without a nonce"),
        },
        Ok((code, v)) => die(&format!("nonce request failed ({code}): {v}")),
        Err(e) => die(&format!("cannot reach {} ({e})", cli.bootstrap)),
    }
}

/// Ask the node which version advances this mutation's target.
async fn next_target_version(cli: &Ctx, action: &federate_mutation::MutationAction) -> u64 {
    let (kind, id) = action.target();
    match node_get(
        &cli.bootstrap,
        &format!("/v1/mutations/target/{}/{id}", kind.as_str()),
    )
    .await
    {
        Ok(v) => v["next_version"]
            .as_u64()
            .unwrap_or_else(|| die("node answered without a next_version")),
        Err(e) => die(&format!("cannot query target version ({e})")),
    }
}

fn report_mutation_outcome(status: u16, v: &serde_json::Value) {
    if v["accepted"].as_bool() == Some(true) {
        println!("[ok] mutation accepted");
        println!(
            "     mutation id : {}",
            v["mutation_id"].as_str().unwrap_or("?")
        );
        println!(
            "     root zone   : v{}",
            v["root_version"].as_u64().unwrap_or(0)
        );
        println!(
            "     audit event : {}",
            v["audit_event"]["event_id"].as_str().unwrap_or("?")
        );
    } else {
        die(&format!(
            "[!!] mutation rejected ({status}): {}",
            v["error"].as_str().unwrap_or("unknown error")
        ));
    }
}

/// Sign and submit one mutation: fetch nonce + target version, sign the
/// envelope with the key in `key_dir`, POST it, report the outcome.
async fn sign_and_submit_mutation(
    cli: &Ctx,
    key_dir: &std::path::Path,
    action: federate_mutation::MutationAction,
) {
    let identity = NodeIdentity::load_or_create(key_dir)
        .unwrap_or_else(|e| die(&format!("cannot load key from {} ({e})", key_dir.display())));
    let nonce = fetch_mutation_nonce(cli).await;
    let version = next_target_version(cli, &action).await;
    let req = federate_mutation::MutationRequest::signed(
        &identity,
        &nonce,
        &chrono::Utc::now().to_rfc3339(),
        version,
        action,
    )
    .unwrap_or_else(|e| die(&format!("cannot sign mutation: {e}")));
    match node_post(
        &cli.bootstrap,
        "/v1/mutations",
        &serde_json::to_value(&req).unwrap(),
    )
    .await
    {
        Ok((status, v)) => report_mutation_outcome(status, &v),
        Err(e) => die(&format!("cannot submit mutation ({e})")),
    }
}

/// Content-address every file of a site dir (shared by `site package` and
/// `publish package`).
fn collect_site_blocks(
    dir: &std::path::Path,
) -> (
    std::collections::BTreeMap<String, String>,
    Vec<federate_mutation::ContentBlock>,
) {
    let mut files = std::collections::BTreeMap::new();
    let mut blocks: Vec<(String, Vec<u8>)> = Vec::new();
    for file in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel = file
            .path()
            .strip_prefix(dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(file.path())
            .unwrap_or_else(|e| die(&format!("cannot read {}: {e}", file.path().display())));
        let hash = federate_storage::hash_bytes(&bytes);
        files.insert(rel, hash.clone());
        blocks.push((hash, bytes));
    }
    if !files.contains_key("index.html") {
        die(&format!("{} has no index.html", dir.display()));
    }
    (files, blocks)
}

/// Build the signed publish mutation + package and submit it to the node's
/// ingest endpoint.
async fn submit_site_package(
    cli: &Ctx,
    owner: &NodeIdentity,
    domain: &str,
    manifest_bytes: Vec<u8>,
    manifest_hash: String,
    blocks: Vec<(String, Vec<u8>)>,
) {
    let action = federate_mutation::MutationAction::PublishSite {
        domain: domain.to_string(),
        manifest_hash: manifest_hash.clone(),
    };
    let nonce = fetch_mutation_nonce(cli).await;
    let version = next_target_version(cli, &action).await;
    let mutation = federate_mutation::MutationRequest::signed(
        owner,
        &nonce,
        &chrono::Utc::now().to_rfc3339(),
        version,
        action,
    )
    .unwrap_or_else(|e| die(&format!("cannot sign publish mutation: {e}")));
    let package = federate_mutation::SitePackage {
        mutation,
        manifest_hex: hex::encode(&manifest_bytes),
        blocks: blocks
            .iter()
            .map(|(hash, bytes)| federate_mutation::PackageBlock {
                hash: hash.clone(),
                data_hex: hex::encode(bytes),
            })
            .collect(),
    };
    println!(
        "submitting {domain} to {} ({} blocks, manifest {manifest_hash})",
        cli.bootstrap,
        blocks.len()
    );
    match node_post(
        &cli.bootstrap,
        "/v1/ingest/package",
        &serde_json::to_value(&package).unwrap(),
    )
    .await
    {
        Ok((status, v)) => {
            report_mutation_outcome(status, &v);
            println!("\nverify it end to end:");
            println!(
                "  federate fetch fed://{domain}/ --bootstrap {}",
                cli.bootstrap
            );
        }
        Err(e) => die(&format!("cannot submit package ({e})")),
    }
}

async fn publish_cmd(cli: &Ctx, cmd: PublishCmd) {
    let PublishCmd::Package {
        dir,
        domain,
        key_dir,
        version,
    } = cmd;
    let parsed = federate_naming::FederateDomain::parse(&domain)
        .unwrap_or_else(|e| die(&format!("invalid domain: {e}")));
    let owner = NodeIdentity::load_or_create(&key_dir)
        .unwrap_or_else(|e| die(&format!("cannot load owner key: {e}")));
    let (files, blocks) = collect_site_blocks(&dir);
    let mut manifest = Manifest {
        domain: parsed.fqdn(),
        version,
        entry: "index.html".into(),
        files,
        owner_public_key: owner.node_id(),
        created_at: chrono::Utc::now().to_rfc3339(),
        signature_algorithm: federate_root::SIGNATURE_ALGORITHM.into(),
        signature: None,
    };
    manifest.signature = Some(
        owner.sign(
            &manifest
                .signable_bytes()
                .unwrap_or_else(|e| die(&format!("cannot canonicalize manifest: {e}"))),
        ),
    );
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let manifest_hash = federate_storage::hash_bytes(&manifest_bytes);
    submit_site_package(
        cli,
        &owner,
        &parsed.fqdn(),
        manifest_bytes,
        manifest_hash,
        blocks,
    )
    .await;
}

async fn registry_cmd(cli: &Ctx, cmd: RegistryCmd) {
    match cmd {
        RegistryCmd::SubmitPackage { package, key_dir } => {
            let owner = NodeIdentity::load_or_create(&key_dir)
                .unwrap_or_else(|e| die(&format!("cannot load owner key: {e}")));

            // The manifest is the file at the package root named by its own
            // content address (blocks live under blocks/).
            let mut manifest_entry: Option<(String, Vec<u8>, Manifest)> = None;
            for entry in std::fs::read_dir(&package)
                .unwrap_or_else(|e| die(&format!("cannot read {}: {e}", package.display())))
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
            {
                let name = entry.file_name().to_string_lossy().to_string();
                if !federate_storage::is_valid_hash(&name) {
                    continue;
                }
                let bytes = std::fs::read(entry.path())
                    .unwrap_or_else(|e| die(&format!("cannot read manifest: {e}")));
                if federate_storage::hash_bytes(&bytes) != name {
                    die(&format!("{name} does not match its own content address"));
                }
                let manifest: Manifest = serde_json::from_slice(&bytes)
                    .unwrap_or_else(|e| die(&format!("{name} is not a manifest: {e}")));
                manifest_entry = Some((name, bytes, manifest));
            }
            let Some((manifest_hash, manifest_bytes, manifest)) = manifest_entry else {
                die(&format!(
                    "{} has no manifest file (expected the output of `federate site package`)",
                    package.display()
                ));
            };
            if manifest.owner_public_key != owner.node_id() {
                die(&format!(
                    "the manifest is owned by {} but {} holds {}; sign with the manifest owner's key",
                    manifest.owner_public_key,
                    key_dir.display(),
                    owner.node_id()
                ));
            }

            let blocks_dir = package.join("blocks");
            let mut blocks: Vec<(String, Vec<u8>)> = Vec::new();
            for entry in std::fs::read_dir(&blocks_dir)
                .unwrap_or_else(|e| die(&format!("cannot read {}: {e}", blocks_dir.display())))
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
            {
                let name = entry.file_name().to_string_lossy().to_string();
                let bytes = std::fs::read(entry.path())
                    .unwrap_or_else(|e| die(&format!("cannot read block {name}: {e}")));
                if let Err(e) = federate_storage::verify(&bytes, &name) {
                    die(&format!("block {name} is corrupted: {e}"));
                }
                blocks.push((name, bytes));
            }
            let domain = manifest.domain.clone();
            submit_site_package(cli, &owner, &domain, manifest_bytes, manifest_hash, blocks).await;
        }
        RegistryCmd::Status => match node_get(&cli.bootstrap, "/v1/registry/status").await {
            Ok(v) => pretty(&v),
            Err(e) => die(&format!("cannot fetch registry status ({e})")),
        },
        RegistryCmd::Audit { limit } => {
            match node_get(&cli.bootstrap, &format!("/v1/registry/audit?limit={limit}")).await {
                Ok(v) => {
                    println!(
                        "audit log: {} event(s) total, showing up to {limit}",
                        v["total"].as_u64().unwrap_or(0)
                    );
                    for e in v["events"].as_array().unwrap_or(&vec![]) {
                        println!(
                            "{}  {:<26} {:<8} {:<14} by {}…  mutation {}…",
                            e["timestamp"].as_str().unwrap_or("?"),
                            e["action"].as_str().unwrap_or("?"),
                            e["target_type"].as_str().unwrap_or("?"),
                            e["target_id"].as_str().unwrap_or("?"),
                            &e["actor_public_key"].as_str().unwrap_or("?")
                                [..12.min(e["actor_public_key"].as_str().unwrap_or("?").len())],
                            &e["mutation_id"].as_str().unwrap_or("?")
                                [..12.min(e["mutation_id"].as_str().unwrap_or("?").len())],
                        );
                    }
                }
                Err(e) => die(&format!("cannot fetch audit log ({e})")),
            }
        }
        RegistryCmd::Snapshot => {
            match node_post(
                &cli.bootstrap,
                "/v1/registry/snapshot",
                &serde_json::json!({}),
            )
            .await
            {
                Ok((200, v)) => {
                    println!(
                        "[ok] snapshot written: {} (root zone v{})",
                        v["snapshot"].as_str().unwrap_or("?"),
                        v["root_version"].as_u64().unwrap_or(0)
                    );
                }
                Ok((code, v)) => die(&format!("snapshot failed ({code}): {v}")),
                Err(e) => die(&format!("cannot request snapshot ({e})")),
            }
        }
        RegistryCmd::MigrateJsonToRedb { data_dir } => {
            eprintln!(
                "note: run this with federate-server STOPPED (the database is single-writer)."
            );
            let root = NodeIdentity::load_or_create(&data_dir.join("root"))
                .unwrap_or_else(|e| die(&format!("cannot load root key: {e}")));
            match federate_mutation::migrate_json_to_redb(
                &data_dir.join("registry"),
                &root.node_id(),
            ) {
                Ok(report) => {
                    println!("[ok] migrated JSON registry to redb");
                    pretty(&report);
                }
                Err(e) => die(&format!("[!!] migration refused: {e}")),
            }
        }
        RegistryCmd::Backup { output, data_dir } => {
            match federate_mutation::backup_registry(&data_dir.join("registry"), &output) {
                Ok(report) => {
                    println!(
                        "[ok] registry database backed up to {} ({} bytes)",
                        report["backup"].as_str().unwrap_or("?"),
                        report["bytes"].as_u64().unwrap_or(0)
                    );
                    println!("remember: manifests/, blocks/, and private keys need their own backup (docs/en-US/backups.md)");
                }
                Err(e) => die(&format!("[!!] backup failed: {e}")),
            }
        }
        RegistryCmd::Restore {
            input,
            data_dir,
            force,
        } => {
            let root = NodeIdentity::load_or_create(&data_dir.join("root"))
                .unwrap_or_else(|e| die(&format!("cannot load root key: {e}")));
            match federate_mutation::restore_registry(
                &input,
                &data_dir.join("registry"),
                &root.node_id(),
                force,
            ) {
                Ok(report) => {
                    println!("[ok] registry database restored and verified");
                    pretty(&report);
                }
                Err(e) => die(&format!("[!!] restore failed: {e}")),
            }
        }
        RegistryCmd::Db { cmd } => {
            let (data_dir, verify) = match cmd {
                RegistryDbCmd::Stats { data_dir } => (data_dir, false),
                RegistryDbCmd::Verify { data_dir } => (data_dir, true),
            };
            let root = NodeIdentity::load_or_create(&data_dir.join("root"))
                .unwrap_or_else(|e| die(&format!("cannot load root key: {e}")));
            let store =
                federate_mutation::RegistryStore::open(&data_dir.join("registry"), &root.node_id())
                    .unwrap_or_else(|e| die(&format!("[!!] cannot open registry: {e}")));
            if verify {
                match store.verify_all(&root.node_id()) {
                    Ok(report) => {
                        println!("[ok] database verification passed");
                        pretty(&report);
                    }
                    Err(e) => die(&format!("[!!] database verification FAILED: {e}")),
                }
            } else {
                pretty(
                    &store
                        .db_stats()
                        .unwrap_or_else(|e| die(&format!("stats failed: {e}"))),
                );
            }
        }
        RegistryCmd::Verify => match node_get(&cli.bootstrap, "/v1/registry/verify").await {
            Ok(v) if v["verified"].as_bool() == Some(true) => {
                println!("[ok] persistent registry self-verification passed");
                pretty(&v);
            }
            Ok(v) => die(&format!(
                "[!!] registry verification FAILED: {}",
                v["error"].as_str().unwrap_or("unknown error")
            )),
            Err(e) => die(&format!("cannot verify registry ({e})")),
        },
    }
}

async fn mutation_cmd(cli: &Ctx, cmd: MutationCmd) {
    match cmd {
        MutationCmd::Nonce => {
            match node_post(
                &cli.bootstrap,
                "/v1/mutations/nonce",
                &serde_json::json!({}),
            )
            .await
            {
                Ok((200, v)) => pretty(&v),
                Ok((code, v)) => die(&format!("nonce request failed ({code}): {v}")),
                Err(e) => die(&format!("cannot reach {} ({e})", cli.bootstrap)),
            }
        }
        MutationCmd::Inspect { id } => {
            match node_get(&cli.bootstrap, &format!("/v1/mutations/{id}")).await {
                Ok(v) => pretty(&v),
                Err(_) => die(&format!("mutation {id} not found on this node")),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// fed:// URL scheme handler registration
// ---------------------------------------------------------------------------

/// Register/unregister the fed:// scheme so links open in the browser via
/// the HTTP compatibility door. Per-user, zero admin rights, zero signing:
/// - macOS: a locally generated AppleScript applet (no quarantine because
///   it is created on this machine, not downloaded) rewrites fed:// to
///   http:// and hands it to the default browser;
/// - Linux: a .desktop entry with x-scheme-handler/fed pointing at
///   `federate open %u`;
/// - Windows: HKCU registry keys pointing at `federate.exe open "%1"`.
fn handler_cmd(cmd: HandlerCmd) {
    match cmd {
        HandlerCmd::Install => handler_install(),
        HandlerCmd::Uninstall => handler_uninstall(),
        HandlerCmd::Status => handler_status(),
    }
}

#[cfg(target_os = "macos")]
fn handler_app_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| die("cannot locate home directory"))
        .join("Applications/Federate URL Handler.app")
}

#[cfg(target_os = "macos")]
const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister";

#[cfg(target_os = "macos")]
fn handler_install() {
    let app = handler_app_path();
    if let Some(parent) = app.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|e| die(&format!("cannot create {}: {e}", parent.display())));
    }
    let _ = std::fs::remove_dir_all(&app);

    // The whole handler: rewrite fed://x -> http://x and let the default
    // browser take it. Generated and compiled locally by osacompile (ships
    // with macOS), so Gatekeeper never quarantines it.
    let script = r#"on open location theURL
	if theURL starts with "fed://" then
		open location "http://" & text 7 thru -1 of theURL
	end if
end open location"#;
    let src = std::env::temp_dir().join("federate-url-handler.applescript");
    std::fs::write(&src, script).unwrap_or_else(|e| die(&format!("cannot write script: {e}")));
    let run = |bin: &str, args: &[&str]| {
        let out = std::process::Command::new(bin)
            .args(args)
            .output()
            .unwrap_or_else(|e| die(&format!("cannot run {bin}: {e}")));
        if !out.status.success() {
            die(&format!(
                "{bin} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    };
    run(
        "osacompile",
        &["-o", &app.to_string_lossy(), &src.to_string_lossy()],
    );
    let plist = app.join("Contents/Info.plist");
    let plist = plist.to_string_lossy().to_string();
    run(
        "plutil",
        &[
            "-replace",
            "CFBundleIdentifier",
            "-string",
            "network.federate.url-handler",
            &plist,
        ],
    );
    run(
        "plutil",
        &[
            "-insert",
            "CFBundleURLTypes",
            "-xml",
            "<array><dict><key>CFBundleURLName</key><string>Federate URL</string>\
             <key>CFBundleURLSchemes</key><array><string>fed</string></array></dict></array>",
            &plist,
        ],
    );
    run(
        "plutil",
        &["-replace", "LSUIElement", "-bool", "YES", &plist],
    );
    run(LSREGISTER, &["-f", &app.to_string_lossy()]);
    let _ = std::fs::remove_file(&src);
    println!("[ok] fed:// links now open in your browser");
    println!("     handler: {}", app.display());
    println!("try it: open fed://home.fed");
}

#[cfg(target_os = "macos")]
fn handler_uninstall() {
    let app = handler_app_path();
    if app.exists() {
        let _ = std::process::Command::new(LSREGISTER)
            .args(["-u", &app.to_string_lossy()])
            .output();
        std::fs::remove_dir_all(&app)
            .unwrap_or_else(|e| die(&format!("cannot remove {}: {e}", app.display())));
        println!("[ok] fed:// handler removed");
    } else {
        println!("fed:// handler is not installed");
    }
}

#[cfg(target_os = "macos")]
fn handler_status() {
    let app = handler_app_path();
    if app.join("Contents/Info.plist").exists() {
        println!("[ok] fed:// handler installed at {}", app.display());
    } else {
        println!("fed:// handler NOT installed; run: federate handler install");
    }
}

#[cfg(target_os = "linux")]
fn handler_desktop_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| die("cannot locate home directory"))
        .join(".local/share/applications/federate-url-handler.desktop")
}

#[cfg(target_os = "linux")]
fn handler_install() {
    let exe = std::env::current_exe()
        .unwrap_or_else(|e| die(&format!("cannot locate the federate binary: {e}")));
    let desktop = handler_desktop_path();
    if let Some(parent) = desktop.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|e| die(&format!("cannot create {}: {e}", parent.display())));
    }
    std::fs::write(
        &desktop,
        format!(
            "[Desktop Entry]\nType=Application\nName=Federate URL Handler\n\
             Exec={} open -- %u\nMimeType=x-scheme-handler/fed;\nNoDisplay=true\nTerminal=false\n",
            exe.display()
        ),
    )
    .unwrap_or_else(|e| die(&format!("cannot write {}: {e}", desktop.display())));
    let ok = std::process::Command::new("xdg-mime")
        .args([
            "default",
            "federate-url-handler.desktop",
            "x-scheme-handler/fed",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::process::Command::new("update-desktop-database")
        .arg(desktop.parent().unwrap())
        .status();
    if ok {
        println!("[ok] fed:// links now open in your browser");
        println!("     handler: {}", desktop.display());
        println!("try it: xdg-open fed://home.fed");
    } else {
        die("xdg-mime is required (package xdg-utils); the .desktop file was written but not registered");
    }
}

#[cfg(target_os = "linux")]
fn handler_uninstall() {
    let desktop = handler_desktop_path();
    if desktop.exists() {
        std::fs::remove_file(&desktop)
            .unwrap_or_else(|e| die(&format!("cannot remove {}: {e}", desktop.display())));
        let _ = std::process::Command::new("update-desktop-database")
            .arg(desktop.parent().unwrap())
            .status();
        println!("[ok] fed:// handler removed");
    } else {
        println!("fed:// handler is not installed");
    }
}

#[cfg(target_os = "linux")]
fn handler_status() {
    let registered = std::process::Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/fed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("federate-url-handler"))
        .unwrap_or(false);
    if registered {
        println!(
            "[ok] fed:// handler installed ({})",
            handler_desktop_path().display()
        );
    } else {
        println!("fed:// handler NOT installed; run: federate handler install");
    }
}

#[cfg(target_os = "windows")]
fn handler_install() {
    let exe = std::env::current_exe()
        .unwrap_or_else(|e| die(&format!("cannot locate federate.exe: {e}")));
    // `--` stops flag parsing: a crafted fed:// URL substituted into %1
    // is always taken as the positional domain, never smuggled argv.
    let command = format!("\"{}\" open -- \"%1\"", exe.display());
    let run = |args: &[&str]| {
        let ok = std::process::Command::new("reg")
            .args(args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            die(&format!("registry write failed: reg {}", args.join(" ")));
        }
    };
    run(&[
        "add",
        r"HKCU\Software\Classes\fed",
        "/ve",
        "/d",
        "URL:Federate Protocol",
        "/f",
    ]);
    run(&[
        "add",
        r"HKCU\Software\Classes\fed",
        "/v",
        "URL Protocol",
        "/d",
        "",
        "/f",
    ]);
    run(&[
        "add",
        r"HKCU\Software\Classes\fed\shell\open\command",
        "/ve",
        "/d",
        &command,
        "/f",
    ]);
    println!("[ok] fed:// links now open in your browser");
    println!("try it: start fed://home.fed");
}

#[cfg(target_os = "windows")]
fn handler_uninstall() {
    let ok = std::process::Command::new("reg")
        .args(["delete", r"HKCU\Software\Classes\fed", "/f"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    println!(
        "{}",
        if ok {
            "[ok] fed:// handler removed"
        } else {
            "fed:// handler is not installed"
        }
    );
}

#[cfg(target_os = "windows")]
fn handler_status() {
    let installed = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Classes\fed\shell\open\command"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if installed {
        println!("[ok] fed:// handler installed (HKCU registry)");
    } else {
        println!("fed:// handler NOT installed; run: federate handler install");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn handler_install() {
    die("fed:// handler registration is not supported on this OS yet");
}
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn handler_uninstall() {
    die("fed:// handler registration is not supported on this OS yet");
}
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn handler_status() {
    die("fed:// handler registration is not supported on this OS yet");
}

// ---------------------------------------------------------------------------
// manifest / keys
// ---------------------------------------------------------------------------

async fn manifest_verify(cli: &Ctx, domain: &str) {
    // Chain-verified record lookup (root-managed and delegated TLDs alike).
    let resolver = build_resolver(cli, None, vec![]).await;
    let rec = resolver
        .resolve_domain(domain)
        .await
        .unwrap_or_else(|e| die(&format!("[!!] {domain}: {e}")));
    let url = format!(
        "{}/v1/manifest/{}",
        cli.bootstrap.trim_end_matches('/'),
        rec.manifest_hash
    );
    let bytes = http()
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .unwrap_or_else(|e| die(&format!("cannot fetch manifest ({e})")))
        .bytes()
        .await
        .unwrap_or_else(|e| die(&format!("cannot read manifest ({e})")));
    if let Err(e) = federate_storage::verify(&bytes, &rec.manifest_hash) {
        die(&format!("[!!] manifest hash mismatch: {e}"));
    }
    let manifest: Manifest =
        serde_json::from_slice(&bytes).unwrap_or_else(|e| die(&format!("bad manifest JSON: {e}")));
    match manifest.verify(domain, &rec.owner_public_key) {
        Ok(()) => println!(
            "[ok] {domain} manifest: hash matches domain record, signature valid, signed by domain owner ({} files)",
            manifest.files.len()
        ),
        Err(e) => die(&format!("[!!] manifest verification FAILED: {e}")),
    }
}

fn key_cmd(cmd: KeyCmd) {
    match cmd {
        KeyCmd::Generate { dir } => match NodeIdentity::load_or_create(&dir) {
            Ok(id) => {
                println!("public key : {}", id.node_id());
                println!(
                    "private key: {} (keep this file secret; never share or upload it)",
                    id.key_path().display()
                );
            }
            Err(e) => die(&format!("key generation failed: {e}")),
        },
        KeyCmd::Inspect => {
            let data_dir = DaemonConfig::default_data_dir();
            match NodeIdentity::load_or_create(&data_dir) {
                Ok(id) => {
                    println!("algorithm  : ed25519");
                    println!("public key : {}", id.node_id());
                    println!(
                        "private key: {} (local file, never transmitted)",
                        id.key_path().display()
                    );
                }
                Err(e) => die(&format!("cannot inspect key: {e}")),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// nodes / dns / gateway / directory
// ---------------------------------------------------------------------------

async fn node_cmd(cli: &Ctx, cmd: NodeCmd) {
    match cmd {
        NodeCmd::Register { config } => {
            let cfg = federate_node::NodeConfig::load(&config)
                .unwrap_or_else(|e| die(&format!("cannot load {}: {e}", config.display())));
            let rt = federate_node::NodeRuntime::new(cfg)
                .unwrap_or_else(|e| die(&format!("node runtime error: {e}")));
            let reg = rt
                .build_registration()
                .unwrap_or_else(|e| die(&format!("cannot build registration: {e}")));
            let dir = federate_directory::DirectoryClient::new(rt.config.directory_url());
            match dir.register(&reg).await {
                Ok(()) => {
                    println!(
                        "[ok] registered node {} with {}",
                        rt.node_id(),
                        dir.base_url()
                    );
                    println!(
                        "     roles: {}",
                        reg.roles
                            .iter()
                            .map(|r| r.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                Err(e) => die(&format!("[!!] registration failed: {e}")),
            }
        }
        NodeCmd::Status { node } => match node_get(&node, "/status").await {
            Ok(v) => pretty(&v),
            Err(e) => die(&format!("node not reachable at {node} ({e})")),
        },
        NodeCmd::Ping { addr } => native_ping(&addr).await,
        NodeCmd::Roles { node } => match node_get(&node, "/roles").await {
            Ok(v) => pretty(&v),
            Err(e) => die(&format!("node not reachable at {node} ({e})")),
        },
        NodeCmd::Health { node } => {
            let url = format!("{}/health", node.trim_end_matches('/'));
            match http().get(&url).send().await {
                Ok(r) if r.status().is_success() => println!("[ok] {node} is healthy"),
                Ok(r) => die(&format!("[!!] {node} answered {}", r.status())),
                Err(e) => die(&format!("[!!] {node} unreachable ({e})")),
            }
        }
        NodeCmd::List { role } => directory_list(cli, role, false).await,
        NodeCmd::Run { roles, config } => {
            let roles_arg = roles
                .iter()
                .map(|r| r.as_str())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "spawning: federate-noded --config {} --roles {roles_arg}",
                config.display()
            );
            let status = std::process::Command::new("federate-noded")
                .arg("--config")
                .arg(&config)
                .arg("--roles")
                .arg(&roles_arg)
                .status();
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => die(&format!("federate-noded exited with {s}")),
                Err(e) => die(&format!(
                    "cannot spawn federate-noded ({e}); build/install it first: cargo install --path crates/federate-noded"
                )),
            }
        }
    }
}

/// Handshake + GetStatus over the native Federate protocol. This command
/// never touches HTTP: it is the smallest possible native Federate client.
async fn native_ping(addr: &str) {
    let data_dir = DaemonConfig::default_data_dir();
    std::fs::create_dir_all(&data_dir).ok();
    let identity = NodeIdentity::load_or_create(&data_dir)
        .unwrap_or_else(|e| die(&format!("cannot load identity: {e}")));
    let agent = concat!("federate-cli/", env!("CARGO_PKG_VERSION"));
    let started = std::time::Instant::now();
    let (mut conn, welcome) = federate_transport::Connection::connect(addr, &identity, agent)
        .await
        .unwrap_or_else(|e| die(&format!("[!!] native handshake with {addr} failed: {e}")));
    let rtt = started.elapsed().as_millis();
    let federate_protocol::Message::Welcome {
        version,
        node_id,
        agent: peer_agent,
        capabilities,
    } = welcome
    else {
        die("[!!] peer did not answer the handshake with Welcome");
    };
    println!("[ok] native handshake with {addr} in {rtt}ms");
    println!("     protocol version: {version}");
    println!("     node id: {node_id}");
    println!("     agent: {peer_agent}");
    println!(
        "     capabilities: {}",
        capabilities
            .iter()
            .map(|c| format!("{c:?}").to_lowercase())
            .collect::<Vec<_>>()
            .join(", ")
    );
    match conn.request(&federate_protocol::Message::GetStatus).await {
        Ok(federate_protocol::Message::Status {
            roles,
            region,
            root_version,
            ..
        }) => {
            println!("     roles: {}", roles.join(", "));
            if !region.is_empty() {
                println!("     region: {region}");
            }
            match root_version {
                Some(v) => println!("     root zone: v{v} (verified locally by the node)"),
                None => println!("     root zone: none loaded yet"),
            }
        }
        Ok(federate_protocol::Message::Error { code, detail }) => {
            println!("     status: not answered ({code:?}: {detail})")
        }
        Ok(other) => println!("     status: unexpected answer {other:?}"),
        Err(e) => println!("     status: request failed ({e})"),
    }
}

/// Show every node the directory knows as a provider for a block, with the
/// transport(s) each one speaks.
async fn providers_cmd(cli: &Ctx, hash: &str) {
    if !federate_storage::is_valid_hash(hash) {
        die("not a valid content address (expected 64 lowercase hex chars)");
    }
    let dir = federate_directory::DirectoryClient::new(&cli.bootstrap);
    match dir.providers(hash, None).await {
        Ok(nodes) if nodes.is_empty() => {
            println!("no providers announced for block {hash}");
            println!("(origin/Node 1 can still serve it over HTTP compatibility)");
        }
        Ok(nodes) => {
            for n in nodes {
                let transport = match n.native_addr() {
                    Some(addr) => format!("native {addr} + http"),
                    None => "http only".to_string(),
                };
                println!(
                    "{:<16} {:<9} transport: {:<28} roles: {:<22} ips: {} last_seen: {}",
                    &n.registration.node_id[..16.min(n.registration.node_id.len())],
                    n.status.as_str(),
                    transport,
                    n.registration
                        .roles
                        .iter()
                        .map(|r| r.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                    n.registration.public_ips.join(","),
                    n.last_seen,
                );
            }
        }
        Err(e) => die(&format!(
            "cannot query providers from {} ({e})",
            cli.bootstrap
        )),
    }
}

async fn dns_test(domain: &str, server: &str) {
    use federate_dns::wire;
    let addr: std::net::SocketAddr = server
        .parse()
        .unwrap_or_else(|_| die(&format!("invalid DNS server address {server}")));
    let query = wire::build_query(rand::random(), domain, 1);
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .unwrap_or_else(|e| die(&format!("socket error: {e}")));
    socket
        .send_to(&query, addr)
        .await
        .unwrap_or_else(|e| die(&format!("cannot reach DNS node {server}: {e}")));
    let mut buf = [0u8; 4096];
    let (len, _) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        socket.recv_from(&mut buf),
    )
    .await
    .unwrap_or_else(|_| die(&format!("DNS node {server} did not answer within 5s")))
    .unwrap_or_else(|e| die(&format!("receive error: {e}")));
    match wire::parse_answers(&buf[..len]) {
        Ok((0, answers)) if !answers.is_empty() => {
            println!("[ok] {domain} answered by {server}:");
            for (ip, ttl) in answers {
                println!("  {domain} A/AAAA {ip} (ttl {ttl}s)");
            }
        }
        Ok((0, _)) => {
            println!("[ok] {server} answered with no A/AAAA records (forwarded or empty)")
        }
        Ok((rcode, _)) => die(&format!(
            "[!!] DNS node answered rcode {rcode} (2=SERVFAIL, 3=NXDOMAIN)"
        )),
        Err(e) => die(&format!("[!!] cannot parse DNS response: {}", e.0)),
    }
}

async fn gateway_test(domain: &str, gateway: &str) {
    let url = format!("{}/", gateway.trim_end_matches('/'));
    match http().get(&url).header("Host", domain).send().await {
        Ok(r) => {
            let status = r.status();
            let via = r
                .headers()
                .get("x-federate-gateway")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("-")
                .to_string();
            let body = r.bytes().await.map(|b| b.len()).unwrap_or(0);
            if status.is_success() {
                println!("[ok] {domain} via {gateway}: {status}, {body} bytes (x-federate-gateway: {via})");
            } else {
                die(&format!("[!!] {domain} via {gateway}: {status}"));
            }
        }
        Err(e) => die(&format!("[!!] gateway {gateway} unreachable ({e})")),
    }
}

async fn directory_list(cli: &Ctx, role: Option<federate_directory::NodeRole>, healthy: bool) {
    let dir = federate_directory::DirectoryClient::new(&cli.bootstrap);
    match dir.list(role, healthy).await {
        Ok(nodes) if nodes.is_empty() => println!(
            "no nodes registered{}",
            role.map(|r| format!(" with role {}", r.as_str()))
                .unwrap_or_default()
        ),
        Ok(nodes) => {
            for n in nodes {
                println!(
                    "{:<16} {:<9} {:<8} roles: {:<28} ips: {} (latency: {})",
                    &n.registration.node_id[..16.min(n.registration.node_id.len())],
                    n.status.as_str(),
                    n.registration.region,
                    n.registration
                        .roles
                        .iter()
                        .map(|r| r.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                    n.registration.public_ips.join(","),
                    n.latency_ms
                        .map(|l| format!("{l}ms"))
                        .unwrap_or_else(|| "-".into()),
                );
            }
        }
        Err(e) => die(&format!(
            "cannot list nodes from directory {} ({e})",
            cli.bootstrap
        )),
    }
}

// ---------------------------------------------------------------------------
// doctor / port-check (unchanged behavior + root trust check)
// ---------------------------------------------------------------------------

fn port_check() {
    match std::net::TcpListener::bind("127.0.0.1:80") {
        Ok(_) => println!("ok: port 80 can be bound; federated will work portless"),
        Err(e) => {
            println!("cannot bind 127.0.0.1:80: {e}");
            println!("if federated is already running, this is expected.");
            println!("otherwise see docs/en-US/port-80-setup.md:");
            println!(
                "  linux:   sudo setcap 'cap_net_bind_service=+ep' ./target/release/federated"
            );
            println!("  macos:   run with sudo or install the launchd service");
            println!("  windows: run terminal as Administrator");
        }
    }
}

async fn doctor(cli: &Ctx) {
    println!("federate doctor\n");
    let mut problems = 0;

    // 1. daemon
    let daemon = api_get(&cli.api, "/status").await;
    match &daemon {
        Ok(v) => {
            println!("[ok] daemon running at {}", cli.api);
            if v["node1_reachable"].as_bool() == Some(true) {
                println!("[ok] Node 1 reachable ({})", v["bootstrap"]);
            } else {
                problems += 1;
                println!(
                    "[!!] Node 1 not reachable ({}); cached sites still work",
                    v["bootstrap"]
                );
            }
            match v["root_version"].as_u64() {
                Some(ver) => println!(
                    "[ok] root zone verified and cached (v{ver}, {} domains)",
                    v["domains"].as_array().map(|a| a.len()).unwrap_or(0)
                ),
                None => {
                    problems += 1;
                    println!("[!!] no verified root zone; the daemon never reached Node 1 or verification failed");
                }
            }
            match v["trusted_root_key"].as_str() {
                Some(k) => println!("[ok] trusted root key pinned: {k}"),
                None => {
                    problems += 1;
                    println!("[!!] no trusted root key pinned yet");
                }
            }
            println!("[ok] cached blocks: {}", v["cached_blocks"]);
        }
        Err(e) => {
            problems += 1;
            println!("[!!] daemon not reachable at {} ({e})", cli.api);
            println!("     fix: run `federated`");
        }
    }

    // 2. port 80 / gateway
    match std::net::TcpListener::bind("127.0.0.1:80") {
        Ok(_) => {
            if daemon.is_ok() {
                problems += 1;
                println!(
                    "[!!] port 80 is free but the daemon is up; the gateway is NOT on port 80"
                );
                println!("     portless URLs like http://home.fed will not work; see docs/en-US/port-80-setup.md");
            } else {
                println!("[ok] port 80 available for federated");
            }
        }
        Err(_) => {
            if daemon.is_ok() {
                println!("[ok] port 80 in use (presumably by federated's gateway)");
            } else {
                problems += 1;
                println!("[!!] port 80 in use by something else");
            }
        }
    }

    // 3. hosts file
    #[cfg(windows)]
    let hosts_path = r"C:\Windows\System32\drivers\etc\hosts";
    #[cfg(not(windows))]
    let hosts_path = "/etc/hosts";
    match std::fs::read_to_string(hosts_path) {
        Ok(contents) => {
            if contents.contains("home.fed") {
                println!("[ok] hosts file maps home.fed");
            } else {
                problems += 1;
                println!("[!!] hosts file has no Federate entries");
                println!("     fix: add mappings from docs/en-US/hosts-setup.md to {hosts_path}");
            }
        }
        Err(e) => {
            problems += 1;
            println!("[!!] cannot read {hosts_path}: {e}");
        }
    }

    // 4. end-to-end resolution + gateway
    if daemon.is_ok() {
        match api_get(&cli.api, "/resolve?domain=home.fed&path=/").await {
            Ok(v) if v["status"] == "ok" => {
                println!("[ok] home.fed resolves with full signature chain verification")
            }
            Ok(v) => {
                problems += 1;
                println!("[!!] home.fed did not resolve: {v}");
            }
            Err(e) => {
                problems += 1;
                println!("[!!] resolve check failed: {e}");
            }
        }
        match http()
            .get("http://127.0.0.1:80/")
            .header("Host", "home.fed")
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => println!("[ok] gateway serves home.fed on port 80"),
            Ok(r) => {
                problems += 1;
                println!("[!!] gateway answered {} for home.fed", r.status());
            }
            Err(e) => {
                problems += 1;
                println!("[!!] gateway not answering on 127.0.0.1:80 ({e})");
            }
        }
    }

    println!();
    if problems == 0 {
        println!("all checks passed. Open http://home.fed");
    } else {
        println!("{problems} problem(s) found. See docs/en-US/troubleshooting.md");
        std::process::exit(1);
    }
}
