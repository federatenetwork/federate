//! Signed mutation requests: the only way registry state changes at runtime.
//!
//! Every mutation is a self-describing envelope signed by the actor key.
//! The envelope carries a server-issued nonce, a timestamp, and a monotonic
//! per-target version, so a captured request can never be replayed and a
//! stale request can never roll state back (see docs/en-US/mutations.md).

use federate_core::{FederateError, Result};
use federate_identity::NodeIdentity;
use federate_naming::{DomainRecord, DomainStatus, RegistryType, TldStatus};
use serde::{Deserialize, Serialize};

/// Signature scheme for mutation envelopes and audit events.
pub const SIGNATURE_ALGORITHM: &str = federate_root::SIGNATURE_ALGORITHM;

/// A signed mutation older than this is rejected even with a valid nonce.
pub const MUTATION_MAX_AGE_SECS: i64 = 300;

/// Issued nonces expire after this window.
pub const NONCE_TTL_SECS: i64 = 300;

/// Hard cap on a submitted site package (manifest + all blocks, decoded).
pub const MAX_PACKAGE_BYTES: usize = 32 * 1024 * 1024;

/// Hard cap on the number of blocks in one site package.
pub const MAX_PACKAGE_BLOCKS: usize = 2048;

/// What a mutation targets; audit events record it as `target_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Tld,
    Domain,
}

impl TargetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TargetKind::Tld => "tld",
            TargetKind::Domain => "domain",
        }
    }
}

/// Who was allowed to sign a mutation; audit events record it as `actor_role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorRole {
    Root,
    TldOperator,
    DomainOwner,
}

impl ActorRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorRole::Root => "root",
            ActorRole::TldOperator => "tld-operator",
            ActorRole::DomainOwner => "domain-owner",
        }
    }
}

/// The registry state changes a mutation can request.
///
/// Authorization is enforced against the CURRENT signed state, not against
/// anything the request claims:
/// - the Federate Root Key delegates TLDs, updates TLD records/statuses, and
///   may perform emergency domain enforcement;
/// - a TLD operator key issues/updates/suspends domains inside its own TLD
///   and moves its delegated registry pointer;
/// - a domain owner key publishes/updates the manifest of its own domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MutationAction {
    /// Publish (create or update) an official-TLD domain from a site
    /// package; the manifest must already be in the node's content store
    /// (package ingest stores it before applying the mutation).
    PublishSite {
        domain: String,
        manifest_hash: String,
    },
    /// Point an existing domain record at a new owner-signed manifest.
    UpdateDomainManifest {
        domain: String,
        manifest_hash: String,
    },
    /// Suspend / reinstate / revoke a root-managed domain.
    SetDomainStatus {
        domain: String,
        status: DomainStatus,
    },
    /// A TLD operator issues (or replaces) a full domain record inside its
    /// own root-managed TLD. The record itself must be operator-signed.
    IssueDomain { record: Box<DomainRecord> },
    /// Root delegates a new TLD to an operator.
    DelegateTld {
        tld: String,
        owner_public_key: String,
        operator_public_key: String,
        operator_name: String,
        registry_type: RegistryType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        registry_endpoint: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
    },
    /// Root updates mutable metadata of an existing TLD record
    /// (set-if-present semantics; omitted fields keep their value).
    UpdateTld {
        tld: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        registry_endpoint: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    },
    /// Root changes a TLD status (suspend a delegation, reinstate it, ...).
    SetTldStatus { tld: String, status: TldStatus },
    /// A delegated operator publishes a new signed registry; the root-signed
    /// TLD record is re-pinned to its content address. `registry_hex` is the
    /// exact signed registry JSON, hex-encoded to survive JSON transport.
    UpdateRegistryPointer { tld: String, registry_hex: String },
}

impl MutationAction {
    /// Stable action name recorded in audit events.
    pub fn name(&self) -> &'static str {
        match self {
            MutationAction::PublishSite { .. } => "domain.publish",
            MutationAction::UpdateDomainManifest { .. } => "domain.update_manifest",
            MutationAction::SetDomainStatus { .. } => "domain.set_status",
            MutationAction::IssueDomain { .. } => "domain.issue",
            MutationAction::DelegateTld { .. } => "tld.delegate",
            MutationAction::UpdateTld { .. } => "tld.update",
            MutationAction::SetTldStatus { .. } => "tld.set_status",
            MutationAction::UpdateRegistryPointer { .. } => "tld.update_registry_pointer",
        }
    }

    /// The (kind, id) this mutation targets; per-target versions are
    /// monotonic over this pair.
    pub fn target(&self) -> (TargetKind, String) {
        match self {
            MutationAction::PublishSite { domain, .. }
            | MutationAction::UpdateDomainManifest { domain, .. }
            | MutationAction::SetDomainStatus { domain, .. } => {
                (TargetKind::Domain, domain.to_ascii_lowercase())
            }
            MutationAction::IssueDomain { record } => {
                (TargetKind::Domain, record.domain.to_ascii_lowercase())
            }
            MutationAction::DelegateTld { tld, .. }
            | MutationAction::UpdateTld { tld, .. }
            | MutationAction::SetTldStatus { tld, .. }
            | MutationAction::UpdateRegistryPointer { tld, .. } => {
                (TargetKind::Tld, tld.to_ascii_lowercase())
            }
        }
    }

    /// Key of a target in the per-target version map ("domain:eu.pagina").
    pub fn target_key(&self) -> String {
        let (kind, id) = self.target();
        format!("{}:{}", kind.as_str(), id)
    }
}

/// The signed envelope every mutation travels in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationRequest {
    /// BLAKE3 of the canonical envelope with `mutation_id` and `signature`
    /// blanked; self-certifying, so replays are detectable forever.
    pub mutation_id: String,
    /// Server-issued single-use nonce (see `NonceStore`).
    pub nonce: String,
    /// RFC 3339 timestamp; requests older than `MUTATION_MAX_AGE_SECS` fail.
    pub issued_at: String,
    /// Hex Ed25519 public key of the signer.
    pub actor_public_key: String,
    /// Must be strictly greater than the target's last accepted version.
    pub target_version: u64,
    pub action: MutationAction,
    pub signature_algorithm: String,
    #[serde(default)]
    pub signature: Option<String>,
}

impl MutationRequest {
    /// Build and sign an envelope in one step.
    pub fn signed(
        identity: &NodeIdentity,
        nonce: &str,
        issued_at: &str,
        target_version: u64,
        action: MutationAction,
    ) -> Result<Self> {
        let mut req = MutationRequest {
            mutation_id: String::new(),
            nonce: nonce.to_string(),
            issued_at: issued_at.to_string(),
            actor_public_key: identity.node_id(),
            target_version,
            action,
            signature_algorithm: SIGNATURE_ALGORITHM.to_string(),
            signature: None,
        };
        req.mutation_id = req.compute_id()?;
        req.signature = Some(identity.sign(&req.signable_bytes()?));
        Ok(req)
    }

    /// Canonical bytes with signature blanked (mutation_id included, so the
    /// signature also covers the id).
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    /// The self-certifying id: BLAKE3 of the canonical envelope with both
    /// `mutation_id` and `signature` blanked.
    pub fn compute_id(&self) -> Result<String> {
        let mut blank = self.clone();
        blank.mutation_id = String::new();
        blank.signature = None;
        Ok(federate_storage::hash_bytes(
            &federate_core::canonical::canonical_bytes(&blank)?,
        ))
    }

    /// Envelope self-verification: id matches content, signature present and
    /// valid for the claimed actor key. Authorization comes later, against
    /// current registry state.
    pub fn verify(&self) -> Result<()> {
        if self.signature_algorithm != SIGNATURE_ALGORITHM {
            return Err(FederateError::MutationRejected(format!(
                "unsupported signature algorithm '{}'",
                self.signature_algorithm
            )));
        }
        if self.mutation_id != self.compute_id()? {
            return Err(FederateError::MutationRejected(
                "mutation_id does not match envelope content".into(),
            ));
        }
        let Some(sig) = &self.signature else {
            return Err(FederateError::MutationRejected(
                "mutation is not signed".into(),
            ));
        };
        if !federate_identity::verify_signature(
            &self.actor_public_key,
            &self.signable_bytes()?,
            sig,
        ) {
            return Err(FederateError::InvalidSignature);
        }
        Ok(())
    }

    /// Fail-closed timestamp window check (unparseable counts as too old).
    pub fn check_age(&self, now: chrono::DateTime<chrono::Utc>) -> Result<()> {
        let issued = chrono::DateTime::parse_from_rfc3339(&self.issued_at)
            .map_err(|_| FederateError::Replay("unparseable mutation timestamp".into()))?;
        let age = now.timestamp() - issued.timestamp();
        if !(-30..=MUTATION_MAX_AGE_SECS).contains(&age) {
            return Err(FederateError::Replay(format!(
                "mutation timestamp outside the {MUTATION_MAX_AGE_SECS}s acceptance window"
            )));
        }
        Ok(())
    }
}

/// One content block of a site package (`data_hex` = raw bytes hex-encoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageBlock {
    pub hash: String,
    pub data_hex: String,
}

/// Everything a publisher submits to put a site under an official TLD:
/// content blocks, the exact owner-signed manifest bytes, and the signed
/// `PublishSite` mutation that authorizes the domain record update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SitePackage {
    pub mutation: MutationRequest,
    /// Exact manifest JSON bytes, hex-encoded (re-serializing would change
    /// the content address).
    pub manifest_hex: String,
    pub blocks: Vec<PackageBlock>,
}

/// A decoded content block: (content address, raw bytes).
pub type ContentBlock = (String, Vec<u8>);

impl SitePackage {
    /// Structural verification before any state is touched: caps, hex
    /// decoding, block hashes, manifest hash, and that the mutation is a
    /// `PublishSite` for the manifest actually shipped.
    /// Returns (exact manifest bytes, decoded blocks).
    pub fn decode(&self) -> Result<(Vec<u8>, Vec<ContentBlock>)> {
        if self.blocks.len() > MAX_PACKAGE_BLOCKS {
            return Err(FederateError::MutationRejected(format!(
                "package has {} blocks (max {MAX_PACKAGE_BLOCKS})",
                self.blocks.len()
            )));
        }
        let manifest_bytes = hex::decode(&self.manifest_hex)
            .map_err(|_| FederateError::MutationRejected("manifest_hex is not hex".into()))?;
        let mut total = manifest_bytes.len();
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let bytes = hex::decode(&block.data_hex).map_err(|_| {
                FederateError::MutationRejected(format!("block {} is not hex", block.hash))
            })?;
            total += bytes.len();
            if total > MAX_PACKAGE_BYTES {
                return Err(FederateError::MutationRejected(format!(
                    "package exceeds {MAX_PACKAGE_BYTES} bytes"
                )));
            }
            federate_storage::verify(&bytes, &block.hash)?;
            blocks.push((block.hash.clone(), bytes));
        }
        let MutationAction::PublishSite { manifest_hash, .. } = &self.mutation.action else {
            return Err(FederateError::MutationRejected(
                "package mutation must be a publish_site action".into(),
            ));
        };
        federate_storage::verify(&manifest_bytes, manifest_hash)?;
        Ok((manifest_bytes, blocks))
    }
}
