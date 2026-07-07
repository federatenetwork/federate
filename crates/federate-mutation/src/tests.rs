//! Persistent-registry and mutation tests: first-boot seed, restart
//! persistence, authorization, replay/rollback rejection, and a full native
//! protocol resolution of a runtime-published package.

use crate::{
    MutationAction, MutationContext, MutationRequest, NonceStore, RegistryStore,
    SIGNATURE_ALGORITHM,
};
use federate_core::FederateError;
use federate_identity::NodeIdentity;
use federate_manifest::Manifest;
use federate_naming::{DomainRecord, DomainStatus, RegistryType, TargetType, TldMode, TldStatus};
use federate_root::{Blocklists, RootZone, TldRecord};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fedmut-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    dir: PathBuf,
    root: NodeIdentity,
    operator: NodeIdentity,
    other_operator: NodeIdentity,
    owner: NodeIdentity,
    blocklists: Blocklists,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = tmp(name);
        Fixture {
            root: NodeIdentity::load_or_create(&dir.join("keys/root")).unwrap(),
            operator: NodeIdentity::load_or_create(&dir.join("keys/operator")).unwrap(),
            other_operator: NodeIdentity::load_or_create(&dir.join("keys/other-op")).unwrap(),
            owner: NodeIdentity::load_or_create(&dir.join("keys/owner")).unwrap(),
            blocklists: Blocklists::default(),
            dir,
        }
    }

    fn registry_dir(&self) -> PathBuf {
        self.dir.join("registry")
    }

    fn tld_record(&self, tld: &str, operator: &NodeIdentity) -> TldRecord {
        let now = chrono::Utc::now().to_rfc3339();
        let mut rec = TldRecord {
            tld: tld.into(),
            status: TldStatus::Official,
            mode: TldMode::Official,
            owner_public_key: self.root.node_id(),
            operator_public_key: operator.node_id(),
            operator_name: "test operator".into(),
            registry_type: RegistryType::RootManaged,
            registry_endpoint: None,
            registry_manifest_hash: None,
            registry_providers: Vec::new(),
            policy_hash: None,
            pricing: None,
            created_at: now.clone(),
            updated_at: now,
            expires_at: None,
            notes: None,
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        rec.signature = Some(self.root.sign(&rec.signable_bytes().unwrap()));
        rec
    }

    /// First-boot seed: two official root-managed TLDs with different
    /// operators, no domains yet.
    fn seed(&self) -> RegistryStore {
        let mut tlds = BTreeMap::new();
        tlds.insert(
            "pagina".to_string(),
            self.tld_record("pagina", &self.operator),
        );
        tlds.insert(
            "tipos".to_string(),
            self.tld_record("tipos", &self.other_operator),
        );
        let mut zone = RootZone {
            network: federate_core::NETWORK_NAME.into(),
            root_version: 1_000,
            generated_at: chrono::Utc::now().to_rfc3339(),
            root_public_key: self.root.node_id(),
            tlds,
            domains: BTreeMap::new(),
            audit: Vec::new(),
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(self.root.sign(&zone.signable_bytes().unwrap()));
        RegistryStore::init(
            &self.registry_dir(),
            zone,
            BTreeMap::new(),
            BTreeMap::new(),
            Vec::new(),
        )
        .unwrap()
    }

    fn ctx(&self) -> MutationContext<'_> {
        MutationContext {
            root: &self.root,
            official_operator: &self.operator,
            blocklists: &self.blocklists,
            now: chrono::Utc::now(),
        }
    }

    /// Build an owner-signed manifest + one content block for `domain`.
    fn site(&self, domain: &str, body: &str) -> (Vec<u8>, String, Vec<crate::ContentBlock>) {
        let block = body.as_bytes().to_vec();
        let block_hash = federate_storage::hash_bytes(&block);
        let mut files = BTreeMap::new();
        files.insert("index.html".to_string(), block_hash.clone());
        let mut manifest = Manifest {
            domain: domain.into(),
            version: 1,
            entry: "index.html".into(),
            files,
            owner_public_key: self.owner.node_id(),
            created_at: chrono::Utc::now().to_rfc3339(),
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        manifest.signature = Some(self.owner.sign(&manifest.signable_bytes().unwrap()));
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let hash = federate_storage::hash_bytes(&bytes);
        (bytes, hash, vec![(block_hash, block)])
    }

    fn mutation(
        &self,
        signer: &NodeIdentity,
        target_version: u64,
        action: MutationAction,
    ) -> MutationRequest {
        MutationRequest::signed(
            signer,
            "test-nonce",
            &chrono::Utc::now().to_rfc3339(),
            target_version,
            action,
        )
        .unwrap()
    }

    /// Publish a site under `domain` as the fixture owner.
    fn publish(
        &self,
        store: &mut RegistryStore,
        domain: &str,
        body: &str,
        target_version: u64,
    ) -> federate_core::Result<crate::AuditRecord> {
        let (manifest_bytes, manifest_hash, blocks) = self.site(domain, body);
        store.store_blocks(&blocks).unwrap();
        store
            .store_manifest(&manifest_hash, &manifest_bytes)
            .unwrap();
        let req = self.mutation(
            &self.owner,
            target_version,
            MutationAction::PublishSite {
                domain: domain.into(),
                manifest_hash,
            },
        );
        store.apply(&req, &self.ctx())
    }
}

// ---------------------------------------------------------------------------
// persistence
// ---------------------------------------------------------------------------

#[test]
fn first_boot_seed_creates_registry() {
    let fix = Fixture::new("first-boot");
    assert!(!RegistryStore::exists(&fix.registry_dir()));
    let store = fix.seed();
    assert!(RegistryStore::exists(&fix.registry_dir()));
    assert_eq!(store.zone().tlds.len(), 2);
    assert!(fix.registry_dir().join("state.json").is_file());
    assert!(fix
        .registry_dir()
        .join("snapshots/root-zone-v1000.json")
        .is_file());
}

#[test]
fn restart_preserves_registry_state() {
    let fix = Fixture::new("restart");
    let mut store = fix.seed();
    fix.publish(&mut store, "joao.pagina", "<h1>oi</h1>", 1)
        .unwrap();
    let version_before = store.zone().root_version;
    drop(store);

    // "Restart": reload from disk only; the seed never runs again.
    let store = RegistryStore::open(&fix.registry_dir(), &fix.root.node_id()).unwrap();
    assert_eq!(store.zone().root_version, version_before);
    let rec = store
        .zone()
        .lookup("joao.pagina")
        .expect("domain survives restart");
    assert_eq!(rec.status, DomainStatus::Active);
    assert!(store.manifest(&rec.manifest_hash).is_some());
    assert_eq!(store.mutation_count(), 1);
    assert_eq!(store.audit_count(), 1);
}

#[test]
fn tampered_state_fails_closed_on_open() {
    let fix = Fixture::new("tamper");
    let mut store = fix.seed();
    fix.publish(&mut store, "joao.pagina", "x", 1).unwrap();
    drop(store);
    let state_path = fix.registry_dir().join("state.json");
    let tampered = std::fs::read_to_string(&state_path)
        .unwrap()
        .replace("joao.pagina", "eval.pagina");
    std::fs::write(&state_path, tampered).unwrap();
    assert!(RegistryStore::open(&fix.registry_dir(), &fix.root.node_id()).is_err());
}

// ---------------------------------------------------------------------------
// mutations mutate without seed edits; versions increment
// ---------------------------------------------------------------------------

#[test]
fn mutation_updates_root_zone_without_seed_edit() {
    let fix = Fixture::new("runtime-mutation");
    let mut store = fix.seed();
    assert!(store.zone().lookup("joao.pagina").is_none());
    let before = store.zone().root_version;
    let event = fix
        .publish(&mut store, "joao.pagina", "<h1>oi</h1>", 1)
        .unwrap();
    assert!(store.zone().lookup("joao.pagina").is_some());
    assert!(
        store.zone().root_version > before,
        "root zone version increments"
    );
    assert_eq!(event.action, "domain.publish");
    assert_eq!(event.target_id, "joao.pagina");
    event.verify(&fix.root.node_id()).unwrap();
    assert_ne!(event.previous_state_hash, event.new_state_hash);
}

#[test]
fn root_zone_version_strictly_increases_and_snapshots_pile_up() {
    let fix = Fixture::new("versions");
    let mut store = fix.seed();
    fix.publish(&mut store, "a.pagina", "a", 1).unwrap();
    let v1 = store.zone().root_version;
    fix.publish(&mut store, "b.pagina", "b", 1).unwrap();
    let v2 = store.zone().root_version;
    assert!(
        v2 > v1,
        "every mutation bumps the version (rollback protection)"
    );
    assert!(fix
        .registry_dir()
        .join(format!("snapshots/root-zone-v{v1}.json"))
        .is_file());
    assert!(fix
        .registry_dir()
        .join(format!("snapshots/root-zone-v{v2}.json"))
        .is_file());
}

// ---------------------------------------------------------------------------
// rejections: unsigned, wrong signer, replay, version rollback
// ---------------------------------------------------------------------------

#[test]
fn unsigned_mutation_rejected() {
    let fix = Fixture::new("unsigned");
    let mut store = fix.seed();
    let (manifest_bytes, manifest_hash, _) = fix.site("joao.pagina", "x");
    store
        .store_manifest(&manifest_hash, &manifest_bytes)
        .unwrap();
    let mut req = fix.mutation(
        &fix.owner,
        1,
        MutationAction::PublishSite {
            domain: "joao.pagina".into(),
            manifest_hash,
        },
    );
    req.signature = None;
    assert!(store.apply(&req, &fix.ctx()).is_err());
}

#[test]
fn wrong_signer_rejected() {
    let fix = Fixture::new("wrong-signer");
    let mut store = fix.seed();
    let (manifest_bytes, manifest_hash, _) = fix.site("joao.pagina", "x");
    store
        .store_manifest(&manifest_hash, &manifest_bytes)
        .unwrap();

    // Attacker signs the envelope but claims the owner's key.
    let attacker = NodeIdentity::load_or_create(&fix.dir.join("keys/attacker")).unwrap();
    let mut req = fix.mutation(
        &attacker,
        1,
        MutationAction::PublishSite {
            domain: "joao.pagina".into(),
            manifest_hash: manifest_hash.clone(),
        },
    );
    req.actor_public_key = fix.owner.node_id();
    req.mutation_id = req.compute_id().unwrap();
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::InvalidSignature)
    ));

    // Attacker signs honestly with their own key: manifest ownership check
    // fails (the manifest is owner-signed, not attacker-signed).
    let req = fix.mutation(
        &attacker,
        1,
        MutationAction::PublishSite {
            domain: "joao.pagina".into(),
            manifest_hash,
        },
    );
    assert!(store.apply(&req, &fix.ctx()).is_err());
}

#[test]
fn replayed_mutation_rejected() {
    let fix = Fixture::new("replay");
    let mut store = fix.seed();
    let (manifest_bytes, manifest_hash, blocks) = fix.site("joao.pagina", "x");
    store.store_blocks(&blocks).unwrap();
    store
        .store_manifest(&manifest_hash, &manifest_bytes)
        .unwrap();
    let req = fix.mutation(
        &fix.owner,
        1,
        MutationAction::PublishSite {
            domain: "joao.pagina".into(),
            manifest_hash,
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Replay(_))
    ));

    // Replay still rejected after a restart: history is persistent.
    drop(store);
    let mut store = RegistryStore::open(&fix.registry_dir(), &fix.root.node_id()).unwrap();
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Replay(_))
    ));
}

#[test]
fn old_target_version_rejected() {
    let fix = Fixture::new("old-version");
    let mut store = fix.seed();
    fix.publish(&mut store, "joao.pagina", "v1", 3).unwrap();
    // A different, freshly signed mutation that does not advance the
    // target version is a rollback attempt.
    let err = fix.publish(&mut store, "joao.pagina", "v2", 3).unwrap_err();
    assert!(matches!(err, FederateError::Replay(_)));
    let err = fix.publish(&mut store, "joao.pagina", "v2", 2).unwrap_err();
    assert!(matches!(err, FederateError::Replay(_)));
    fix.publish(&mut store, "joao.pagina", "v2", 4).unwrap();
}

#[test]
fn stale_timestamp_rejected() {
    let fix = Fixture::new("stale");
    let mut store = fix.seed();
    let (manifest_bytes, manifest_hash, _) = fix.site("joao.pagina", "x");
    store
        .store_manifest(&manifest_hash, &manifest_bytes)
        .unwrap();
    let old = (chrono::Utc::now() - chrono::Duration::seconds(3600)).to_rfc3339();
    let req = MutationRequest::signed(
        &fix.owner,
        "test-nonce",
        &old,
        1,
        MutationAction::PublishSite {
            domain: "joao.pagina".into(),
            manifest_hash,
        },
    )
    .unwrap();
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Replay(_))
    ));
}

#[test]
fn nonce_challenge_response_is_single_use() {
    let store = NonceStore::default();
    let now = chrono::Utc::now().timestamp();
    let (nonce, _) = store.issue(now);
    assert!(store.consume(&nonce, now + 1));
    assert!(!store.consume(&nonce, now + 2), "replayed nonce rejected");
}

// ---------------------------------------------------------------------------
// owner / operator / root authorization
// ---------------------------------------------------------------------------

#[test]
fn domain_owner_can_update_own_manifest() {
    let fix = Fixture::new("owner-update");
    let mut store = fix.seed();
    fix.publish(&mut store, "joao.pagina", "v1", 1).unwrap();
    let (manifest_bytes, manifest_hash, blocks) = fix.site("joao.pagina", "v2");
    store.store_blocks(&blocks).unwrap();
    store
        .store_manifest(&manifest_hash, &manifest_bytes)
        .unwrap();
    let req = fix.mutation(
        &fix.owner,
        2,
        MutationAction::UpdateDomainManifest {
            domain: "joao.pagina".into(),
            manifest_hash: manifest_hash.clone(),
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    assert_eq!(
        store.zone().lookup("joao.pagina").unwrap().manifest_hash,
        manifest_hash
    );
}

#[test]
fn wrong_domain_owner_cannot_update_manifest() {
    let fix = Fixture::new("wrong-owner");
    let mut store = fix.seed();
    fix.publish(&mut store, "joao.pagina", "v1", 1).unwrap();

    // A different key builds its own (valid!) manifest for the same domain.
    let intruder = NodeIdentity::load_or_create(&fix.dir.join("keys/intruder")).unwrap();
    let block = b"takeover".to_vec();
    let block_hash = federate_storage::hash_bytes(&block);
    let mut files = BTreeMap::new();
    files.insert("index.html".to_string(), block_hash);
    let mut manifest = Manifest {
        domain: "joao.pagina".into(),
        version: 9,
        entry: "index.html".into(),
        files,
        owner_public_key: intruder.node_id(),
        created_at: chrono::Utc::now().to_rfc3339(),
        signature_algorithm: SIGNATURE_ALGORITHM.into(),
        signature: None,
    };
    manifest.signature = Some(intruder.sign(&manifest.signable_bytes().unwrap()));
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let hash = federate_storage::hash_bytes(&bytes);
    store.store_manifest(&hash, &bytes).unwrap();

    let req = fix.mutation(
        &intruder,
        2,
        MutationAction::UpdateDomainManifest {
            domain: "joao.pagina".into(),
            manifest_hash: hash,
        },
    );
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Unauthorized(_))
    ));
}

#[test]
fn tld_operator_can_issue_domain_inside_own_tld() {
    let fix = Fixture::new("op-issue");
    let mut store = fix.seed();
    let now = chrono::Utc::now().to_rfc3339();
    let mut record = DomainRecord {
        domain: "loja.pagina".into(),
        tld: "pagina".into(),
        label: "loja".into(),
        owner_public_key: fix.owner.node_id(),
        target_type: TargetType::Manifest,
        manifest_hash: federate_storage::hash_bytes(b"external manifest"),
        service_id: None,
        node_id: None,
        status: DomainStatus::Active,
        created_at: now.clone(),
        updated_at: now,
        expires_at: None,
        renewal: None,
        pricing: None,
        signature_algorithm: SIGNATURE_ALGORITHM.into(),
        signature: None,
    };
    record.signature = Some(fix.operator.sign(&record.signable_bytes().unwrap()));
    let req = fix.mutation(
        &fix.operator,
        1,
        MutationAction::IssueDomain {
            record: Box::new(record),
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    assert!(store.zone().lookup("loja.pagina").is_some());
}

#[test]
fn tld_operator_cannot_issue_domain_outside_own_tld() {
    let fix = Fixture::new("op-cross-tld");
    let mut store = fix.seed();
    let now = chrono::Utc::now().to_rfc3339();
    // fix.operator operates .pagina, NOT .tipos.
    let mut record = DomainRecord {
        domain: "loja.tipos".into(),
        tld: "tipos".into(),
        label: "loja".into(),
        owner_public_key: fix.owner.node_id(),
        target_type: TargetType::Manifest,
        manifest_hash: federate_storage::hash_bytes(b"m"),
        service_id: None,
        node_id: None,
        status: DomainStatus::Active,
        created_at: now.clone(),
        updated_at: now,
        expires_at: None,
        renewal: None,
        pricing: None,
        signature_algorithm: SIGNATURE_ALGORITHM.into(),
        signature: None,
    };
    record.signature = Some(fix.operator.sign(&record.signable_bytes().unwrap()));
    let req = fix.mutation(
        &fix.operator,
        1,
        MutationAction::IssueDomain {
            record: Box::new(record),
        },
    );
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Unauthorized(_))
    ));
}

#[test]
fn root_can_delegate_tld_and_others_cannot() {
    let fix = Fixture::new("delegate");
    let mut store = fix.seed();
    let op = NodeIdentity::load_or_create(&fix.dir.join("keys/new-op")).unwrap();
    let action = MutationAction::DelegateTld {
        tld: "quintal".into(),
        owner_public_key: op.node_id(),
        operator_public_key: op.node_id(),
        operator_name: "quintal operator".into(),
        registry_type: RegistryType::DelegatedManifest,
        registry_endpoint: None,
        expires_at: None,
    };

    // Non-root signer: fail closed.
    let req = fix.mutation(&fix.operator, 1, action.clone());
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Unauthorized(_))
    ));

    // Root key: accepted, root-signed record appears.
    let req = fix.mutation(&fix.root, 1, action);
    store.apply(&req, &fix.ctx()).unwrap();
    let rec = store.zone().lookup_tld("quintal").unwrap();
    assert_eq!(rec.status, TldStatus::Delegated);
    assert_eq!(rec.operator_public_key, op.node_id());
    rec.verify(&fix.root.node_id()).unwrap();
}

#[test]
fn delegated_operator_updates_registry_pointer_with_rollback_protection() {
    let fix = Fixture::new("pointer");
    let mut store = fix.seed();
    let op = NodeIdentity::load_or_create(&fix.dir.join("keys/qop")).unwrap();
    let req = fix.mutation(
        &fix.root,
        1,
        MutationAction::DelegateTld {
            tld: "quintal".into(),
            owner_public_key: op.node_id(),
            operator_public_key: op.node_id(),
            operator_name: "quintal".into(),
            registry_type: RegistryType::DelegatedManifest,
            registry_endpoint: None,
            expires_at: None,
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();

    let signed_registry = |version: u64| {
        let registry = federate_registry::TldRegistry::signed(
            &op,
            "quintal",
            version,
            &chrono::Utc::now().to_rfc3339(),
            BTreeMap::new(),
        )
        .unwrap();
        hex::encode(serde_json::to_vec(&registry).unwrap())
    };

    // v5 accepted; the TLD record now pins the registry hash.
    let req = fix.mutation(
        &op,
        2,
        MutationAction::UpdateRegistryPointer {
            tld: "quintal".into(),
            registry_hex: signed_registry(5),
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    let pinned = store
        .zone()
        .lookup_tld("quintal")
        .unwrap()
        .registry_manifest_hash
        .clone()
        .expect("pointer pinned");
    assert!(
        store.manifest(&pinned).is_some(),
        "registry fetchable as content"
    );

    // v5 again (registry version rollback): rejected.
    let req = fix.mutation(
        &op,
        3,
        MutationAction::UpdateRegistryPointer {
            tld: "quintal".into(),
            registry_hex: signed_registry(5),
        },
    );
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Replay(_))
    ));

    // Wrong operator: rejected.
    let req = fix.mutation(
        &fix.operator,
        3,
        MutationAction::UpdateRegistryPointer {
            tld: "quintal".into(),
            registry_hex: signed_registry(6),
        },
    );
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Unauthorized(_))
    ));
}

#[test]
fn suspend_reinstate_transitions_enforced() {
    let fix = Fixture::new("status");
    let mut store = fix.seed();
    fix.publish(&mut store, "joao.pagina", "x", 1).unwrap();

    // Owner cannot suspend their own domain (enforcement is operator/root).
    let req = fix.mutation(
        &fix.owner,
        2,
        MutationAction::SetDomainStatus {
            domain: "joao.pagina".into(),
            status: DomainStatus::Suspended,
        },
    );
    assert!(matches!(
        store.apply(&req, &fix.ctx()),
        Err(FederateError::Unauthorized(_))
    ));

    // Operator suspends; record stops being resolvable.
    let req = fix.mutation(
        &fix.operator,
        2,
        MutationAction::SetDomainStatus {
            domain: "joao.pagina".into(),
            status: DomainStatus::Suspended,
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    let rec = store.zone().lookup("joao.pagina").unwrap();
    assert_eq!(rec.status, DomainStatus::Suspended);
    assert!(!rec.status.is_resolvable());

    // Suspended blocks owner updates.
    let err = fix.publish(&mut store, "joao.pagina", "y", 3).unwrap_err();
    assert!(matches!(err, FederateError::MutationRejected(_)));

    // Root reinstates.
    let req = fix.mutation(
        &fix.root,
        3,
        MutationAction::SetDomainStatus {
            domain: "joao.pagina".into(),
            status: DomainStatus::Active,
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    assert_eq!(
        store.zone().lookup("joao.pagina").unwrap().status,
        DomainStatus::Active
    );

    // Revoked is terminal for the operator: revoke, then operator cannot
    // reactivate, root can.
    let req = fix.mutation(
        &fix.operator,
        4,
        MutationAction::SetDomainStatus {
            domain: "joao.pagina".into(),
            status: DomainStatus::Revoked,
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    let req = fix.mutation(
        &fix.operator,
        5,
        MutationAction::SetDomainStatus {
            domain: "joao.pagina".into(),
            status: DomainStatus::Active,
        },
    );
    assert!(store.apply(&req, &fix.ctx()).is_err());
    let req = fix.mutation(
        &fix.root,
        5,
        MutationAction::SetDomainStatus {
            domain: "joao.pagina".into(),
            status: DomainStatus::Active,
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
}

#[test]
fn registry_self_verification_passes() {
    let fix = Fixture::new("verify-all");
    let mut store = fix.seed();
    fix.publish(&mut store, "joao.pagina", "x", 1).unwrap();
    let report = store.verify_all(&fix.root.node_id()).unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(report["domains"], 1);
}

// ---------------------------------------------------------------------------
// a runtime-published package resolves over the NATIVE protocol
// ---------------------------------------------------------------------------

struct TestNode(std::sync::Arc<tokio::sync::RwLock<RegistryStore>>);

#[federate_transport::async_trait]
impl federate_transport::NodeService for TestNode {
    fn node_id(&self) -> String {
        "test-node".into()
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
        let store = self.0.read().await;
        match request {
            Message::GetRoot => Message::Root {
                zone_json: serde_json::to_vec(store.zone()).unwrap(),
            },
            Message::GetManifest { hash } => match store.manifest(&hash) {
                Some(bytes) => Message::Manifest {
                    hash,
                    bytes: bytes.clone(),
                },
                None => Message::Error {
                    code: ErrorCode::NotFound,
                    detail: "no such manifest".into(),
                },
            },
            Message::GetBlock { hash } => match store.block(&hash) {
                Some(bytes) => Message::Block { hash, bytes },
                None => Message::Error {
                    code: ErrorCode::NotFound,
                    detail: "no such block".into(),
                },
            },
            _ => Message::Error {
                code: ErrorCode::Unsupported,
                detail: "test node".into(),
            },
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn published_package_resolves_over_native_protocol() {
    let fix = Fixture::new("native-resolve");
    let mut store = fix.seed();
    fix.publish(&mut store, "joao.pagina", "<h1>native oi</h1>", 1)
        .unwrap();
    let root_key = fix.root.node_id();
    let shared = std::sync::Arc::new(tokio::sync::RwLock::new(store));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(federate_transport::serve(
        listener,
        std::sync::Arc::new(TestNode(shared)),
        "federate-mutation-test/0".into(),
    ));

    // Verifying resolver: HTTP client points at a dead port, so the whole
    // chain (root -> TLD -> domain -> manifest -> block) must come over the
    // native protocol and verify locally.
    let client_dir = fix.dir.join("client");
    std::fs::create_dir_all(&client_dir).unwrap();
    let resolver = federate_resolution::Resolver::new(
        federate_client::NodeClient::new("http://127.0.0.1:1"),
        &client_dir,
        Some(root_key),
    )
    .unwrap()
    .with_native_providers(vec![addr.to_string()]);

    let uri = federate_uri::FederateUri::parse("fed://joao.pagina/").unwrap();
    match resolver.resolve_uri(&uri).await.unwrap() {
        federate_resolution::Resolved::Content { bytes, .. } => {
            assert_eq!(bytes, b"<h1>native oi</h1>");
        }
        other => panic!("expected content, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// TLD source of truth: database only, seeded by explicit command, never by
// code constants
// ---------------------------------------------------------------------------

const TEST_SEED: &str = r#"
[[tlds]]
name = "fedx"
mode = "official"
purpose = "Federate core namespace (test)"

[[tlds]]
name = "livros"
mode = "official"
purpose = "Books and reading (test)"

[[tlds]]
name = "cofre"
mode = "reserved"
reason = "kept for future infrastructure (test)"
"#;

#[test]
fn registry_starts_empty_before_explicit_init() {
    let fix = Fixture::new("empty-init");
    assert!(
        !RegistryStore::exists(&fix.registry_dir()),
        "no registry exists before explicit init"
    );
    let store = crate::init_empty_registry(&fix.registry_dir(), &fix.root).unwrap();
    assert!(RegistryStore::exists(&fix.registry_dir()));
    assert!(store.zone().tlds.is_empty(), "init creates ZERO TLDs");
    assert!(store.zone().domains.is_empty());
    store.zone().verify(&fix.root.node_id()).unwrap();

    // Init is explicit and single-shot: a second init refuses.
    assert!(crate::init_empty_registry(&fix.registry_dir(), &fix.root).is_err());

    // A restart of an un-seeded node keeps the registry empty: nothing
    // recreates TLDs from code.
    drop(store);
    let store = RegistryStore::open(&fix.registry_dir(), &fix.root.node_id()).unwrap();
    assert!(store.zone().tlds.is_empty(), "no auto-seed on restart");
}

#[test]
fn seed_file_creates_official_tlds_via_audited_mutations() {
    let fix = Fixture::new("seed-file");
    let mut store = crate::init_empty_registry(&fix.registry_dir(), &fix.root).unwrap();
    let seed = crate::SeedFile::parse(TEST_SEED).unwrap();
    let before = store.zone().root_version;
    let outcome = crate::apply_seed(&mut store, &seed, &fix.ctx(), false).unwrap();
    assert_eq!(outcome.created, vec!["fedx", "livros", "cofre"]);

    let rec = store
        .zone()
        .lookup_tld("fedx")
        .expect("official TLD created");
    assert_eq!(rec.status, TldStatus::Official);
    assert_eq!(rec.registry_type, RegistryType::RootManaged);
    rec.verify(&fix.root.node_id()).unwrap();

    let reserved = store.zone().lookup_tld("cofre").unwrap();
    assert_eq!(reserved.status, TldStatus::Reserved);
    assert!(!reserved.status.is_resolvable());

    assert!(
        store.zone().root_version > before,
        "seed signs new zone versions"
    );
    assert_eq!(store.audit_count(), 3, "one audit event per seeded TLD");
    assert_eq!(
        store.mutation_count(),
        3,
        "seed goes through the mutation path"
    );
}

#[test]
fn seed_refuses_populated_registry_and_force_only_adds_missing() {
    let fix = Fixture::new("seed-force");
    let mut store = crate::init_empty_registry(&fix.registry_dir(), &fix.root).unwrap();
    let seed = crate::SeedFile::parse(TEST_SEED).unwrap();
    crate::apply_seed(&mut store, &seed, &fix.ctx(), false).unwrap();

    // Default: an already-populated registry is never re-seeded.
    let err = crate::apply_seed(&mut store, &seed, &fix.ctx(), false).unwrap_err();
    assert!(matches!(err, FederateError::MutationRejected(_)));

    // Changing the seed file alone changes nothing (data, not authority):
    // the registry keeps its records until a command runs.
    let mut extended = crate::SeedFile::parse(TEST_SEED).unwrap();
    extended.tlds.push(crate::SeedTld {
        name: "novo".into(),
        mode: "official".into(),
        purpose: Some("added later (test)".into()),
        reason: None,
    });
    assert!(store.zone().lookup_tld("novo").is_none());

    // --force adds ONLY the missing entry; existing records are untouched.
    let fedx_before = store.zone().lookup_tld("fedx").unwrap().clone();
    let outcome = crate::apply_seed(&mut store, &extended, &fix.ctx(), true).unwrap();
    assert_eq!(outcome.created, vec!["novo"]);
    assert_eq!(outcome.skipped_existing.len(), 3);
    assert_eq!(
        store.zone().lookup_tld("fedx").unwrap().updated_at,
        fedx_before.updated_at,
        "force never overwrites existing records"
    );
}

#[test]
fn seeded_and_mutated_tlds_persist_across_restart() {
    let fix = Fixture::new("seed-restart");
    let mut store = crate::init_empty_registry(&fix.registry_dir(), &fix.root).unwrap();
    let seed = crate::SeedFile::parse(TEST_SEED).unwrap();
    crate::apply_seed(&mut store, &seed, &fix.ctx(), false).unwrap();

    // One more TLD via a runtime signed mutation (the online path).
    let req = fix.mutation(
        &fix.root,
        1,
        MutationAction::CreateTld {
            tld: "quintal".into(),
            purpose: "runtime-created TLD (test)".into(),
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    let version = store.zone().root_version;
    drop(store);

    // Restart: the database is the source of truth; nothing is recreated
    // or overwritten from code.
    let store = RegistryStore::open(&fix.registry_dir(), &fix.root.node_id()).unwrap();
    assert_eq!(store.zone().root_version, version);
    for tld in ["fedx", "livros", "cofre", "quintal"] {
        assert!(
            store.zone().lookup_tld(tld).is_some(),
            ".{tld} survives restart"
        );
    }
    assert_eq!(store.zone().tlds.len(), 4);
}

#[test]
fn blocked_tlds_file_prevents_forbidden_tlds() {
    let fix = Fixture::new("seed-blocked");
    let mut store = crate::init_empty_registry(&fix.registry_dir(), &fix.root).unwrap();
    let mut blocklists = Blocklists::default();
    blocklists.iana.insert("com".into());
    let ctx = MutationContext {
        root: &fix.root,
        official_operator: &fix.operator,
        blocklists: &blocklists,
        now: chrono::Utc::now(),
    };

    // Direct mutation rejected.
    let req = fix.mutation(
        &fix.root,
        1,
        MutationAction::CreateTld {
            tld: "com".into(),
            purpose: "collision attempt".into(),
        },
    );
    assert!(matches!(
        store.apply(&req, &ctx),
        Err(FederateError::BlockedTld { .. })
    ));

    // Seed file containing a blocked name rejected too.
    let seed = crate::SeedFile::parse(
        "[[tlds]]\nname = \"com\"\nmode = \"official\"\npurpose = \"nope\"\n",
    )
    .unwrap();
    assert!(crate::apply_seed(&mut store, &seed, &ctx, false).is_err());
    assert!(store.zone().lookup_tld("com").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn newly_created_tld_resolves_natively_without_code_change() {
    // The resolver (and therefore the gateway, which is a pure adapter on
    // top of it) and the DNS zone gate all consume the served signed zone.
    // A TLD created purely through database mutations must work end to end
    // with no code or seed edits.
    let fix = Fixture::new("new-tld-native");
    let mut store = crate::init_empty_registry(&fix.registry_dir(), &fix.root).unwrap();

    // TLD exists only as a database record created by a signed mutation.
    let req = fix.mutation(
        &fix.root,
        1,
        MutationAction::CreateTld {
            tld: "pagina".into(),
            purpose: "created at runtime (test)".into(),
        },
    );
    store.apply(&req, &fix.ctx()).unwrap();
    fix.publish(&mut store, "nova.pagina", "<p>tld from database</p>", 1)
        .unwrap();

    let root_key = fix.root.node_id();
    let shared = std::sync::Arc::new(tokio::sync::RwLock::new(store));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(federate_transport::serve(
        listener,
        std::sync::Arc::new(TestNode(shared)),
        "federate-mutation-test/0".into(),
    ));

    let client_dir = fix.dir.join("client");
    std::fs::create_dir_all(&client_dir).unwrap();
    let resolver = federate_resolution::Resolver::new(
        federate_client::NodeClient::new("http://127.0.0.1:1"),
        &client_dir,
        Some(root_key),
    )
    .unwrap()
    .with_native_providers(vec![addr.to_string()]);
    let uri = federate_uri::FederateUri::parse("fed://nova.pagina/").unwrap();
    match resolver.resolve_uri(&uri).await.unwrap() {
        federate_resolution::Resolved::Content { bytes, .. } => {
            assert_eq!(bytes, b"<p>tld from database</p>");
        }
        other => panic!("expected content, got {other:?}"),
    }
}

#[test]
fn no_hardcoded_tld_list_exists_in_runtime_code() {
    // Source scan: FEDERATE_TLDS (and its membership helper) must not
    // exist anywhere in the workspace. The TLD set is database state.
    fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                scan(&path, hits);
            } else if path.extension().and_then(|x| x.to_str()) == Some("rs") {
                if path.ends_with("federate-mutation/src/tests.rs") {
                    continue; // this file names the token in the scan itself
                }
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                for token in [
                    "FEDERATE_TLDS",
                    "is_default_official_tld",
                    "SEED_DELEGATED_TLDS",
                ] {
                    if content.contains(token) {
                        hits.push(format!("{}: {token}", path.display()));
                    }
                }
            }
        }
    }
    let crates_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut hits = Vec::new();
    scan(&crates_dir, &mut hits);
    assert!(
        hits.is_empty(),
        "hardcoded TLD list tokens found in source: {hits:?}"
    );
}
