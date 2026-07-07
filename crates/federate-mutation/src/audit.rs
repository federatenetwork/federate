//! Signed audit events: every accepted mutation produces exactly one,
//! appended to the node's append-only audit log and embedded (tail only)
//! in the root zone.

use federate_core::{FederateError, Result};
use federate_identity::NodeIdentity;
use serde::{Deserialize, Serialize};

/// One immutable audit event, signed by the Federate Root Key so the log
/// itself is tamper-evident.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    /// BLAKE3 of the canonical event with `event_id` and `signature` blanked.
    pub event_id: String,
    pub mutation_id: String,
    pub actor_public_key: String,
    /// "root" | "tld-operator" | "domain-owner"
    pub actor_role: String,
    /// Stable action name, e.g. "domain.publish", "tld.delegate".
    pub action: String,
    /// "tld" | "domain"
    pub target_type: String,
    pub target_id: String,
    /// BLAKE3 of the canonical signed root zone before the mutation.
    pub previous_state_hash: String,
    /// BLAKE3 of the canonical signed root zone after the mutation.
    pub new_state_hash: String,
    pub timestamp: String,
    pub signature_algorithm: String,
    #[serde(default)]
    pub signature: Option<String>,
}

impl AuditRecord {
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    fn compute_id(&self) -> Result<String> {
        let mut blank = self.clone();
        blank.event_id = String::new();
        blank.signature = None;
        Ok(federate_storage::hash_bytes(
            &federate_core::canonical::canonical_bytes(&blank)?,
        ))
    }

    /// Finalize an event: compute its id and sign it with the root key.
    pub fn finalize(mut self, root: &NodeIdentity) -> Result<Self> {
        self.event_id = self.compute_id()?;
        self.signature = Some(root.sign(&self.signable_bytes()?));
        Ok(self)
    }

    /// Verify id integrity and the root signature.
    pub fn verify(&self, root_public_key: &str) -> Result<()> {
        if self.event_id != self.compute_id()? {
            return Err(FederateError::VerificationFailed {
                layer: "audit".into(),
                subject: self.event_id.clone(),
                reason: "event_id does not match event content".into(),
            });
        }
        let Some(sig) = &self.signature else {
            return Err(FederateError::VerificationFailed {
                layer: "audit".into(),
                subject: self.event_id.clone(),
                reason: "audit event is not signed".into(),
            });
        };
        if !federate_identity::verify_signature(root_public_key, &self.signable_bytes()?, sig) {
            return Err(FederateError::VerificationFailed {
                layer: "audit".into(),
                subject: self.event_id.clone(),
                reason: "audit signature is not from the root key".into(),
            });
        }
        Ok(())
    }
}
