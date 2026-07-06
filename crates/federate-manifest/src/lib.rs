//! federate-manifest — manifest types, validation, signatures, path mapping.
//!
//! A manifest is signed by the domain owner key named in the domain record.
//! The manifest bytes themselves are content-addressed (the domain record's
//! `manifest_hash` covers the full signed JSON).

use federate_core::{FederateError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every Federate site has a manifest: a signed description of its content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub domain: String,
    pub version: u64,
    /// Entry file served for "/" (e.g. "index.html").
    pub entry: String,
    /// file path (no leading slash) -> content block hash
    pub files: BTreeMap<String, String>,
    /// The domain owner key that signed this manifest. Must match the
    /// `owner_public_key` in the domain record.
    pub owner_public_key: String,
    pub created_at: String,
    pub signature_algorithm: String,
    /// Ed25519 signature (hex) by the domain owner key over canonical bytes.
    #[serde(default)]
    pub signature: Option<String>,
}

impl Manifest {
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    /// Verify this manifest against the expected domain and the owner key
    /// authorized in the domain record.
    pub fn verify(&self, expected_domain: &str, owner_public_key: &str) -> Result<()> {
        let fail = |reason: &str| {
            Err(FederateError::VerificationFailed {
                layer: "manifest".into(),
                subject: self.domain.clone(),
                reason: reason.to_string(),
            })
        };
        if self.domain != expected_domain {
            return fail("manifest domain does not match the requested domain");
        }
        if self.owner_public_key != owner_public_key {
            return fail("manifest signer is not the domain owner key from the domain record");
        }
        let Some(sig) = &self.signature else {
            return fail("manifest is unsigned");
        };
        if !federate_identity::verify_signature(owner_public_key, &self.signable_bytes()?, sig) {
            return fail("manifest signature invalid (tampered or wrong key)");
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.domain.is_empty() || self.entry.is_empty() {
            return Err(FederateError::InvalidRoot(format!(
                "invalid manifest for {}",
                self.domain
            )));
        }
        if !self.files.contains_key(&self.entry) {
            return Err(FederateError::PathNotFound(self.entry.clone()));
        }
        Ok(())
    }

    /// Map an HTTP path to a content hash. "/" maps to the entry file;
    /// "/foo/" tries "foo/index.html".
    pub fn resolve_path(&self, path: &str) -> Option<&str> {
        let trimmed = path.trim_start_matches('/');
        let candidates = if trimmed.is_empty() {
            vec![self.entry.clone()]
        } else if trimmed.ends_with('/') {
            vec![format!("{trimmed}index.html")]
        } else {
            vec![trimmed.to_string(), format!("{trimmed}/index.html")]
        };
        candidates
            .iter()
            .find_map(|c| self.files.get(c))
            .map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_sign_verify_and_tamper() {
        let dir = std::env::temp_dir().join(format!("fed-manifest-test-{}", std::process::id()));
        let owner = federate_identity::NodeIdentity::load_or_create(&dir.join("owner")).unwrap();
        let attacker =
            federate_identity::NodeIdentity::load_or_create(&dir.join("attacker")).unwrap();
        let mut m = Manifest {
            domain: "home.fed".into(),
            version: 1,
            entry: "index.html".into(),
            files: BTreeMap::from([("index.html".to_string(), "hash".to_string())]),
            owner_public_key: owner.node_id(),
            created_at: "t".into(),
            signature_algorithm: "ed25519".into(),
            signature: None,
        };
        m.signature = Some(owner.sign(&m.signable_bytes().unwrap()));
        assert!(m.verify("home.fed", &owner.node_id()).is_ok());
        // wrong requested domain
        assert!(m.verify("docs.fed", &owner.node_id()).is_err());
        // signed by wrong owner
        let mut forged = m.clone();
        forged.owner_public_key = attacker.node_id();
        forged.signature = Some(attacker.sign(&forged.signable_bytes().unwrap()));
        assert!(forged.verify("home.fed", &owner.node_id()).is_err());
        // tampered content mapping
        let mut bad = m.clone();
        bad.files.insert("index.html".into(), "evil".into());
        assert!(bad.verify("home.fed", &owner.node_id()).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
