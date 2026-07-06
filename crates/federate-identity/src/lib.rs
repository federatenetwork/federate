//! federate-identity — local node identity, keys, signatures.

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use federate_core::Result;
use std::path::{Path, PathBuf};

/// A local Ed25519 node identity. Generated on first run, stored on disk.
pub struct NodeIdentity {
    signing_key: SigningKey,
    path: PathBuf,
}

impl NodeIdentity {
    /// Load the identity from `<data_dir>/identity.key`, generating one on
    /// first run.
    pub fn load_or_create(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join("identity.key");
        let signing_key = if path.exists() {
            let bytes = std::fs::read(&path)?;
            let arr: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
                federate_core::FederateError::InvalidRoot("corrupt identity key".into())
            })?;
            SigningKey::from_bytes(&arr)
        } else {
            std::fs::create_dir_all(data_dir)?;
            let key = SigningKey::generate(&mut rand::rngs::OsRng);
            std::fs::write(&path, key.to_bytes())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            key
        };
        Ok(Self { signing_key, path })
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Public node ID: hex of the verifying key.
    pub fn node_id(&self) -> String {
        hex::encode(self.verifying_key().to_bytes())
    }

    pub fn sign(&self, message: &[u8]) -> String {
        hex::encode(self.signing_key.sign(message).to_bytes())
    }

    pub fn key_path(&self) -> &Path {
        &self.path
    }
}

/// Verify an Ed25519 signature. `public_key_hex` and `signature_hex` are hex
/// encodings of the 32-byte verifying key and 64-byte signature.
pub fn verify_signature(public_key_hex: &str, message: &[u8], signature_hex: &str) -> bool {
    let Ok(pk_bytes) = hex::decode(public_key_hex) else {
        return false;
    };
    let Ok(pk_arr) = <[u8; 32]>::try_from(pk_bytes.as_slice()) else {
        return false;
    };
    let Ok(vk) = VerifyingKey::from_bytes(&pk_arr) else {
        return false;
    };
    let Ok(sig_bytes) = hex::decode(signature_hex) else {
        return false;
    };
    let Ok(sig_arr) = <[u8; 64]>::try_from(sig_bytes.as_slice()) else {
        return false;
    };
    use ed25519_dalek::Verifier;
    vk.verify(message, &ed25519_dalek::Signature::from_bytes(&sig_arr))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() {
        let dir = std::env::temp_dir().join(format!("fed-id-test-{}", std::process::id()));
        let id = NodeIdentity::load_or_create(&dir).unwrap();
        let sig = id.sign(b"hello");
        assert!(verify_signature(&id.node_id(), b"hello", &sig));
        assert!(!verify_signature(&id.node_id(), b"tampered", &sig));
        std::fs::remove_dir_all(&dir).ok();
    }
}
