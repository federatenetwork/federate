//! federate-storage: content-addressed blocks, hashing, local cache.

use federate_core::{FederateError, Result};
use std::path::{Path, PathBuf};

/// BLAKE3 hex is 64 lowercase hex characters.
const HASH_HEX_LEN: usize = 64;

/// BLAKE3 hex hash of a byte slice. This is the content address of a block.
pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Is this string a syntactically valid content address (64 lowercase hex
/// chars)? Any hash that reaches the filesystem or a fetch URL comes from
/// untrusted input (HTTP path params, manifests) and MUST pass this first -
/// it blocks path traversal (`../`), absolute paths, and multi-byte slicing
/// panics before the value is ever used to build a path.
pub fn is_valid_hash(hash: &str) -> bool {
    hash.len() == HASH_HEX_LEN
        && hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Verify bytes against an expected hash. Rejects malformed hashes up front
/// so a caller can never be tricked into "verifying" against `../../etc`.
pub fn verify(bytes: &[u8], expected: &str) -> Result<()> {
    if !is_valid_hash(expected) {
        return Err(FederateError::HashMismatch {
            expected: expected.to_string(),
            actual: "<malformed expected hash>".to_string(),
        });
    }
    let actual = hash_bytes(bytes);
    if actual != expected {
        return Err(FederateError::HashMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

/// Filesystem-backed content-addressed block store.
/// Blocks live at `<dir>/<first-2-hex>/<hash>`.
#[derive(Debug, Clone)]
pub struct BlockStore {
    dir: PathBuf,
}

impl BlockStore {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("blocks");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Path for a block. Returns None for any hash that is not a valid content
    /// address, so a crafted hash can never escape the block directory.
    fn block_path(&self, hash: &str) -> Option<PathBuf> {
        if !is_valid_hash(hash) {
            return None;
        }
        Some(self.dir.join(&hash[..2]).join(hash))
    }

    pub fn has(&self, hash: &str) -> bool {
        self.block_path(hash).map(|p| p.exists()).unwrap_or(false)
    }

    /// Get a block and re-verify its hash (guards against disk corruption).
    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self
            .block_path(hash)
            .ok_or_else(|| FederateError::BlockNotFound(hash.to_string()))?;
        let bytes =
            std::fs::read(path).map_err(|_| FederateError::BlockNotFound(hash.to_string()))?;
        verify(&bytes, hash)?;
        Ok(bytes)
    }

    /// Store bytes, verifying they match the expected hash first.
    pub fn put(&self, hash: &str, bytes: &[u8]) -> Result<()> {
        verify(bytes, hash)?;
        // verify() already rejected malformed hashes, so this cannot be None.
        let path = self
            .block_path(hash)
            .ok_or_else(|| FederateError::BlockNotFound(hash.to_string()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Remove a single block (used by cache eviction).
    pub fn remove(&self, hash: &str) -> Result<()> {
        let path = self
            .block_path(hash)
            .ok_or_else(|| FederateError::BlockNotFound(hash.to_string()))?;
        std::fs::remove_file(path).map_err(|_| FederateError::BlockNotFound(hash.to_string()))
    }

    pub fn list(&self) -> Result<Vec<(String, u64)>> {
        let mut out = Vec::new();
        if !self.dir.exists() {
            return Ok(out);
        }
        for shard in std::fs::read_dir(&self.dir)? {
            let shard = shard?;
            if !shard.file_type()?.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(shard.path())? {
                let entry = entry?;
                out.push((
                    entry.file_name().to_string_lossy().to_string(),
                    entry.metadata()?.len(),
                ));
            }
        }
        Ok(out)
    }

    pub fn clear(&self) -> Result<usize> {
        let n = self.list()?.len();
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        std::fs::create_dir_all(&self.dir)?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tampered_block_rejected() {
        let dir = std::env::temp_dir().join(format!("fed-storage-test-{}", std::process::id()));
        let store = BlockStore::new(&dir).unwrap();
        let bytes = b"hello federate";
        let hash = hash_bytes(bytes);
        // put with wrong hash rejected
        assert!(store.put("00", bytes).is_err());
        store.put(&hash, bytes).unwrap();
        assert_eq!(store.get(&hash).unwrap(), bytes);
        // corrupt on disk -> cached read fails verification
        let path = dir.join("blocks").join(&hash[..2]).join(&hash);
        std::fs::write(&path, b"tampered").unwrap();
        assert!(matches!(
            store.get(&hash),
            Err(FederateError::HashMismatch { .. })
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_malformed_and_traversal_hashes() {
        let dir = std::env::temp_dir().join(format!("fed-storage-trav-{}", std::process::id()));
        let store = BlockStore::new(&dir).unwrap();
        for bad in [
            "../../etc/passwd",
            "..",
            "/etc/passwd",
            "abc",           // too short
            &"g".repeat(64), // non-hex
            &"A".repeat(64), // uppercase not allowed
            "é",             // multi-byte (would panic on hash[..2])
            &format!("{}/x", "0".repeat(63)),
        ] {
            assert!(!is_valid_hash(bad), "{bad} should be invalid");
            assert!(
                store.block_path(bad).is_none(),
                "{bad} must not map to a path"
            );
            assert!(matches!(
                store.get(bad),
                Err(FederateError::BlockNotFound(_))
            ));
            assert!(store.put(bad, b"x").is_err());
            assert!(!store.has(bad));
        }
        assert!(is_valid_hash(&hash_bytes(b"anything")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
