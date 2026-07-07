//! Explicit registry initialization and external seed files.
//!
//! The network's TLD set is DATABASE state, never code constants. A brand
//! new node gets its registry in two explicit steps:
//!
//! ```text
//! federate root init                        # empty signed registry
//! federate root seed --file seeds/official-tlds.toml
//! ```
//!
//! The seed file is plain TOML data; editing it changes nothing until the
//! seed command is run again, and the command refuses to touch an
//! already-populated registry unless forced (and even then it only ADDS
//! missing TLDs through the normal audited mutation path, never overwrites).

use crate::request::{MutationAction, MutationRequest};
use crate::store::{MutationContext, RegistryStore};
use federate_core::{FederateError, Result};
use federate_identity::NodeIdentity;
use federate_root::{AuditEvent, RootZone, SIGNATURE_ALGORITHM};
use serde::Deserialize;
use std::path::Path;

/// One TLD entry of a seed file.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedTld {
    pub name: String,
    /// "official" | "reserved" | "blocked"
    pub mode: String,
    /// Human purpose for official TLDs (stored in the record's notes).
    #[serde(default)]
    pub purpose: Option<String>,
    /// Reason for reserved/blocked entries.
    #[serde(default)]
    pub reason: Option<String>,
}

/// External TOML seed file: `[[tlds]]` entries.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedFile {
    #[serde(default)]
    pub tlds: Vec<SeedTld>,
}

impl SeedFile {
    pub fn parse(content: &str) -> Result<Self> {
        let seed: SeedFile = toml::from_str(content)
            .map_err(|e| FederateError::MutationRejected(format!("invalid seed file: {e}")))?;
        if seed.tlds.is_empty() {
            return Err(FederateError::MutationRejected(
                "seed file defines no TLDs".into(),
            ));
        }
        Ok(seed)
    }

    pub fn load(path: &Path) -> Result<Self> {
        Self::parse(&std::fs::read_to_string(path)?)
    }
}

/// What a seed run did.
#[derive(Debug, Default)]
pub struct SeedOutcome {
    pub created: Vec<String>,
    pub skipped_existing: Vec<String>,
}

/// Explicit first initialization: an EMPTY signed registry (zero TLDs).
/// Refuses when registry state already exists.
pub fn init_empty_registry(dir: &Path, root: &NodeIdentity) -> Result<RegistryStore> {
    if RegistryStore::exists(dir) {
        return Err(FederateError::MutationRejected(format!(
            "registry already initialized at {}",
            dir.display()
        )));
    }
    let now = chrono::Utc::now();
    let mut zone = RootZone {
        network: federate_core::NETWORK_NAME.into(),
        root_version: now.timestamp().max(0) as u64,
        generated_at: now.to_rfc3339(),
        root_public_key: root.node_id(),
        tlds: std::collections::BTreeMap::new(),
        domains: std::collections::BTreeMap::new(),
        audit: vec![AuditEvent {
            at: now.to_rfc3339(),
            actor: "root".into(),
            action: "root.init".into(),
            subject: "root-zone".into(),
            detail: Some("empty registry initialized; TLDs arrive via seed/mutations".into()),
        }],
        signature_algorithm: SIGNATURE_ALGORITHM.into(),
        signature: None,
    };
    zone.signature = Some(root.sign(&zone.signable_bytes()?));
    zone.verify(&root.node_id())?;
    RegistryStore::init(
        dir,
        zone,
        std::collections::BTreeMap::new(),
        std::collections::BTreeMap::new(),
        Vec::new(),
    )
}

/// Apply a seed file through the normal audited mutation path.
///
/// Default: refuses when the registry already holds ANY TLD records (a
/// populated registry is never silently re-seeded). With `force`, existing
/// TLDs are left untouched and only missing entries are created; every
/// creation is a versioned, signed, audited mutation.
pub fn apply_seed(
    store: &mut RegistryStore,
    seed: &SeedFile,
    ctx: &MutationContext,
    force: bool,
) -> Result<SeedOutcome> {
    if !store.zone().tlds.is_empty() && !force {
        return Err(FederateError::MutationRejected(format!(
            "registry already holds {} TLD record(s); refusing to re-seed (use --force to add missing entries only)",
            store.zone().tlds.len()
        )));
    }
    let mut outcome = SeedOutcome::default();
    for entry in &seed.tlds {
        let name = federate_naming::validate_tld_name(&entry.name)?;
        if store.zone().tlds.contains_key(&name) {
            outcome.skipped_existing.push(name);
            continue;
        }
        let action = match entry.mode.as_str() {
            "official" => MutationAction::CreateTld {
                tld: name.clone(),
                purpose: entry
                    .purpose
                    .clone()
                    .unwrap_or_else(|| "official Federate TLD".into()),
            },
            "reserved" => MutationAction::ReserveTld {
                tld: name.clone(),
                reason: entry
                    .reason
                    .clone()
                    .or_else(|| entry.purpose.clone())
                    .unwrap_or_else(|| "reserved by seed file".into()),
            },
            "blocked" => MutationAction::BlockTld {
                tld: name.clone(),
                reason: entry
                    .reason
                    .clone()
                    .or_else(|| entry.purpose.clone())
                    .unwrap_or_else(|| "blocked by seed file".into()),
            },
            other => {
                return Err(FederateError::MutationRejected(format!(
                    "seed entry .{name}: unknown mode '{other}' (official | reserved | blocked)"
                )))
            }
        };
        let version = store.target_version(&action.target_key()) + 1;
        let req = MutationRequest::signed(
            ctx.root,
            &format!("seed:{name}"),
            &ctx.now.to_rfc3339(),
            version,
            action,
        )?;
        store.apply(&req, ctx)?;
        outcome.created.push(name);
    }
    Ok(outcome)
}
