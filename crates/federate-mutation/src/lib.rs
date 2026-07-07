//! federate-mutation: runtime-mutable Federate Root Registry.
//!
//! This crate turns the registry from a startup-time artifact into durable,
//! runtime-mutable signed state:
//!
//! - [`MutationRequest`]: the signed envelope every state change travels in
//!   (nonce, timestamp, self-certifying id, per-target monotonic version);
//! - [`NonceStore`]: server-issued single-use challenges (anti-replay);
//! - [`AuditRecord`]: one signed audit event per accepted mutation;
//! - [`RegistryStore`]: the persistent registry itself (signed root zone,
//!   delegated registries, content stores, audit log, mutation history,
//!   root zone snapshots), plus the only apply path that mutates it.
//!
//! Seed data initializes the store on FIRST boot only; after that the
//! persistent registry is the source of truth and every change arrives as a
//! signed mutation. Private keys are never part of any persisted record.

mod audit;
mod nonce;
mod request;
mod seed;
mod store;

pub use audit::AuditRecord;
pub use nonce::NonceStore;
pub use request::{
    ActorRole, ContentBlock, MutationAction, MutationRequest, PackageBlock, SitePackage,
    TargetKind, MAX_PACKAGE_BLOCKS, MAX_PACKAGE_BYTES, MUTATION_MAX_AGE_SECS, NONCE_TTL_SECS,
    SIGNATURE_ALGORITHM,
};
pub use seed::{apply_seed, init_empty_registry, SeedFile, SeedOutcome, SeedTld};
pub use store::{AppliedMutation, MutationContext, RegistryStore};

#[cfg(test)]
mod tests;
