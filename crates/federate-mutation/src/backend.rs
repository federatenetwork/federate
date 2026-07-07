//! Storage abstraction for the persistent root registry.
//!
//! The domain layer (`RegistryStore`) never touches the database engine
//! directly: it talks to this trait. The production implementation is
//! [`crate::RedbRegistryStore`] (embedded redb database); the old JSON file
//! layout survives only as a read-only migration source
//! (`crate::legacy_json`).
//!
//! Content blobs (site blocks, manifest bytes) stay in content-addressed
//! file stores; private keys stay in their own 0600 `identity.key` files.
//! Neither ever enters the database.

use crate::audit::AuditRecord;
use crate::store::AppliedMutation;
use federate_core::Result;
use federate_naming::DomainRecord;
use federate_root::TldRecord;
use serde::{Deserialize, Serialize};

/// Metadata of one root zone snapshot (the signed zone bytes themselves
/// live in the `root_zone_versions` table; a human-inspectable JSON copy is
/// also written under `snapshots/`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub root_version: u64,
    pub created_at: String,
    /// BLAKE3 of the canonical signed zone at this version.
    pub state_hash: String,
}

/// Everything one accepted mutation changes, committed in a SINGLE
/// database transaction: either all of it lands or none of it does, and a
/// crash mid-mutation leaves the previous state intact.
pub struct CommitBatch {
    /// Exact JSON bytes of the new signed root zone.
    pub zone_json: Vec<u8>,
    pub root_version: u64,
    /// BLAKE3 of the canonical signed zone (consistency check on load).
    pub state_hash: String,
    /// Full record sets of the new zone (the tables mirror the zone).
    pub tlds: Vec<TldRecord>,
    pub domains: Vec<DomainRecord>,
    pub target_key: String,
    pub target_version: u64,
    pub applied: AppliedMutation,
    pub audit: AuditRecord,
    /// New delegated registry bytes when the mutation re-pinned one.
    pub delegated_registry: Option<(String, Vec<u8>)>,
    pub snapshot: SnapshotMeta,
}

/// First-initialization payload (empty or seeded-by-migration zone).
pub struct InitialState {
    pub zone_json: Vec<u8>,
    pub root_version: u64,
    pub state_hash: String,
    pub tlds: Vec<TldRecord>,
    pub domains: Vec<DomainRecord>,
    pub delegated_registries: Vec<(String, Vec<u8>)>,
    pub snapshot: SnapshotMeta,
}

/// The registry storage backend: durable, transactional record store.
pub trait RegistryBackend: Send + Sync {
    // --- TLD records ---
    fn get_tld(&self, tld: &str) -> Result<Option<TldRecord>>;
    fn put_tld(&self, record: &TldRecord) -> Result<()>;
    fn list_tlds(&self) -> Result<Vec<TldRecord>>;

    // --- domain records ---
    fn get_domain(&self, fqdn: &str) -> Result<Option<DomainRecord>>;
    fn put_domain(&self, record: &DomainRecord) -> Result<()>;
    fn list_domains(&self) -> Result<Vec<DomainRecord>>;

    // --- signed root zone versions ---
    fn get_root_zone_version(&self, version: u64) -> Result<Option<Vec<u8>>>;
    fn put_root_zone_version(&self, version: u64, zone_json: &[u8]) -> Result<()>;
    /// (version, zone bytes) of the current zone, from registry metadata.
    fn current_root_zone(&self) -> Result<Option<(u64, Vec<u8>)>>;

    // --- mutation history ---
    fn append_mutation(&self, applied: &AppliedMutation) -> Result<()>;
    fn get_mutation(&self, mutation_id: &str) -> Result<Option<AppliedMutation>>;
    fn list_mutations(&self) -> Result<Vec<AppliedMutation>>;

    // --- audit log ---
    fn append_audit_event(&self, event: &AuditRecord) -> Result<()>;
    fn list_audit_events(&self) -> Result<Vec<AuditRecord>>;

    // --- nonces (challenge-response replay protection) ---
    /// Store an issued nonce with its expiry (unix seconds).
    fn reserve_nonce(&self, nonce: &str, expires_at: i64) -> Result<()>;
    /// Single use: true exactly once, and only before expiry. Expired
    /// entries are pruned opportunistically.
    fn consume_nonce(&self, nonce: &str, now: i64) -> Result<bool>;

    // --- registry metadata ---
    fn get_meta(&self, key: &str) -> Result<Option<String>>;
    fn put_meta(&self, key: &str, value: &str) -> Result<()>;

    // --- per-target monotonic mutation versions ---
    fn get_target_version(&self, target_key: &str) -> Result<u64>;
    fn put_target_version(&self, target_key: &str, version: u64) -> Result<()>;
    fn list_target_versions(&self) -> Result<Vec<(String, u64)>>;

    // --- delegated registry pointers (exact signed bytes) ---
    fn get_delegated_registry(&self, tld: &str) -> Result<Option<Vec<u8>>>;
    fn list_delegated_registries(&self) -> Result<Vec<(String, Vec<u8>)>>;

    // --- snapshots metadata ---
    fn create_snapshot(&self, meta: &SnapshotMeta) -> Result<()>;
    fn list_snapshots(&self) -> Result<Vec<SnapshotMeta>>;

    // --- transactional entry points ---
    /// Adopt the initial state (first init / migration) atomically.
    fn commit_initial(&self, state: &InitialState) -> Result<()>;
    /// Apply one accepted mutation atomically (all tables in one
    /// transaction; crash safety comes from the database engine).
    fn commit_mutation(&self, batch: &CommitBatch) -> Result<()>;

    /// Table counts and file size for `federate registry db stats`.
    fn stats(&self) -> Result<serde_json::Value>;
}
