//! federate: CLI for the Federate Network.
//!
//! Registry/verification commands talk to Node 1 (bootstrap URL) and verify
//! signatures locally; the server is a distributor of signed data, not a
//! trusted authority. Daemon commands talk to the local daemon API.

use clap::{Parser, Subcommand};
use federate_core::{DaemonConfig, DEFAULT_API_ADDR, DEFAULT_BOOTSTRAP_URL};
use federate_identity::NodeIdentity;
use federate_manifest::Manifest;
use federate_naming::DomainRecord;
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
    /// Resolve a Federate domain (e.g. federate resolve home.fed)
    Resolve { domain: String },
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
    /// Block a TLD (admin/seed-data-only in this phase)
    Block {
        tld: String,
        #[arg(long)]
        reason: String,
    },
    /// Reserve a TLD (admin/seed-data-only in this phase)
    Reserve {
        tld: String,
        #[arg(long)]
        reason: String,
    },
    /// Verify a TLD record's signature against the root key
    Verify { tld: String },
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

async fn api_get(api: &str, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("http://{api}{path}");
    let resp = reqwest::get(&url).await.map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

async fn api_delete(api: &str, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("http://{api}{path}");
    let resp = reqwest::Client::new()
        .delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

async fn node_get(bootstrap: &str, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}{path}", bootstrap.trim_end_matches('/'));
    let resp = reqwest::get(&url).await.map_err(|e| e.to_string())?;
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
            match api_get(&cli.api, &format!("/resolve?domain={domain}&path=/")).await {
                Ok(v) => pretty(&v),
                Err(e) => die(&format!(
                    "daemon not reachable ({e}); is federated running?"
                )),
            }
        }
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
        Cmd::Open { domain } => {
            // Portless URL: this is the whole point.
            let url = format!("http://{domain}");
            println!("opening {url}");
            #[cfg(target_os = "macos")]
            let cmd = ("open", vec![url.clone()]);
            #[cfg(target_os = "linux")]
            let cmd = ("xdg-open", vec![url.clone()]);
            #[cfg(target_os = "windows")]
            let cmd = ("cmd", vec!["/C".to_string(), format!("start {url}")]);
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
                         additionally pins the root key as a trust anchor (see docs/signatures.md)."
                    );
                    if bad > 0 {
                        std::process::exit(1);
                    }
                }
                Err(e) => die(&format!("[!!] root zone verification FAILED: {e}")),
            }
        }
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
                    println!("See docs/tld-marketplace-roadmap.md; no payments are implemented.");
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
            println!("`federate tld approve` is admin/seed-data-only in this phase.");
            println!("Would approve .{tld} with owner={owner} operator={operator}.");
            println!("Runtime mutation APIs (with signed requests + nonce/challenge replay protection) arrive in marketplace phase 2.");
        }
        TldCmd::Block { tld, reason } => {
            println!("`federate tld block` is admin/seed-data-only in this phase.");
            println!("To block .{tld} today, add it to data/blocked/policy-tlds.txt (reason: {reason}) and restart federate-server.");
        }
        TldCmd::Reserve { tld, reason } => {
            println!("`federate tld reserve` is admin/seed-data-only in this phase.");
            println!("To reserve .{tld} today, add it to data/blocked/reserved-tlds.txt (reason: {reason}) and restart federate-server.");
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
            let zone = fetch_root(&cli.bootstrap)
                .await
                .unwrap_or_else(|e| die(&format!("cannot fetch root zone ({e})")));
            verify_domain_chain(&zone, &domain).map(|rec: DomainRecord| {
                println!("[ok] {domain}: root zone, TLD record, and domain record signatures all valid");
                println!("     owner: {}", rec.owner_public_key);
                println!("     manifest: {}", rec.manifest_hash);
            }).unwrap_or_else(|e| die(&format!("[!!] {e}")));
        }
    }
}

fn verify_domain_chain(zone: &RootZone, domain: &str) -> Result<DomainRecord, String> {
    zone.verify(&zone.root_public_key)
        .map_err(|e| format!("root zone verification failed: {e}"))?;
    let rec = zone
        .lookup(&domain.to_ascii_lowercase())
        .ok_or_else(|| format!("{domain} not found in the registry"))?;
    let tld = zone
        .tlds
        .get(&rec.tld)
        .ok_or_else(|| format!(".{} missing from root zone", rec.tld))?;
    tld.verify(&zone.root_public_key)
        .map_err(|e| e.to_string())?;
    rec.verify(&tld.operator_public_key)
        .map_err(|e| e.to_string())?;
    Ok(rec.clone())
}

// ---------------------------------------------------------------------------
// manifest / keys
// ---------------------------------------------------------------------------

async fn manifest_verify(cli: &Ctx, domain: &str) {
    let zone = fetch_root(&cli.bootstrap)
        .await
        .unwrap_or_else(|e| die(&format!("cannot fetch root zone ({e})")));
    let rec = verify_domain_chain(&zone, domain).unwrap_or_else(|e| die(&format!("[!!] {e}")));
    let url = format!(
        "{}/v1/manifest/{}",
        cli.bootstrap.trim_end_matches('/'),
        rec.manifest_hash
    );
    let bytes = reqwest::get(&url)
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
        NodeCmd::Roles { node } => match node_get(&node, "/roles").await {
            Ok(v) => pretty(&v),
            Err(e) => die(&format!("node not reachable at {node} ({e})")),
        },
        NodeCmd::Health { node } => {
            let url = format!("{}/health", node.trim_end_matches('/'));
            match reqwest::get(&url).await {
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
    match reqwest::Client::new()
        .get(&url)
        .header("Host", domain)
        .send()
        .await
    {
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
            println!("otherwise see docs/port-80-setup.md:");
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
                println!("     portless URLs like http://home.fed will not work; see docs/port-80-setup.md");
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
                println!("     fix: add mappings from docs/hosts-setup.md to {hosts_path}");
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
        match reqwest::Client::new()
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
        println!("{problems} problem(s) found. See docs/troubleshooting.md");
        std::process::exit(1);
    }
}
