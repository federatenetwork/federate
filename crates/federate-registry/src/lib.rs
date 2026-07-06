//! federate-registry: registry model on top of the signed root zone.
//!
//! Hierarchy: Federate Root Registry → TLD (root-managed or delegated) →
//! domain records. The root zone is the single source of truth for which
//! TLDs exist and who operates them; this crate answers "where does the
//! domain record for X live, and is it valid?".

use federate_core::{FederateError, Result};
use federate_naming::{DomainRecord, FederateDomain, RegistryType};
use federate_root::{RootZone, TldRecord};

/// Where a domain's record comes from.
#[derive(Debug)]
pub enum DomainSource {
    /// Record lives in the signed root zone itself.
    RootManaged(Box<DomainRecord>),
    /// TLD is delegated to an operator registry at this endpoint. The record
    /// must be fetched from the operator and verified against the operator
    /// key in the (root-signed) TLD record.
    Delegated {
        tld: String,
        operator_public_key: String,
        registry_endpoint: Option<String>,
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
                registry_endpoint: tld_rec.registry_endpoint.clone(),
            }),
        }
    }
}

/// Client for a delegated TLD operator registry. Fetches domain records over
/// HTTP and verifies them against the operator key from the root-signed TLD
/// record; the operator server is never trusted blindly.
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

    pub async fn fetch_domain(&self, fqdn: &str) -> Result<DomainRecord> {
        // Operator registries are untrusted peers: same timeout + download
        // cap as every other cross-node fetch, then signature verification.
        let url = format!("{}/v1/domain/{fqdn}", self.endpoint);
        let rec: DomainRecord = serde_json::from_value(federate_client::get_json(&url).await?)?;
        rec.verify(&self.operator_public_key)?;
        if rec.is_expired() {
            return Err(FederateError::TldUnavailable {
                tld: rec.tld.clone(),
                status: "expired".into(),
            });
        }
        Ok(rec)
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

    fn signed_zone() -> (RootZone, NodeIdentity) {
        let root = NodeIdentity::load_or_create(&tmp("root")).unwrap();
        let operator = NodeIdentity::load_or_create(&tmp("op")).unwrap();
        let owner = NodeIdentity::load_or_create(&tmp("owner")).unwrap();

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
        let (zone, _root) = signed_zone();
        let view = RegistryView::new(&zone);
        match view.locate_domain("home.fed").unwrap() {
            DomainSource::RootManaged(rec) => assert_eq!(rec.domain, "home.fed"),
            other => panic!("expected root-managed, got {other:?}"),
        }
    }

    #[test]
    fn delegated_tld_routes_to_operator_registry() {
        let (zone, _root) = signed_zone();
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
        let (zone, _root) = signed_zone();
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
}
