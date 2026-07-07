//! The persistent Federate Root Registry: durable signed state plus the
//! only code path that mutates it.
//!
//! Layout under the registry directory (all JSON, house style: atomic
//! write-to-tmp-then-rename, signatures re-verified on load, fail closed):
//!
//! ```text
//! registry/
//!   state.json        current signed root zone + delegated registries +
//!                     per-target versions (the source of truth)
//!   manifests/<hash>  content-addressed manifest / registry bytes
//!   blocks/           content-addressed site blocks (federate-storage)
//!   audit.jsonl       append-only signed audit log (one event per line)
//!   mutations.jsonl   append-only accepted-mutation history
//!   snapshots/        root-zone-v<version>.json, one per accepted version
//! ```
//!
//! Private keys are NEVER stored in any of these records.

use crate::audit::AuditRecord;
use crate::request::{ActorRole, MutationAction, MutationRequest};
use federate_core::{FederateError, Result};
use federate_identity::NodeIdentity;
use federate_manifest::Manifest;
use federate_naming::{DomainRecord, DomainStatus, RegistryType, TargetType, TldStatus};
use federate_registry::TldRegistry;
use federate_root::{AuditEvent, Blocklists, RootZone, TldRecord, SIGNATURE_ALGORITHM};
use federate_storage::BlockStore;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The root zone embeds only the newest audit events; the full history
/// lives in audit.jsonl.
const ZONE_AUDIT_TAIL: usize = 200;

/// Everything the server needs to authorize and countersign a mutation.
pub struct MutationContext<'a> {
    /// Federate Root Key: signs the zone, TLD records, and audit events.
    pub root: &'a NodeIdentity,
    /// Operator key of the root-managed official TLDs: countersigns domain
    /// records issued through publishing.
    pub official_operator: &'a NodeIdentity,
    pub blocklists: &'a Blocklists,
    pub now: chrono::DateTime<chrono::Utc>,
}

/// One accepted mutation, as recorded in mutations.jsonl.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppliedMutation {
    pub mutation: MutationRequest,
    pub audit_event_id: String,
    pub applied_at: String,
    pub root_version: u64,
}

/// Durable snapshot format of state.json.
#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedState {
    zone: RootZone,
    /// tld -> exact signed registry JSON bytes, hex-encoded
    registries: BTreeMap<String, String>,
    /// target key ("domain:x.y" | "tld:z") -> last accepted mutation version
    target_versions: BTreeMap<String, u64>,
}

pub struct RegistryStore {
    dir: PathBuf,
    zone: RootZone,
    /// tld -> (exact signed bytes, parsed registry)
    registries: BTreeMap<String, (Vec<u8>, TldRegistry)>,
    /// content address -> manifest / registry bytes
    manifests: BTreeMap<String, Vec<u8>>,
    blocks: BlockStore,
    target_versions: BTreeMap<String, u64>,
    applied: BTreeMap<String, AppliedMutation>,
    audit: Vec<AuditRecord>,
}

impl RegistryStore {
    /// True when persistent registry state already exists (seed must not run).
    pub fn exists(dir: &Path) -> bool {
        dir.join("state.json").is_file()
    }

    /// First boot: adopt the seeded state as the initial persistent registry.
    /// The zone must already be signed by the root key.
    pub fn init(
        dir: &Path,
        zone: RootZone,
        registries: BTreeMap<String, (Vec<u8>, TldRegistry)>,
        manifests: BTreeMap<String, Vec<u8>>,
        blocks: Vec<(String, Vec<u8>)>,
    ) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let block_store = BlockStore::new(dir)?;
        for (hash, bytes) in &blocks {
            block_store.put(hash, bytes)?;
        }
        let mut store = RegistryStore {
            dir: dir.to_path_buf(),
            zone,
            registries,
            manifests: BTreeMap::new(),
            blocks: block_store,
            target_versions: BTreeMap::new(),
            applied: BTreeMap::new(),
            audit: Vec::new(),
        };
        for (hash, bytes) in manifests {
            store.write_manifest(&hash, &bytes)?;
        }
        store.persist_state()?;
        store.write_snapshot()?;
        Ok(store)
    }

    /// Every boot after the first: the persistent registry is the source of
    /// truth. Everything is re-verified against the pinned root key; a
    /// tampered state file stops the node instead of serving forged data.
    pub fn open(dir: &Path, expected_root_key: &str) -> Result<Self> {
        let state_path = dir.join("state.json");
        let bytes = std::fs::read(&state_path)?;
        let state: PersistedState = serde_json::from_slice(&bytes)?;
        state.zone.validate()?;
        state.zone.verify(expected_root_key)?;

        let mut registries = BTreeMap::new();
        for (tld, hex_bytes) in state.registries {
            let raw = hex::decode(&hex_bytes).map_err(|_| {
                FederateError::InvalidRoot(format!("persisted .{tld} registry is not hex"))
            })?;
            let parsed: TldRegistry = serde_json::from_slice(&raw)?;
            let record = state.zone.lookup_tld(&tld).ok_or_else(|| {
                FederateError::InvalidRoot(format!(
                    "persisted registry for .{tld} has no TLD record in the zone"
                ))
            })?;
            parsed.verify(&tld, &record.operator_public_key)?;
            registries.insert(tld, (raw, parsed));
        }

        // Content stores: hash-verified on load; a corrupted entry is
        // dropped, never served.
        let mut manifests = BTreeMap::new();
        let manifest_dir = dir.join("manifests");
        if manifest_dir.is_dir() {
            for entry in std::fs::read_dir(&manifest_dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();
                let bytes = std::fs::read(entry.path())?;
                if federate_storage::hash_bytes(&bytes) == name {
                    manifests.insert(name, bytes);
                } else {
                    tracing::warn!("dropping corrupted manifest file {name}");
                }
            }
        }

        let mut applied = BTreeMap::new();
        for line in read_lines(&dir.join("mutations.jsonl"))? {
            match serde_json::from_str::<AppliedMutation>(&line) {
                Ok(m) => {
                    applied.insert(m.mutation.mutation_id.clone(), m);
                }
                Err(e) => tracing::warn!("skipping unreadable mutation history line: {e}"),
            }
        }
        let mut audit = Vec::new();
        for line in read_lines(&dir.join("audit.jsonl"))? {
            match serde_json::from_str::<AuditRecord>(&line) {
                Ok(a) => audit.push(a),
                Err(e) => tracing::warn!("skipping unreadable audit line: {e}"),
            }
        }

        Ok(RegistryStore {
            dir: dir.to_path_buf(),
            zone: state.zone,
            registries,
            manifests,
            blocks: BlockStore::new(dir)?,
            target_versions: state.target_versions,
            applied,
            audit,
        })
    }

    // ------------------------------------------------------------------
    // read surface
    // ------------------------------------------------------------------

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn zone(&self) -> &RootZone {
        &self.zone
    }

    pub fn registry(&self, tld: &str) -> Option<&(Vec<u8>, TldRegistry)> {
        self.registries.get(tld)
    }

    pub fn manifest(&self, hash: &str) -> Option<&Vec<u8>> {
        self.manifests.get(hash)
    }

    pub fn manifest_count(&self) -> usize {
        self.manifests.len()
    }

    pub fn block(&self, hash: &str) -> Option<Vec<u8>> {
        self.blocks.get(hash).ok()
    }

    pub fn block_count(&self) -> usize {
        self.blocks.list().map(|l| l.len()).unwrap_or(0)
    }

    /// A domain record from the root zone or any locally distributed
    /// delegated registry.
    pub fn lookup_domain(&self, fqdn: &str) -> Option<&DomainRecord> {
        if let Some(rec) = self.zone.lookup(fqdn) {
            return Some(rec);
        }
        self.registries
            .values()
            .find_map(|(_, registry)| registry.lookup(fqdn))
    }

    pub fn applied(&self, mutation_id: &str) -> Option<&AppliedMutation> {
        self.applied.get(mutation_id)
    }

    pub fn mutation_count(&self) -> usize {
        self.applied.len()
    }

    pub fn audit_tail(&self, limit: usize) -> &[AuditRecord] {
        let start = self.audit.len().saturating_sub(limit);
        &self.audit[start..]
    }

    pub fn audit_count(&self) -> usize {
        self.audit.len()
    }

    /// Last accepted mutation version for a target key ("domain:x.y").
    pub fn target_version(&self, target_key: &str) -> u64 {
        self.target_versions.get(target_key).copied().unwrap_or(0)
    }

    /// Full self-check of the persisted registry: zone signature, every
    /// delegated registry, every manifest hash, every block, every audit
    /// event signature.
    pub fn verify_all(&self, expected_root_key: &str) -> Result<serde_json::Value> {
        self.zone.validate()?;
        self.zone.verify(expected_root_key)?;
        for (tld, (_, registry)) in &self.registries {
            let record = self.zone.lookup_tld(tld).ok_or_else(|| {
                FederateError::InvalidRoot(format!(".{tld} registry has no TLD record"))
            })?;
            registry.verify(tld, &record.operator_public_key)?;
        }
        for (hash, bytes) in &self.manifests {
            federate_storage::verify(bytes, hash)?;
        }
        let blocks = self.blocks.list()?;
        for (hash, _) in &blocks {
            self.blocks.get(hash)?; // get() re-verifies content
        }
        for event in &self.audit {
            event.verify(expected_root_key)?;
        }
        Ok(serde_json::json!({
            "root_version": self.zone.root_version,
            "tlds": self.zone.tlds.len(),
            "domains": self.zone.domains.len(),
            "delegated_registries": self.registries.len(),
            "manifests": self.manifests.len(),
            "blocks": blocks.len(),
            "audit_events": self.audit.len(),
            "mutations": self.applied.len(),
            "verified": true,
        }))
    }

    // ------------------------------------------------------------------
    // content ingestion (used by package ingest before the mutation runs)
    // ------------------------------------------------------------------

    /// Store verified content blocks (hashes already checked by the caller
    /// or re-checked by the block store).
    pub fn store_blocks(&mut self, blocks: &[(String, Vec<u8>)]) -> Result<()> {
        for (hash, bytes) in blocks {
            self.blocks.put(hash, bytes)?;
        }
        Ok(())
    }

    /// Store content-addressed manifest bytes (hash re-verified here).
    pub fn store_manifest(&mut self, hash: &str, bytes: &[u8]) -> Result<()> {
        federate_storage::verify(bytes, hash)?;
        self.write_manifest(hash, bytes)
    }

    // ------------------------------------------------------------------
    // the mutation path
    // ------------------------------------------------------------------

    /// Apply one signed mutation. This is the ONLY way registry state
    /// changes at runtime. The caller consumes the nonce first; everything
    /// else is enforced here, fail closed:
    /// envelope signature, timestamp window, mutation-id replay,
    /// per-target version rollback, actor authorization against CURRENT
    /// state, and status transition rules. On success the zone is re-signed
    /// with a strictly higher version, persisted, snapshotted, and a signed
    /// audit event is appended and returned.
    pub fn apply(&mut self, req: &MutationRequest, ctx: &MutationContext) -> Result<AuditRecord> {
        req.verify()?;
        req.check_age(ctx.now)?;
        if self.applied.contains_key(&req.mutation_id) {
            return Err(FederateError::Replay(format!(
                "mutation {} was already applied",
                req.mutation_id
            )));
        }
        let target_key = req.action.target_key();
        let current = self.target_version(&target_key);
        if req.target_version <= current {
            return Err(FederateError::Replay(format!(
                "target version {} does not advance {target_key} (current {current})",
                req.target_version
            )));
        }

        // Work on clones; nothing is committed until the new zone verifies.
        let mut zone = self.zone.clone();
        let mut new_registry: Option<(String, Vec<u8>, TldRegistry)> = None;
        let actor_role = self.apply_action(req, ctx, &mut zone, &mut new_registry)?;

        // Re-sign the zone with a strictly increasing version: rollback
        // protection in clients keeps working across restarts and mutations.
        let now = ctx.now.to_rfc3339();
        let prev_hash = state_hash(&self.zone)?;
        zone.root_version = zone
            .root_version
            .saturating_add(1)
            .max(ctx.now.timestamp().max(0) as u64);
        zone.generated_at = now.clone();
        let (kind, id) = req.action.target();
        zone.audit.push(AuditEvent {
            at: now.clone(),
            actor: actor_role.as_str().into(),
            action: req.action.name().into(),
            subject: id.clone(),
            detail: Some(req.mutation_id.clone()),
        });
        if zone.audit.len() > ZONE_AUDIT_TAIL {
            let drop = zone.audit.len() - ZONE_AUDIT_TAIL;
            zone.audit.drain(..drop);
        }
        zone.signature = Some(ctx.root.sign(&zone.signable_bytes()?));
        zone.verify(&ctx.root.node_id())?; // self-check before serving
        let new_hash = state_hash(&zone)?;

        let event = AuditRecord {
            event_id: String::new(),
            mutation_id: req.mutation_id.clone(),
            actor_public_key: req.actor_public_key.clone(),
            actor_role: actor_role.as_str().into(),
            action: req.action.name().into(),
            target_type: kind.as_str().into(),
            target_id: id,
            previous_state_hash: prev_hash,
            new_state_hash: new_hash,
            timestamp: now.clone(),
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        }
        .finalize(ctx.root)?;

        // Commit to memory, then persist. Registry bytes ship as manifests
        // too (content-addressed fetch path).
        self.zone = zone;
        if let Some((tld, bytes, parsed)) = new_registry {
            let hash = federate_storage::hash_bytes(&bytes);
            self.write_manifest(&hash, &bytes)?;
            self.registries.insert(tld, (bytes, parsed));
        }
        self.target_versions.insert(target_key, req.target_version);
        let applied = AppliedMutation {
            mutation: req.clone(),
            audit_event_id: event.event_id.clone(),
            applied_at: now,
            root_version: self.zone.root_version,
        };
        self.applied
            .insert(req.mutation_id.clone(), applied.clone());
        self.audit.push(event.clone());

        self.persist_state()?;
        append_line(&self.dir.join("mutations.jsonl"), &applied)?;
        append_line(&self.dir.join("audit.jsonl"), &event)?;
        self.write_snapshot()?;
        Ok(event)
    }

    /// Authorize and execute one action against the zone clone. Returns the
    /// role that authorized the actor.
    fn apply_action(
        &self,
        req: &MutationRequest,
        ctx: &MutationContext,
        zone: &mut RootZone,
        new_registry: &mut Option<(String, Vec<u8>, TldRegistry)>,
    ) -> Result<ActorRole> {
        let actor = req.actor_public_key.as_str();
        let is_root = actor == zone.root_public_key;
        let now = ctx.now.to_rfc3339();

        match &req.action {
            MutationAction::PublishSite {
                domain,
                manifest_hash,
            }
            | MutationAction::UpdateDomainManifest {
                domain,
                manifest_hash,
            } => {
                let parsed = federate_naming::FederateDomain::parse(domain)
                    .map_err(|e| FederateError::MutationRejected(format!("invalid domain: {e}")))?;
                let fqdn = parsed.fqdn();
                let tld_rec = zone
                    .lookup_tld(&parsed.tld)
                    .ok_or(FederateError::TldNotFound {
                        tld: parsed.tld.clone(),
                    })?;
                if tld_rec.registry_type != RegistryType::RootManaged {
                    return Err(FederateError::MutationRejected(format!(
                        ".{} is delegated; publish through its operator registry",
                        parsed.tld
                    )));
                }
                if !tld_rec.status.is_resolvable() || tld_rec.is_expired() {
                    return Err(FederateError::TldUnavailable {
                        tld: parsed.tld.clone(),
                        status: tld_rec.status.as_str().into(),
                    });
                }
                if tld_rec.operator_public_key != ctx.official_operator.node_id() {
                    return Err(FederateError::MutationRejected(format!(
                        "this node does not hold the operator key for .{}",
                        parsed.tld
                    )));
                }
                let is_update = matches!(req.action, MutationAction::UpdateDomainManifest { .. });
                let existing = zone.domains.get(&fqdn);
                if let Some(rec) = existing {
                    if rec.owner_public_key != actor {
                        return Err(FederateError::Unauthorized(format!(
                            "{fqdn} is owned by a different key"
                        )));
                    }
                    if !matches!(rec.status, DomainStatus::Active | DomainStatus::Pending) {
                        return Err(FederateError::MutationRejected(format!(
                            "{fqdn} status '{}' does not allow updates",
                            rec.status.as_str()
                        )));
                    }
                } else if is_update {
                    return Err(FederateError::DomainNotFound(fqdn.clone()));
                }
                // The manifest must already be in the content store (package
                // ingest puts it there first) and must be signed by the
                // actor for exactly this domain.
                let manifest_bytes =
                    self.manifests
                        .get(manifest_hash)
                        .ok_or(FederateError::ManifestNotFound(format!(
                            "{manifest_hash} (submit the site package first)"
                        )))?;
                let manifest: Manifest = serde_json::from_slice(manifest_bytes)?;
                manifest.validate()?;
                manifest.verify(&fqdn, actor)?;

                let mut record = DomainRecord {
                    domain: fqdn.clone(),
                    tld: parsed.tld.clone(),
                    label: parsed.name.clone(),
                    owner_public_key: actor.to_string(),
                    target_type: TargetType::Manifest,
                    manifest_hash: manifest_hash.clone(),
                    service_id: None,
                    node_id: None,
                    status: DomainStatus::Active,
                    created_at: existing
                        .map(|r| r.created_at.clone())
                        .unwrap_or_else(|| now.clone()),
                    updated_at: now.clone(),
                    expires_at: existing.and_then(|r| r.expires_at.clone()),
                    renewal: None,
                    pricing: None,
                    signature_algorithm: SIGNATURE_ALGORITHM.into(),
                    signature: None,
                };
                record.signature = Some(ctx.official_operator.sign(&record.signable_bytes()?));
                zone.domains.insert(fqdn, record);
                Ok(ActorRole::DomainOwner)
            }

            MutationAction::SetDomainStatus { domain, status } => {
                let fqdn = domain.to_ascii_lowercase();
                let rec = zone
                    .domains
                    .get(&fqdn)
                    .ok_or(FederateError::DomainNotFound(fqdn.clone()))?
                    .clone();
                let tld_rec = zone
                    .lookup_tld(&rec.tld)
                    .ok_or(FederateError::TldNotFound {
                        tld: rec.tld.clone(),
                    })?;
                let role = if is_root {
                    ActorRole::Root
                } else if actor == tld_rec.operator_public_key {
                    ActorRole::TldOperator
                } else {
                    return Err(FederateError::Unauthorized(format!(
                        "only the root key or the .{} operator can change {fqdn} status",
                        rec.tld
                    )));
                };
                if !allowed_domain_transition(rec.status, *status, is_root) {
                    return Err(FederateError::MutationRejected(format!(
                        "{fqdn}: transition {} -> {} is not allowed",
                        rec.status.as_str(),
                        status.as_str()
                    )));
                }
                if tld_rec.operator_public_key != ctx.official_operator.node_id() {
                    return Err(FederateError::MutationRejected(format!(
                        "this node does not hold the operator key for .{}",
                        rec.tld
                    )));
                }
                let mut updated = rec;
                updated.status = *status;
                updated.updated_at = now;
                updated.signature = None;
                updated.signature = Some(ctx.official_operator.sign(&updated.signable_bytes()?));
                zone.domains.insert(fqdn, updated);
                Ok(role)
            }

            MutationAction::IssueDomain { record } => {
                let record = record.as_ref();
                let tld_rec = zone
                    .lookup_tld(&record.tld)
                    .ok_or(FederateError::TldNotFound {
                        tld: record.tld.clone(),
                    })?;
                if tld_rec.registry_type != RegistryType::RootManaged {
                    return Err(FederateError::MutationRejected(format!(
                        ".{} is delegated; its registry is operator-published, not root-zone",
                        record.tld
                    )));
                }
                if actor != tld_rec.operator_public_key {
                    return Err(FederateError::Unauthorized(format!(
                        "actor is not the operator of .{}",
                        record.tld
                    )));
                }
                federate_naming::validate_label(&record.label)
                    .map_err(|e| FederateError::MutationRejected(format!("invalid label: {e}")))?;
                record.verify(actor)?;
                if record.is_expired() {
                    return Err(FederateError::MutationRejected(format!(
                        "{} record is already expired",
                        record.domain
                    )));
                }
                if let Some(existing) = zone.domains.get(&record.domain) {
                    if existing.status == DomainStatus::Revoked && !is_root {
                        return Err(FederateError::MutationRejected(format!(
                            "{} is revoked; only root can reissue it",
                            record.domain
                        )));
                    }
                }
                zone.domains.insert(record.domain.clone(), record.clone());
                Ok(ActorRole::TldOperator)
            }

            MutationAction::DelegateTld {
                tld,
                owner_public_key,
                operator_public_key,
                operator_name,
                registry_type,
                registry_endpoint,
                expires_at,
            } => {
                if !is_root {
                    return Err(FederateError::Unauthorized(
                        "only the Federate Root Key can delegate TLDs".into(),
                    ));
                }
                let name = ctx.blocklists.validate_new_tld(tld, false)?;
                if zone.tlds.contains_key(&name) {
                    return Err(FederateError::MutationRejected(format!(
                        ".{name} already exists; use tld.update"
                    )));
                }
                if *registry_type == RegistryType::RootManaged {
                    return Err(FederateError::MutationRejected(
                        "a delegated TLD needs a delegated registry type".into(),
                    ));
                }
                for (which, key) in [
                    ("owner", owner_public_key),
                    ("operator", operator_public_key),
                ] {
                    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(FederateError::MutationRejected(format!(
                            "{which} key must be a 64-char hex public key"
                        )));
                    }
                }
                let mut rec = TldRecord {
                    tld: name.clone(),
                    status: TldStatus::Delegated,
                    mode: federate_naming::TldMode::Delegated,
                    owner_public_key: owner_public_key.clone(),
                    operator_public_key: operator_public_key.clone(),
                    operator_name: operator_name.clone(),
                    registry_type: *registry_type,
                    registry_endpoint: registry_endpoint.clone(),
                    registry_manifest_hash: None,
                    registry_providers: Vec::new(),
                    policy_hash: None,
                    pricing: None,
                    created_at: now.clone(),
                    updated_at: now,
                    expires_at: expires_at.clone(),
                    notes: None,
                    signature_algorithm: SIGNATURE_ALGORITHM.into(),
                    signature: None,
                };
                rec.signature = Some(ctx.root.sign(&rec.signable_bytes()?));
                zone.tlds.insert(name, rec);
                Ok(ActorRole::Root)
            }

            MutationAction::CreateTld { tld, purpose } => {
                if !is_root {
                    return Err(FederateError::Unauthorized(
                        "only the Federate Root Key can create official TLDs".into(),
                    ));
                }
                // Official TLDs may use reserved names (e.g. .fed) but never
                // public IANA / policy-blocked names.
                let name = ctx.blocklists.validate_new_tld(tld, true)?;
                if zone.tlds.contains_key(&name) {
                    return Err(FederateError::MutationRejected(format!(
                        ".{name} already exists in the registry"
                    )));
                }
                let mut rec = TldRecord {
                    tld: name.clone(),
                    status: TldStatus::Official,
                    mode: federate_naming::TldMode::Official,
                    owner_public_key: ctx.root.node_id(),
                    operator_public_key: ctx.official_operator.node_id(),
                    operator_name: "Federate Network (root-managed)".into(),
                    registry_type: RegistryType::RootManaged,
                    registry_endpoint: None,
                    registry_manifest_hash: None,
                    registry_providers: Vec::new(),
                    policy_hash: None,
                    pricing: None,
                    created_at: now.clone(),
                    updated_at: now,
                    expires_at: None,
                    notes: Some(purpose.clone()),
                    signature_algorithm: SIGNATURE_ALGORITHM.into(),
                    signature: None,
                };
                rec.signature = Some(ctx.root.sign(&rec.signable_bytes()?));
                zone.tlds.insert(name, rec);
                Ok(ActorRole::Root)
            }

            MutationAction::ReserveTld { tld, reason }
            | MutationAction::BlockTld { tld, reason } => {
                if !is_root {
                    return Err(FederateError::Unauthorized(
                        "only the Federate Root Key can reserve or block TLDs".into(),
                    ));
                }
                // Only naming rules here: reserving/blocking adds a
                // restriction record, it never creates a resolvable TLD.
                let name = federate_naming::validate_tld_name(tld)?;
                if zone.tlds.contains_key(&name) {
                    return Err(FederateError::MutationRejected(format!(
                        ".{name} already exists in the registry; use tld.set_status"
                    )));
                }
                let (status, mode) = if matches!(req.action, MutationAction::ReserveTld { .. }) {
                    (TldStatus::Reserved, federate_naming::TldMode::Reserved)
                } else {
                    (TldStatus::Blocked, federate_naming::TldMode::Blocked)
                };
                let mut rec = TldRecord {
                    tld: name.clone(),
                    status,
                    mode,
                    owner_public_key: ctx.root.node_id(),
                    operator_public_key: ctx.root.node_id(),
                    operator_name: "Federate Network (root)".into(),
                    registry_type: RegistryType::RootManaged,
                    registry_endpoint: None,
                    registry_manifest_hash: None,
                    registry_providers: Vec::new(),
                    policy_hash: None,
                    pricing: None,
                    created_at: now.clone(),
                    updated_at: now,
                    expires_at: None,
                    notes: Some(reason.clone()),
                    signature_algorithm: SIGNATURE_ALGORITHM.into(),
                    signature: None,
                };
                rec.signature = Some(ctx.root.sign(&rec.signable_bytes()?));
                zone.tlds.insert(name, rec);
                Ok(ActorRole::Root)
            }

            MutationAction::UpdateTld {
                tld,
                registry_endpoint,
                expires_at,
                notes,
            } => {
                if !is_root {
                    return Err(FederateError::Unauthorized(
                        "only the Federate Root Key can update TLD records".into(),
                    ));
                }
                let name = tld.to_ascii_lowercase();
                let mut rec = zone
                    .tlds
                    .get(&name)
                    .ok_or(FederateError::TldNotFound { tld: name.clone() })?
                    .clone();
                if let Some(endpoint) = registry_endpoint {
                    rec.registry_endpoint = Some(endpoint.clone());
                }
                if let Some(expiry) = expires_at {
                    rec.expires_at = Some(expiry.clone());
                }
                if let Some(n) = notes {
                    rec.notes = Some(n.clone());
                }
                rec.updated_at = now;
                rec.signature = None;
                rec.signature = Some(ctx.root.sign(&rec.signable_bytes()?));
                zone.tlds.insert(name, rec);
                Ok(ActorRole::Root)
            }

            MutationAction::SetTldStatus { tld, status } => {
                if !is_root {
                    return Err(FederateError::Unauthorized(
                        "only the Federate Root Key can change TLD status".into(),
                    ));
                }
                let name = tld.to_ascii_lowercase();
                let mut rec = zone
                    .tlds
                    .get(&name)
                    .ok_or(FederateError::TldNotFound { tld: name.clone() })?
                    .clone();
                if rec.status == *status {
                    return Err(FederateError::MutationRejected(format!(
                        ".{name} already has status '{}'",
                        status.as_str()
                    )));
                }
                rec.status = *status;
                rec.updated_at = now;
                rec.signature = None;
                rec.signature = Some(ctx.root.sign(&rec.signable_bytes()?));
                zone.tlds.insert(name, rec);
                Ok(ActorRole::Root)
            }

            MutationAction::UpdateRegistryPointer { tld, registry_hex } => {
                let name = tld.to_ascii_lowercase();
                let mut rec = zone
                    .tlds
                    .get(&name)
                    .ok_or(FederateError::TldNotFound { tld: name.clone() })?
                    .clone();
                if actor != rec.operator_public_key {
                    return Err(FederateError::Unauthorized(format!(
                        "actor is not the operator of .{name}"
                    )));
                }
                if rec.registry_type != RegistryType::DelegatedManifest {
                    return Err(FederateError::MutationRejected(format!(
                        ".{name} registry type is {:?}; only delegated_manifest pins a hash \
                         through the root",
                        rec.registry_type
                    )));
                }
                let bytes = hex::decode(registry_hex).map_err(|_| {
                    FederateError::MutationRejected("registry_hex is not hex".into())
                })?;
                let registry: TldRegistry = serde_json::from_slice(&bytes)?;
                registry.verify(&name, actor)?;
                let current_version = self
                    .registries
                    .get(&name)
                    .map(|(_, r)| r.version)
                    .unwrap_or(0);
                if registry.version <= current_version {
                    return Err(FederateError::Replay(format!(
                        ".{name} registry v{} does not advance v{current_version}",
                        registry.version
                    )));
                }
                rec.registry_manifest_hash = Some(federate_storage::hash_bytes(&bytes));
                rec.updated_at = now;
                rec.signature = None;
                rec.signature = Some(ctx.root.sign(&rec.signable_bytes()?));
                zone.tlds.insert(name.clone(), rec);
                *new_registry = Some((name, bytes, registry));
                Ok(ActorRole::TldOperator)
            }
        }
    }

    // ------------------------------------------------------------------
    // persistence plumbing
    // ------------------------------------------------------------------

    fn write_manifest(&mut self, hash: &str, bytes: &[u8]) -> Result<()> {
        let manifest_dir = self.dir.join("manifests");
        std::fs::create_dir_all(&manifest_dir)?;
        atomic_write(&manifest_dir.join(hash), bytes)?;
        self.manifests.insert(hash.to_string(), bytes.to_vec());
        Ok(())
    }

    fn persist_state(&self) -> Result<()> {
        let state = PersistedState {
            zone: self.zone.clone(),
            registries: self
                .registries
                .iter()
                .map(|(tld, (bytes, _))| (tld.clone(), hex::encode(bytes)))
                .collect(),
            target_versions: self.target_versions.clone(),
        };
        atomic_write(
            &self.dir.join("state.json"),
            &serde_json::to_vec_pretty(&state)?,
        )
    }

    /// Write the current signed zone as an immutable snapshot file.
    pub fn write_snapshot(&self) -> Result<PathBuf> {
        let dir = self.dir.join("snapshots");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("root-zone-v{}.json", self.zone.root_version));
        atomic_write(&path, &serde_json::to_vec_pretty(&self.zone)?)?;
        Ok(path)
    }
}

/// Status transition matrix for root-managed domains. Same-status writes
/// are rejected upstream.
fn allowed_domain_transition(from: DomainStatus, to: DomainStatus, actor_is_root: bool) -> bool {
    use DomainStatus::*;
    if from == to {
        return false;
    }
    match to {
        Suspended => matches!(from, Active),
        Active => matches!(from, Suspended | Pending) || (actor_is_root && from == Revoked),
        Revoked => true,
        _ => false,
    }
}

/// BLAKE3 of the canonical bytes of the signed zone: what audit events
/// record as previous/new state hash.
fn state_hash(zone: &RootZone) -> Result<String> {
    Ok(federate_storage::hash_bytes(
        &federate_core::canonical::canonical_bytes(zone)?,
    ))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn append_line<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    use std::io::Write;
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(&line)?;
    Ok(())
}

fn read_lines(path: &Path) -> Result<Vec<String>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    Ok(std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect())
}
