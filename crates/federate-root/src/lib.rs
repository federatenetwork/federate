//! federate-root — the Federate Root Registry layer.
//!
//! Root zone types, TLD records, blocklists, signing/verification, cache.
//!
//! Hierarchy (do not collapse):
//!   Federate Root Registry → TLD Operator → Domain Registrant → Site/Manifest Owner
//!
//! Chain of trust:
//!   Federate Root Key → TLD Record → Domain Record → Site Manifest → Content Blocks

use federate_core::{FederateError, Result};
use federate_naming::{validate_tld_name, DomainRecord, RegistryType, TldMode, TldStatus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

pub const SIGNATURE_ALGORITHM: &str = "ed25519";

// ---------------------------------------------------------------------------
// TLD record
// ---------------------------------------------------------------------------

/// A TLD record in the Federate Root Registry. Signed by the Federate Root
/// Key (signature covers canonical JSON with `signature: null`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TldRecord {
    pub tld: String,
    pub status: TldStatus,
    pub mode: TldMode,
    /// Economic/legal owner of the TLD.
    pub owner_public_key: String,
    /// Key authorized to operate the TLD registry and sign domain records.
    pub operator_public_key: String,
    pub operator_name: String,
    pub registry_type: RegistryType,
    /// For delegated_http registries: base URL of the operator registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_endpoint: Option<String>,
    /// For delegated_manifest registries: hash of the signed registry manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_manifest_hash: Option<String>,
    /// Hash of the policy document governing this TLD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_hash: Option<String>,
    /// Pricing metadata placeholder (future marketplace — no payments yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub signature_algorithm: String,
    /// Ed25519 signature (hex) by the Federate Root Key.
    #[serde(default)]
    pub signature: Option<String>,
}

impl TldRecord {
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    /// Verify this TLD record is signed by the Federate Root Key.
    pub fn verify(&self, root_public_key: &str) -> Result<()> {
        let fail = |reason: &str| {
            Err(FederateError::VerificationFailed {
                layer: "tld".into(),
                subject: format!(".{}", self.tld),
                reason: reason.to_string(),
            })
        };
        let Some(sig) = &self.signature else {
            return fail("TLD record is unsigned");
        };
        if !federate_identity::verify_signature(root_public_key, &self.signable_bytes()?, sig) {
            return fail("TLD record signature is not from the Federate Root Key");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Audit events
// ---------------------------------------------------------------------------

/// Governance/audit trail entry (creation, delegation, block, revocation, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub at: String,
    pub actor: String,
    pub action: String,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------
// Root zone
// ---------------------------------------------------------------------------

/// The Federate root zone: the authoritative signed map of the namespace.
/// Signed by the Federate Root Key. Node 1 *distributes* this data; clients
/// trust the signatures, not the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootZone {
    pub network: String,
    pub root_version: u64,
    pub generated_at: String,
    /// The Federate Root public key (hex). Daemons pin/verify against a
    /// configured trust anchor — this field is informational + for TOFU.
    pub root_public_key: String,
    /// tld name -> signed TLD record
    pub tlds: BTreeMap<String, TldRecord>,
    /// fqdn -> signed domain record (root-managed registries only; delegated
    /// TLD domains live in the operator's registry — future phases).
    pub domains: BTreeMap<String, DomainRecord>,
    /// Audit trail of root governance actions.
    #[serde(default)]
    pub audit: Vec<AuditEvent>,
    pub signature_algorithm: String,
    /// Ed25519 signature (hex) by the Federate Root Key over canonical bytes.
    #[serde(default)]
    pub signature: Option<String>,
}

impl RootZone {
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    /// Verify zone signature against a trusted root public key, then verify
    /// every TLD record. Domain records are verified lazily at resolution
    /// time against their TLD's operator key.
    pub fn verify(&self, trusted_root_key: &str) -> Result<()> {
        let fail = |reason: String| {
            Err(FederateError::VerificationFailed {
                layer: "root".into(),
                subject: self.network.clone(),
                reason,
            })
        };
        if self.root_public_key != trusted_root_key {
            return fail(
                "root zone advertises a different root key than the trusted anchor".into(),
            );
        }
        let Some(sig) = &self.signature else {
            return fail("root zone is unsigned".into());
        };
        if !federate_identity::verify_signature(trusted_root_key, &self.signable_bytes()?, sig) {
            return fail("root zone signature invalid (tampered or wrong key)".into());
        }
        for record in self.tlds.values() {
            record.verify(trusted_root_key)?;
        }
        Ok(())
    }

    /// Basic structural validation (signature checks live in `verify`).
    pub fn validate(&self) -> Result<()> {
        if self.network.is_empty() {
            return Err(FederateError::InvalidRoot("empty network name".into()));
        }
        if self.tlds.is_empty() {
            return Err(FederateError::InvalidRoot("no TLDs defined".into()));
        }
        for (fqdn, rec) in &self.domains {
            if !self.tlds.contains_key(&rec.tld) {
                return Err(FederateError::InvalidRoot(format!(
                    "domain {fqdn} references unknown TLD .{}",
                    rec.tld
                )));
            }
        }
        Ok(())
    }

    pub fn lookup_tld(&self, tld: &str) -> Option<&TldRecord> {
        self.tlds.get(tld)
    }

    pub fn lookup(&self, fqdn: &str) -> Option<&DomainRecord> {
        self.domains.get(fqdn)
    }
}

// ---------------------------------------------------------------------------
// Blocklists
// ---------------------------------------------------------------------------

/// Which list rejected a TLD name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// Public IANA/ICANN TLD (from blocked_tlds.txt).
    PublicIana,
    /// Federate reserved name (data/blocked/reserved-tlds.txt).
    Reserved,
    /// Policy blocklist (data/blocked/policy-tlds.txt).
    Policy,
    /// Brand/safety blocklist (data/blocked/brand-safety-tlds.txt).
    BrandSafety,
}

impl BlockReason {
    pub fn describe(self) -> &'static str {
        match self {
            BlockReason::PublicIana => {
                "it is a public IANA/ICANN TLD (blocked_tlds.txt) — Federate never collides with the normal internet"
            }
            BlockReason::Reserved => {
                "it is reserved for Federate infrastructure, governance, safety, or future use"
            }
            BlockReason::Policy => "it is blocked by Federate policy",
            BlockReason::BrandSafety => "it is blocked for brand/safety reasons",
        }
    }
}

/// Loaded blocklists. Files are one name per line; `#` comments and blank
/// lines ignored; names normalized to lowercase.
#[derive(Debug, Default)]
pub struct Blocklists {
    pub iana: HashSet<String>,
    pub reserved: HashSet<String>,
    pub policy: HashSet<String>,
    pub brand_safety: HashSet<String>,
}

fn load_list(path: &Path) -> Result<HashSet<String>> {
    let content = std::fs::read_to_string(path)?;
    Ok(content
        .lines()
        .map(|l| l.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect())
}

impl Blocklists {
    /// Load from `blocked_tlds_path` (authoritative IANA list — must exist)
    /// and `blocked_dir` (reserved/policy/brand-safety — created with
    /// defaults when missing).
    pub fn load(blocked_tlds_path: &Path, blocked_dir: &Path) -> Result<Self> {
        let iana = load_list(blocked_tlds_path).map_err(|e| {
            FederateError::InvalidRoot(format!(
                "cannot load IANA blocklist {}: {e}",
                blocked_tlds_path.display()
            ))
        })?;
        std::fs::create_dir_all(blocked_dir)?;
        let ensure = |name: &str, default: &str| -> Result<HashSet<String>> {
            let path = blocked_dir.join(name);
            if !path.exists() {
                std::fs::write(&path, default)?;
            }
            load_list(&path)
        };
        Ok(Self {
            iana,
            reserved: ensure(
                "reserved-tlds.txt",
                "# Federate reserved TLD names (infrastructure, governance, safety, future use)\nfed\nroot\nadmin\nregistry\nstatus\nnodes\nprotocol\nsystem\n",
            )?,
            policy: ensure(
                "policy-tlds.txt",
                "# Federate policy blocklist (placeholder for future governance)\n",
            )?,
            brand_safety: ensure(
                "brand-safety-tlds.txt",
                "# Federate brand/safety blocklist (placeholder for future governance)\n",
            )?,
        })
    }

    /// Why (if at all) a TLD name is blocked from creation.
    pub fn check(&self, tld: &str) -> Option<BlockReason> {
        if self.iana.contains(tld) {
            Some(BlockReason::PublicIana)
        } else if self.reserved.contains(tld) {
            Some(BlockReason::Reserved)
        } else if self.policy.contains(tld) {
            Some(BlockReason::Policy)
        } else if self.brand_safety.contains(tld) {
            Some(BlockReason::BrandSafety)
        } else {
            None
        }
    }

    /// Full validation for *creating/applying for* a new TLD: naming rules
    /// then every blocklist. `allow_reserved` lets the root itself register
    /// official TLDs whose names sit on the reserved list (e.g. `.fed`).
    pub fn validate_new_tld(&self, input: &str, allow_reserved: bool) -> Result<String> {
        let tld = validate_tld_name(input)?;
        match self.check(&tld) {
            None => Ok(tld),
            Some(BlockReason::Reserved) if allow_reserved => Ok(tld),
            Some(BlockReason::Reserved) => Err(FederateError::ReservedTld {
                tld,
                reason: BlockReason::Reserved.describe().into(),
            }),
            Some(reason) => Err(FederateError::BlockedTld {
                tld,
                reason: reason.describe().into(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Disk cache
// ---------------------------------------------------------------------------

/// Disk cache for the root zone so cached sites keep working when Node 1 is
/// temporarily unavailable.
pub struct RootCache {
    path: PathBuf,
}

impl RootCache {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("root-zone.json"),
        }
    }

    pub fn load(&self) -> Result<RootZone> {
        let bytes = std::fs::read(&self.path)?;
        let zone: RootZone = serde_json::from_slice(&bytes)?;
        zone.validate()?;
        Ok(zone)
    }

    pub fn store(&self, zone: &RootZone) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serde_json::to_vec_pretty(zone)?)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use federate_identity::NodeIdentity;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fed-root-test-{name}-{}", std::process::id()))
    }

    fn blocklists() -> Blocklists {
        let dir = tmp("bl");
        std::fs::create_dir_all(&dir).unwrap();
        let iana = dir.join("blocked_tlds.txt");
        std::fs::write(&iana, "COM\nNET\nORG\nBR\nDEV\nAPP\nLIVE\nPAGE\n").unwrap();
        Blocklists::load(&iana, &dir.join("blocked")).unwrap()
    }

    fn make_tld(root: &NodeIdentity, tld: &str, status: TldStatus, mode: TldMode) -> TldRecord {
        let mut rec = TldRecord {
            tld: tld.into(),
            status,
            mode,
            owner_public_key: root.node_id(),
            operator_public_key: root.node_id(),
            operator_name: "test".into(),
            registry_type: RegistryType::RootManaged,
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
        rec.signature = Some(root.sign(&rec.signable_bytes().unwrap()));
        rec
    }

    #[test]
    fn iana_blocklist_rejects_public_tlds() {
        let bl = blocklists();
        for t in ["com", "net", "org", "br", "dev", "app", "live", "page"] {
            assert!(matches!(
                bl.validate_new_tld(t, false),
                Err(FederateError::BlockedTld { .. })
            ));
        }
        assert!(bl.validate_new_tld("femboy", false).is_ok());
    }

    #[test]
    fn reserved_and_policy_lists_reject() {
        let bl = blocklists();
        assert!(matches!(
            bl.validate_new_tld("root", false),
            Err(FederateError::ReservedTld { .. })
        ));
        // root itself may register reserved names as official TLDs
        assert!(bl.validate_new_tld("fed", true).is_ok());
        // policy list is loaded from file
        let dir = tmp("bl2");
        std::fs::create_dir_all(dir.join("blocked")).unwrap();
        std::fs::write(dir.join("iana.txt"), "com\n").unwrap();
        std::fs::write(dir.join("blocked/policy-tlds.txt"), "scamcoin\n").unwrap();
        let bl2 = Blocklists::load(&dir.join("iana.txt"), &dir.join("blocked")).unwrap();
        assert!(matches!(
            bl2.validate_new_tld("scamcoin", false),
            Err(FederateError::BlockedTld { .. })
        ));
    }

    #[test]
    fn root_zone_sign_verify_and_tamper() {
        let root = NodeIdentity::load_or_create(&tmp("rootkey")).unwrap();
        let mut tlds = BTreeMap::new();
        tlds.insert(
            "fed".to_string(),
            make_tld(&root, "fed", TldStatus::Official, TldMode::Official),
        );
        let mut zone = RootZone {
            network: "federate".into(),
            root_version: 1,
            generated_at: "t".into(),
            root_public_key: root.node_id(),
            tlds,
            domains: BTreeMap::new(),
            audit: vec![],
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(root.sign(&zone.signable_bytes().unwrap()));
        assert!(zone.verify(&root.node_id()).is_ok());

        // tampered zone fails
        let mut bad = zone.clone();
        bad.root_version = 999;
        assert!(bad.verify(&root.node_id()).is_err());

        // wrong trust anchor fails
        assert!(zone.verify(&"22".repeat(32)).is_err());
    }

    #[test]
    fn tld_record_wrong_key_fails() {
        let root = NodeIdentity::load_or_create(&tmp("rootkey2")).unwrap();
        let attacker = NodeIdentity::load_or_create(&tmp("attacker")).unwrap();
        let mut rec = make_tld(&root, "femboy", TldStatus::Delegated, TldMode::Delegated);
        assert!(rec.verify(&root.node_id()).is_ok());
        // re-sign with attacker key → must fail against root anchor
        rec.signature = Some(attacker.sign(&rec.signable_bytes().unwrap()));
        assert!(rec.verify(&root.node_id()).is_err());
    }
}
