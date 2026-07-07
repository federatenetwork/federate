//! federate-registry: registry model on top of the signed root zone.
//!
//! Hierarchy: Federate Root Registry → TLD (root-managed or delegated) →
//! domain records. The root zone is the single source of truth for which
//! TLDs exist and who operates them; this crate answers "where does the
//! domain record for X live, and is it valid?".
//!
//! Delegation model: the Federate Root signs the TLD record (who the
//! operator is, where the registry lives); the TLD operator signs the
//! registry and every domain record inside it; the domain owner signs the
//! site manifest. The root controls which TLDs exist, never every domain.

use federate_core::{FederateError, Result};
use federate_naming::{DomainRecord, FederateDomain, RegistryType};
use federate_root::{RootZone, TldRecord};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Size cap for a fetched TLD registry document (same order as the root
/// zone cap; operators are untrusted peers).
pub const MAX_REGISTRY_BYTES: u64 = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Signed TLD registry
// ---------------------------------------------------------------------------

/// The signed registry of a delegated TLD: every domain record the operator
/// has issued under it. Signed by the TLD operator key named in the
/// root-signed TLD record (signature covers canonical JSON with
/// `signature: null`), so a registry host can distribute it but never forge
/// or edit it.
///
/// Distribution modes (`RegistryType`):
/// - `delegated_manifest`: these bytes are content-addressed and pinned by
///   `registry_manifest_hash` in the TLD record
/// - `delegated_native` / `delegated_http`: served live by registry
///   providers; `version` is the rollback guard (clients reject a registry
///   older than one they already verified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TldRegistry {
    pub tld: String,
    /// Monotonic registry version, bumped by the operator on every change.
    pub version: u64,
    pub generated_at: String,
    /// The operator key this registry claims to be signed by. Must match the
    /// `operator_public_key` in the root-signed TLD record.
    pub operator_public_key: String,
    /// fqdn -> operator-signed domain record.
    pub domains: BTreeMap<String, DomainRecord>,
    pub signature_algorithm: String,
    /// Ed25519 signature (hex) by the TLD operator key over canonical bytes.
    #[serde(default)]
    pub signature: Option<String>,
}

impl TldRegistry {
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    /// Verify this registry against the TLD and operator key from the
    /// root-signed TLD record. Checks: TLD match, operator key match,
    /// operator signature, and that every entry belongs to this TLD (a
    /// registry for `.a` must never smuggle records for `.b`).
    pub fn verify(&self, expected_tld: &str, operator_public_key: &str) -> Result<()> {
        let fail = |reason: String| {
            Err(FederateError::VerificationFailed {
                layer: "tld-registry".into(),
                subject: format!(".{}", self.tld),
                reason,
            })
        };
        if self.tld != expected_tld {
            return fail(format!(
                "registry is for .{}, expected .{expected_tld}",
                self.tld
            ));
        }
        if self.operator_public_key != operator_public_key {
            return fail(
                "registry signer is not the operator key from the root-signed TLD record".into(),
            );
        }
        let Some(sig) = &self.signature else {
            return fail("registry is unsigned".into());
        };
        if !federate_identity::verify_signature(operator_public_key, &self.signable_bytes()?, sig) {
            return fail("registry signature invalid (tampered or wrong key)".into());
        }
        for (fqdn, rec) in &self.domains {
            if rec.tld != self.tld || rec.domain != *fqdn {
                return fail(format!(
                    "registry entry {fqdn} is inconsistent (record domain {}, tld .{})",
                    rec.domain, rec.tld
                ));
            }
        }
        Ok(())
    }

    /// Build and sign a registry from domain records (operator-side helper;
    /// used by Node 1 seed data, tests, and future operator tooling).
    pub fn signed(
        identity: &federate_identity::NodeIdentity,
        tld: &str,
        version: u64,
        generated_at: &str,
        domains: BTreeMap<String, DomainRecord>,
    ) -> Result<Self> {
        let mut registry = Self {
            tld: tld.to_string(),
            version,
            generated_at: generated_at.to_string(),
            operator_public_key: identity.node_id(),
            domains,
            signature_algorithm: federate_root::SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        registry.signature = Some(identity.sign(&registry.signable_bytes()?));
        Ok(registry)
    }

    pub fn lookup(&self, fqdn: &str) -> Option<&DomainRecord> {
        self.domains.get(fqdn)
    }
}

/// Load a signed registry document from disk, keeping the exact bytes (the
/// content address of a `delegated_manifest` registry is the hash of these
/// bytes, so they must be served verbatim). Used by operator tooling and by
/// registry provider nodes; signature verification stays the receiver's job.
pub fn load_registry_file(path: &std::path::Path) -> Result<(Vec<u8>, TldRegistry)> {
    let bytes = std::fs::read(path)?;
    let registry: TldRegistry = serde_json::from_slice(&bytes).map_err(|e| {
        FederateError::InvalidRoot(format!(
            "{} is not a valid TLD registry document: {e}",
            path.display()
        ))
    })?;
    Ok((bytes, registry))
}

/// Where a domain's record comes from.
#[derive(Debug)]
pub enum DomainSource {
    /// Record lives in the signed root zone itself.
    RootManaged(Box<DomainRecord>),
    /// TLD is delegated: the record lives in the operator's signed registry,
    /// reached through the distribution mode in the root-signed TLD record.
    Delegated {
        tld: String,
        operator_public_key: String,
        registry_type: RegistryType,
        registry_endpoint: Option<String>,
        registry_manifest_hash: Option<String>,
        registry_providers: Vec<String>,
    },
}

/// A read view over the root registry. Callers must pass an already
/// signature-verified `RootZone` (federate-resolution does this).
pub struct RegistryView<'a> {
    zone: &'a RootZone,
}

impl<'a> RegistryView<'a> {
    pub fn new(zone: &'a RootZone) -> Self {
        Self { zone }
    }

    pub fn tld(&self, tld: &str) -> Result<&'a TldRecord> {
        self.zone
            .lookup_tld(tld)
            .ok_or_else(|| FederateError::TldNotFound { tld: tld.into() })
    }

    /// Route a domain lookup through the TLD hierarchy: existence and status
    /// come from the root-signed TLD record; the domain record source depends
    /// on the registry type.
    pub fn locate_domain(&self, host: &str) -> Result<DomainSource> {
        let domain = FederateDomain::parse(host)?;
        let tld_rec = self.tld(&domain.tld)?;
        if !tld_rec.status.is_resolvable() {
            return Err(FederateError::TldUnavailable {
                tld: domain.tld.clone(),
                status: tld_rec.status.as_str().into(),
            });
        }
        if tld_rec.is_expired() {
            return Err(FederateError::TldUnavailable {
                tld: domain.tld.clone(),
                status: "expired".into(),
            });
        }
        match tld_rec.registry_type {
            RegistryType::RootManaged => {
                let rec = self
                    .zone
                    .lookup(&domain.fqdn())
                    .ok_or_else(|| FederateError::DomainNotFound(domain.fqdn()))?;
                rec.verify(&tld_rec.operator_public_key)?;
                if rec.is_expired() {
                    return Err(FederateError::TldUnavailable {
                        tld: domain.tld.clone(),
                        status: "expired".into(),
                    });
                }
                Ok(DomainSource::RootManaged(Box::new(rec.clone())))
            }
            _ => Ok(DomainSource::Delegated {
                tld: domain.tld.clone(),
                operator_public_key: tld_rec.operator_public_key.clone(),
                registry_type: tld_rec.registry_type,
                registry_endpoint: tld_rec.registry_endpoint.clone(),
                registry_manifest_hash: tld_rec.registry_manifest_hash.clone(),
                registry_providers: tld_rec.registry_providers.clone(),
            }),
        }
    }
}

/// HTTP compatibility client for a delegated TLD operator registry
/// (`delegated_http` mode). Fetches the whole signed registry document and
/// verifies the operator signature; the operator server is never trusted
/// blindly. The native protocol (`GetTldRegistry`) is the preferred path.
pub struct DelegatedRegistryClient {
    endpoint: String,
    operator_public_key: String,
}

impl DelegatedRegistryClient {
    pub fn new(endpoint: &str, operator_public_key: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            operator_public_key: operator_public_key.to_string(),
        }
    }

    /// Fetch and verify the signed registry for `tld`. Same timeout and
    /// download cap as every other cross-node fetch.
    pub async fn fetch_registry(&self, tld: &str) -> Result<TldRegistry> {
        let url = format!("{}/v1/tld-registry/{tld}", self.endpoint);
        let registry: TldRegistry = serde_json::from_value(federate_client::get_json(&url).await?)?;
        registry.verify(tld, &self.operator_public_key)?;
        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use federate_identity::NodeIdentity;
    use federate_naming::{
        DomainRecord, DomainStatus, RegistryType, TargetType, TldMode, TldStatus,
    };
    use federate_root::{RootZone, TldRecord, SIGNATURE_ALGORITHM};
    use std::collections::BTreeMap;

    fn tmp(n: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("fed-registry-{n}-{}", std::process::id()))
    }

    fn tld_rec(
        root: &NodeIdentity,
        operator: &NodeIdentity,
        tld: &str,
        status: TldStatus,
        rtype: RegistryType,
    ) -> TldRecord {
        let mut r = TldRecord {
            tld: tld.into(),
            status,
            mode: TldMode::Official,
            owner_public_key: root.node_id(),
            operator_public_key: operator.node_id(),
            operator_name: "op".into(),
            registry_type: rtype,
            registry_endpoint: Some("https://reg.example".into()),
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
        r.signature = Some(root.sign(&r.signable_bytes().unwrap()));
        r
    }

    // `name` must be unique per test: tests run on parallel threads, and a
    // shared identity dir means one test can read a key file while another
    // is still creating it.
    fn signed_zone(name: &str) -> (RootZone, NodeIdentity) {
        let root = NodeIdentity::load_or_create(&tmp(&format!("{name}-root"))).unwrap();
        let operator = NodeIdentity::load_or_create(&tmp(&format!("{name}-op"))).unwrap();
        let owner = NodeIdentity::load_or_create(&tmp(&format!("{name}-owner"))).unwrap();

        let mut tlds = BTreeMap::new();
        tlds.insert(
            "fed".to_string(),
            tld_rec(
                &root,
                &operator,
                "fed",
                TldStatus::Official,
                RegistryType::RootManaged,
            ),
        );
        tlds.insert(
            "shop".to_string(),
            tld_rec(
                &root,
                &operator,
                "shop",
                TldStatus::Delegated,
                RegistryType::DelegatedHttp,
            ),
        );

        let mut rec = DomainRecord {
            domain: "home.fed".into(),
            tld: "fed".into(),
            label: "home".into(),
            owner_public_key: owner.node_id(),
            target_type: TargetType::Manifest,
            manifest_hash: federate_storage_hash(),
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
        rec.signature = Some(operator.sign(&rec.signable_bytes().unwrap()));
        let mut domains = BTreeMap::new();
        domains.insert("home.fed".to_string(), rec);

        let mut zone = RootZone {
            network: "federate".into(),
            root_version: 1,
            generated_at: "t".into(),
            root_public_key: root.node_id(),
            tlds,
            domains,
            audit: vec![],
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(root.sign(&zone.signable_bytes().unwrap()));
        zone.verify(&root.node_id()).unwrap();
        (zone, root)
    }

    // 64-char hex placeholder manifest hash.
    fn federate_storage_hash() -> String {
        "0".repeat(64)
    }

    #[test]
    fn root_managed_tld_routes_to_verified_domain_record() {
        let (zone, _root) = signed_zone("rm-route");
        let view = RegistryView::new(&zone);
        match view.locate_domain("home.fed").unwrap() {
            DomainSource::RootManaged(rec) => assert_eq!(rec.domain, "home.fed"),
            other => panic!("expected root-managed, got {other:?}"),
        }
    }

    #[test]
    fn delegated_tld_routes_to_operator_registry() {
        let (zone, _root) = signed_zone("dlg-route");
        let view = RegistryView::new(&zone);
        match view.locate_domain("store.shop").unwrap() {
            DomainSource::Delegated {
                tld,
                registry_endpoint,
                ..
            } => {
                assert_eq!(tld, "shop");
                assert!(registry_endpoint.is_some());
            }
            other => panic!("expected delegated, got {other:?}"),
        }
    }

    #[test]
    fn expired_domain_record_rejected() {
        let root = NodeIdentity::load_or_create(&tmp("exp-root")).unwrap();
        let operator = NodeIdentity::load_or_create(&tmp("exp-op")).unwrap();
        let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();

        let mut tlds = BTreeMap::new();
        tlds.insert(
            "fed".to_string(),
            tld_rec(
                &root,
                &operator,
                "fed",
                TldStatus::Official,
                RegistryType::RootManaged,
            ),
        );
        let mut rec = DomainRecord {
            domain: "old.fed".into(),
            tld: "fed".into(),
            label: "old".into(),
            owner_public_key: "00".repeat(32),
            target_type: TargetType::Manifest,
            manifest_hash: federate_storage_hash(),
            service_id: None,
            node_id: None,
            status: DomainStatus::Active,
            created_at: "t".into(),
            updated_at: "t".into(),
            expires_at: Some(past),
            renewal: None,
            pricing: None,
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        rec.signature = Some(operator.sign(&rec.signable_bytes().unwrap()));
        let mut domains = BTreeMap::new();
        domains.insert("old.fed".to_string(), rec);
        let mut zone = RootZone {
            network: "federate".into(),
            root_version: 1,
            generated_at: "t".into(),
            root_public_key: root.node_id(),
            tlds,
            domains,
            audit: vec![],
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(root.sign(&zone.signable_bytes().unwrap()));

        let view = RegistryView::new(&zone);
        // Signature is valid, but the lease expired: must NOT resolve.
        assert!(matches!(
            view.locate_domain("old.fed"),
            Err(FederateError::TldUnavailable { status, .. }) if status == "expired"
        ));
    }

    #[test]
    fn expired_tld_rejected() {
        let root = NodeIdentity::load_or_create(&tmp("exptld-root")).unwrap();
        let operator = NodeIdentity::load_or_create(&tmp("exptld-op")).unwrap();
        let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let mut tld = tld_rec(
            &root,
            &operator,
            "lapsed",
            TldStatus::Delegated,
            RegistryType::DelegatedHttp,
        );
        tld.expires_at = Some(past);
        tld.signature = Some(root.sign(&tld.signable_bytes().unwrap()));
        let mut tlds = BTreeMap::new();
        tlds.insert("lapsed".to_string(), tld);
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
        let view = RegistryView::new(&zone);
        assert!(matches!(
            view.locate_domain("shop.lapsed"),
            Err(FederateError::TldUnavailable { status, .. }) if status == "expired"
        ));
    }

    #[test]
    fn unknown_tld_and_missing_domain_error() {
        let (zone, _root) = signed_zone("unknown-tld");
        let view = RegistryView::new(&zone);
        assert!(matches!(
            view.locate_domain("x.doesnotexist"),
            Err(FederateError::TldNotFound { .. })
        ));
        assert!(matches!(
            view.locate_domain("missing.fed"),
            Err(FederateError::DomainNotFound(_))
        ));
    }

    // -----------------------------------------------------------------
    // Signed TLD registry
    // -----------------------------------------------------------------

    fn domain_record(operator: &NodeIdentity, fqdn: &str) -> DomainRecord {
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
            expires_at: None,
            renewal: None,
            pricing: None,
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        rec.signature = Some(operator.sign(&rec.signable_bytes().unwrap()));
        rec
    }

    fn registry_for(operator: &NodeIdentity, tld: &str, fqdns: &[&str]) -> TldRegistry {
        let domains: BTreeMap<String, DomainRecord> = fqdns
            .iter()
            .map(|f| (f.to_string(), domain_record(operator, f)))
            .collect();
        TldRegistry::signed(operator, tld, 1, "t", domains).unwrap()
    }

    #[test]
    fn registry_sign_verify_and_lookup() {
        let operator = NodeIdentity::load_or_create(&tmp("reg-op")).unwrap();
        let reg = registry_for(&operator, "livros", &["eu.livros", "loja.livros"]);
        reg.verify("livros", &operator.node_id()).unwrap();
        assert!(reg.lookup("eu.livros").is_some());
        assert!(reg.lookup("nao.livros").is_none());
        // every entry also verifies against the operator key individually
        for rec in reg.domains.values() {
            rec.verify(&operator.node_id()).unwrap();
        }
    }

    #[test]
    fn registry_signed_by_wrong_key_fails_closed() {
        let operator = NodeIdentity::load_or_create(&tmp("reg-op2")).unwrap();
        let attacker = NodeIdentity::load_or_create(&tmp("reg-atk")).unwrap();
        let mut reg = registry_for(&operator, "livros", &["eu.livros"]);
        // attacker re-signs and claims their own key: key mismatch
        reg.operator_public_key = attacker.node_id();
        reg.signature = Some(attacker.sign(&reg.signable_bytes().unwrap()));
        assert!(reg.verify("livros", &operator.node_id()).is_err());
        // attacker re-signs but keeps the operator key claim: bad signature
        let mut forged = registry_for(&operator, "livros", &["eu.livros"]);
        forged.signature = Some(attacker.sign(&forged.signable_bytes().unwrap()));
        assert!(forged.verify("livros", &operator.node_id()).is_err());
        // unsigned
        let mut unsigned = registry_for(&operator, "livros", &["eu.livros"]);
        unsigned.signature = None;
        assert!(unsigned.verify("livros", &operator.node_id()).is_err());
    }

    #[test]
    fn tampered_registry_fails_closed() {
        let operator = NodeIdentity::load_or_create(&tmp("reg-op3")).unwrap();
        let mut reg = registry_for(&operator, "livros", &["eu.livros"]);
        reg.domains
            .insert("mal.livros".into(), domain_record(&operator, "mal.livros"));
        assert!(reg.verify("livros", &operator.node_id()).is_err());
    }

    #[test]
    fn registry_file_roundtrip_preserves_exact_bytes() {
        let operator = NodeIdentity::load_or_create(&tmp("reg-file-op")).unwrap();
        let reg = registry_for(&operator, "livros", &["eu.livros"]);
        let bytes = serde_json::to_vec(&reg).unwrap();
        let dir = tmp("reg-file");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("livros.json");
        std::fs::write(&path, &bytes).unwrap();

        let (loaded_bytes, loaded) = load_registry_file(&path).unwrap();
        // Bytes verbatim: the content address of a delegated_manifest
        // registry is the hash of these exact bytes.
        assert_eq!(loaded_bytes, bytes);
        loaded.verify("livros", &operator.node_id()).unwrap();

        // Malformed file fails cleanly.
        std::fs::write(&path, b"not a registry").unwrap();
        assert!(load_registry_file(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn registry_cannot_smuggle_foreign_tld_records() {
        let operator = NodeIdentity::load_or_create(&tmp("reg-op4")).unwrap();
        let mut domains = BTreeMap::new();
        domains.insert(
            "eu.livros".to_string(),
            domain_record(&operator, "eu.livros"),
        );
        // an entry whose record belongs to a different TLD, signed and all
        domains.insert("home.fed".to_string(), domain_record(&operator, "home.fed"));
        let reg = TldRegistry::signed(&operator, "livros", 1, "t", domains).unwrap();
        assert!(reg.verify("livros", &operator.node_id()).is_err());
        // wrong expected TLD is also rejected
        let ok = registry_for(&operator, "livros", &["eu.livros"]);
        assert!(ok.verify("outra", &operator.node_id()).is_err());
    }
}
