//! Server-issued single-use nonces: the challenge half of challenge-response
//! replay protection. A mutation must carry a nonce this node issued, still
//! inside its TTL, and never seen in an accepted mutation before.

use crate::request::NONCE_TTL_SECS;
use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory nonce issuance and single-use consumption. Nonces are cheap and
/// short-lived, so losing them on restart is safe: clients just request a
/// new challenge.
pub struct NonceStore {
    ttl_secs: i64,
    /// nonce -> unix expiry
    inner: Mutex<HashMap<String, i64>>,
}

impl Default for NonceStore {
    fn default() -> Self {
        Self::new(NONCE_TTL_SECS)
    }
}

impl NonceStore {
    pub fn new(ttl_secs: i64) -> Self {
        Self {
            ttl_secs,
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Issue a fresh random nonce; returns (nonce, unix expiry).
    pub fn issue(&self, now: i64) -> (String, i64) {
        let nonce = hex::encode(rand::random::<[u8; 32]>());
        let expires = now + self.ttl_secs;
        let mut inner = self.inner.lock().expect("nonce store poisoned");
        inner.retain(|_, exp| *exp > now); // prune expired while we hold the lock
        inner.insert(nonce.clone(), expires);
        (nonce, expires)
    }

    /// Consume a nonce: true exactly once, and only inside its TTL.
    pub fn consume(&self, nonce: &str, now: i64) -> bool {
        let mut inner = self.inner.lock().expect("nonce store poisoned");
        match inner.remove(nonce) {
            Some(expires) => expires > now,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_single_use() {
        let store = NonceStore::new(300);
        let (nonce, expires) = store.issue(1_000);
        assert!(expires > 1_000);
        assert!(store.consume(&nonce, 1_010), "first use accepted");
        assert!(!store.consume(&nonce, 1_020), "reuse rejected");
    }

    #[test]
    fn nonce_expires() {
        let store = NonceStore::new(300);
        let (nonce, _) = store.issue(1_000);
        assert!(!store.consume(&nonce, 2_000), "expired nonce rejected");
    }

    #[test]
    fn unknown_nonce_rejected() {
        let store = NonceStore::new(300);
        assert!(!store.consume("deadbeef", 1_000));
    }
}
