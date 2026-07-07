//! federate-mutation: runtime-mutable Federate Root Registry.
//!
//! This crate turns the registry from a startup-time artifact into durable,
//! runtime-mutable signed state:
//!
//! - [`MutationRequest`]: the signed envelope every state change travels in
//!   (nonce, timestamp, self-certifying id, per-target monotonic version);
//! - [`AuditRecord`]: one signed audit event per accepted mutation;
//! - [`RegistryStore`]: the persistent registry itself, on top of the
//!   [`RegistryBackend`] storage abstraction whose production
//!   implementation is [`RedbRegistryStore`], an embedded redb database
//!   (transactional, crash-safe, single file `registry.redb`);
//! - [`legacy_json`]: the retired JSON file layout, kept only as a
//!   read-only migration source for `federate registry
//!   migrate-json-to-redb`.
//!
//! The registry starts EMPTY and is populated only by explicit seed
//! commands and signed mutations. Nonces are persistent, so a consumed
//! challenge can never be replayed, not even across restarts. Private keys
//! are never part of any persisted record or database table.

mod audit;
mod backend;
pub mod legacy_json;
mod redb_backend;
mod request;
mod seed;
mod store;

pub use audit::AuditRecord;
pub use backend::{CommitBatch, InitialState, RegistryBackend, SnapshotMeta};
pub use redb_backend::{RedbRegistryStore, REGISTRY_DB_FILE};
pub use request::{
    ActorRole, ContentBlock, MutationAction, MutationRequest, PackageBlock, SitePackage,
    TargetKind, MAX_PACKAGE_BLOCKS, MAX_PACKAGE_BYTES, MUTATION_MAX_AGE_SECS, NONCE_TTL_SECS,
    SIGNATURE_ALGORITHM,
};
pub use seed::{apply_seed, init_empty_registry, SeedFile, SeedOutcome, SeedTld};
pub use store::{
    backup_registry, migrate_json_to_redb, restore_registry, AppliedMutation, MutationContext,
    RegistryStore,
};

#[cfg(test)]
mod tests;
