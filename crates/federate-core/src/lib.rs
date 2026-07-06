//! federate-core: shared types, errors, config, constants for Federate Network.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Network name used in the root zone.
pub const NETWORK_NAME: &str = "federate";

/// Public bootstrap / control-plane node (Node 1).
pub const DEFAULT_BOOTSTRAP_URL: &str = "https://federate.network";

/// Development bootstrap (local federate-server).
pub const DEV_BOOTSTRAP_URL: &str = "http://127.0.0.1:9000";

/// Primary browser-facing gateway address. Portless URLs require port 80.
pub const DEFAULT_GATEWAY_ADDR: &str = "127.0.0.1:80";

/// Optional development fallback gateway port.
pub const DEV_GATEWAY_ADDR: &str = "127.0.0.1:8787";

/// Local daemon API for CLI / desktop app integration.
pub const DEFAULT_API_ADDR: &str = "127.0.0.1:7777";

/// Default listen address for Node 1 server in development.
pub const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:9000";

#[derive(Debug, thiserror::Error)]
pub enum FederateError {
    #[error("not a federate domain: {0}")]
    NotFederateDomain(String),
    #[error("unknown TLD: {0}")]
    UnknownTld(String),
    #[error("domain not found in root zone: {0}")]
    DomainNotFound(String),
    #[error("path not found in manifest: {0}")]
    PathNotFound(String),
    #[error("manifest not found: {0}")]
    ManifestNotFound(String),
    #[error("block not found: {0}")]
    BlockNotFound(String),
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("root zone unavailable (network down and no local cache)")]
    RootUnavailable,
    #[error("invalid root zone: {0}")]
    InvalidRoot(String),
    #[error("invalid TLD name '{name}': {reason}")]
    InvalidTldName { name: String, reason: String },
    #[error("TLD '{tld}' is blocked: {reason}")]
    BlockedTld { tld: String, reason: String },
    #[error("TLD '{tld}' is reserved: {reason}")]
    ReservedTld { tld: String, reason: String },
    #[error("TLD '{tld}' not found in Federate root registry")]
    TldNotFound { tld: String },
    #[error("TLD '{tld}' is not resolvable (status: {status})")]
    TldUnavailable { tld: String, status: String },
    #[error("TLD '{tld}' is delegated but delegated registry resolution is not implemented yet")]
    DelegatedRegistryNotImplemented { tld: String },
    #[error("verification failed at {layer} layer for '{subject}': {reason}")]
    VerificationFailed {
        layer: String,
        subject: String,
        reason: String,
    },
    #[error("invalid signature")]
    InvalidSignature,
    #[error("network error: {0}")]
    Network(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, FederateError>;

/// Configuration for the local daemon (`federated`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Node 1 bootstrap URL, e.g. https://federate.network
    pub bootstrap_url: String,
    /// Browser-facing gateway bind address (must be 127.0.0.1:80 for portless URLs).
    pub gateway_addr: SocketAddr,
    /// Local daemon API bind address.
    pub api_addr: SocketAddr,
    /// Local cache directory (root zone, manifests, blocks, identity).
    pub data_dir: PathBuf,
}

impl DaemonConfig {
    pub fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("federate")
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bootstrap_url: DEFAULT_BOOTSTRAP_URL.to_string(),
            gateway_addr: DEFAULT_GATEWAY_ADDR.parse().unwrap(),
            api_addr: DEFAULT_API_ADDR.parse().unwrap(),
            data_dir: Self::default_data_dir(),
        }
    }
}

/// Canonical serialization for signed payloads.
///
/// Signatures must never depend on JSON whitespace or field order. Canonical
/// form is compact JSON with object keys sorted lexicographically at every
/// nesting level (arrays keep their order). Signed structs set their
/// `signature` field to None before canonicalization. See docs/signatures.md.
pub mod canonical {
    use serde::Serialize;

    pub fn canonical_bytes<T: Serialize>(value: &T) -> super::Result<Vec<u8>> {
        let v = serde_json::to_value(value)?;
        let mut out = Vec::new();
        write_canonical(&v, &mut out);
        Ok(out)
    }

    fn write_canonical(v: &serde_json::Value, out: &mut Vec<u8>) {
        match v {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                out.push(b'{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    out.extend(serde_json::to_vec(k).unwrap());
                    out.push(b':');
                    write_canonical(&map[k.as_str()], out);
                }
                out.push(b'}');
            }
            serde_json::Value::Array(items) => {
                out.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    write_canonical(item, out);
                }
                out.push(b']');
            }
            other => out.extend(serde_json::to_vec(other).unwrap()),
        }
    }

    #[cfg(test)]
    mod tests {
        #[derive(serde::Serialize)]
        struct B {
            z: u32,
            a: u32,
        }

        #[test]
        fn keys_sorted_and_compact() {
            let bytes = super::canonical_bytes(&B { z: 1, a: 2 }).unwrap();
            assert_eq!(bytes, br#"{"a":2,"z":1}"#);
        }
    }
}
