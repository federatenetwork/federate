//! federate-protocol: the native Federate wire protocol.
//!
//! This is the protocol Federate nodes and native clients speak to each
//! other. HTTP endpoints (`/v1/root`, `/v1/block/:hash`, ...) are the
//! *compatibility* surface for browsers and plain tooling; this protocol is
//! the native surface the network is built around.
//!
//! v0 design decisions, chosen to be replaceable without breaking peers:
//! - messages are JSON values framed by a 4-byte big-endian length prefix
//!   (simple to debug, trivially portable; a binary encoding can become
//!   version 1 through the same negotiation)
//! - transport-agnostic: framing lives here, sockets live in
//!   `federate-transport` (framed TCP today, QUIC later, same messages)
//! - every session starts with `Hello`/`Welcome` version negotiation and
//!   node identity exchange; a peer that cannot agree on a version answers
//!   `Error { code: Unsupported }` and closes
//! - trust never comes from the transport: root zones, records, manifests,
//!   and blocks are verified by the *receiver* against signatures/hashes,
//!   exactly like the HTTP path. The protocol moves bytes, signatures decide.
//!
//! Request/response pairs (v0):
//!   Hello        -> Welcome            handshake + version/capabilities
//!   GetRoot      -> Root               signed root zone (JSON bytes)
//!   GetManifest  -> Manifest           content-addressed manifest bytes
//!   GetBlock     -> Block              content-addressed block bytes
//!   GetProviders -> Providers          who serves a block (directory-backed)
//!   GetStatus    -> Status             role/health/version info
//!   anything     -> Error              structured failure

use federate_core::{FederateError, Result};
use serde::{Deserialize, Serialize};

/// Protocol versions this implementation can speak, newest first.
pub const SUPPORTED_VERSIONS: &[u16] = &[0];

/// Hard cap on one framed message. Blocks are capped at 64 MiB by
/// `federate-client`; the frame cap leaves room for envelope overhead.
pub const MAX_FRAME_BYTES: u32 = 68 * 1024 * 1024;

/// Default TCP port for the native protocol: 0xFED.
pub const DEFAULT_NATIVE_PORT: u16 = 4077;

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// What a node can do for you. Sent in the handshake so peers know which
/// requests are worth making.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Root,
    Manifests,
    Blocks,
    Providers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Message {
    /// Client -> node. Opens every session.
    Hello {
        /// Versions the sender speaks, newest first.
        versions: Vec<u16>,
        /// Sender's public key (hex). Identity, not authority: what a node
        /// may *claim* is still bounded by signatures on the data itself.
        node_id: String,
        /// Software version string (diagnostic only).
        agent: String,
    },
    /// Node -> client. Accepts the handshake.
    Welcome {
        /// The version this session will use (highest shared).
        version: u16,
        node_id: String,
        agent: String,
        capabilities: Vec<Capability>,
    },
    GetRoot,
    /// Signed root zone as canonical JSON bytes. The receiver MUST verify
    /// the zone signature against its pinned root key before use.
    Root {
        zone_json: Vec<u8>,
    },
    GetManifest {
        hash: String,
    },
    /// Raw manifest bytes; receiver MUST verify they hash to the requested
    /// content address.
    Manifest {
        hash: String,
        bytes: Vec<u8>,
    },
    GetBlock {
        hash: String,
    },
    /// Raw block bytes; receiver MUST verify they hash to the requested
    /// content address.
    Block {
        hash: String,
        bytes: Vec<u8>,
    },
    GetProviders {
        hash: String,
    },
    /// Node entries (directory JSON) announcing the block. Advisory: every
    /// fetched block is still hash-verified.
    Providers {
        hash: String,
        nodes_json: Vec<u8>,
    },
    GetStatus,
    Status {
        node_id: String,
        roles: Vec<String>,
        region: String,
        agent: String,
        root_version: Option<u64>,
    },
    Error {
        code: ErrorCode,
        detail: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// No shared protocol version, or an unknown message type.
    Unsupported,
    /// Requested content address does not exist here.
    NotFound,
    /// Request was malformed (bad hash, bad field).
    BadRequest,
    /// The node cannot serve this right now (no verified root zone yet, ...).
    Unavailable,
}

/// Highest version both sides speak, if any.
pub fn negotiate(ours: &[u16], theirs: &[u16]) -> Option<u16> {
    ours.iter().copied().filter(|v| theirs.contains(v)).max()
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// Encode a message as one frame: 4-byte big-endian length + JSON body.
pub fn encode(msg: &Message) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(msg)?;
    if body.len() as u64 > MAX_FRAME_BYTES as u64 {
        return Err(FederateError::Network(format!(
            "message exceeds frame cap ({} > {MAX_FRAME_BYTES} bytes)",
            body.len()
        )));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    Ok(out)
}

/// Validate a frame length prefix before allocating for the body.
pub fn frame_len(prefix: [u8; 4]) -> Result<usize> {
    let len = u32::from_be_bytes(prefix);
    if len == 0 || len > MAX_FRAME_BYTES {
        return Err(FederateError::Network(format!(
            "invalid frame length {len} (cap {MAX_FRAME_BYTES})"
        )));
    }
    Ok(len as usize)
}

/// Decode one frame body.
pub fn decode(body: &[u8]) -> Result<Message> {
    Ok(serde_json::from_slice(body)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip_all_shapes() {
        let msgs = vec![
            Message::Hello {
                versions: SUPPORTED_VERSIONS.to_vec(),
                node_id: "ab".repeat(32),
                agent: "federate/0.1.0".into(),
            },
            Message::Welcome {
                version: 0,
                node_id: "cd".repeat(32),
                agent: "federate-noded/0.1.0".into(),
                capabilities: vec![Capability::Root, Capability::Blocks],
            },
            Message::GetBlock {
                hash: "0".repeat(64),
            },
            Message::Block {
                hash: "0".repeat(64),
                bytes: b"hello".to_vec(),
            },
            Message::Error {
                code: ErrorCode::NotFound,
                detail: "no such block".into(),
            },
        ];
        for msg in msgs {
            let frame = encode(&msg).unwrap();
            let len = frame_len(frame[..4].try_into().unwrap()).unwrap();
            assert_eq!(len, frame.len() - 4);
            let back = decode(&frame[4..]).unwrap();
            assert_eq!(
                serde_json::to_string(&back).unwrap(),
                serde_json::to_string(&msg).unwrap()
            );
        }
    }

    #[test]
    fn frame_length_limits_enforced() {
        assert!(frame_len(0u32.to_be_bytes()).is_err());
        assert!(frame_len((MAX_FRAME_BYTES + 1).to_be_bytes()).is_err());
        assert!(frame_len(1024u32.to_be_bytes()).is_ok());
    }

    #[test]
    fn version_negotiation_picks_highest_shared() {
        assert_eq!(negotiate(&[0], &[0]), Some(0));
        assert_eq!(negotiate(&[2, 1, 0], &[1, 0]), Some(1));
        assert_eq!(negotiate(&[0], &[7]), None);
        assert_eq!(negotiate(&[], &[0]), None);
    }

    #[test]
    fn unknown_message_type_fails_decode_not_panic() {
        assert!(decode(br#"{"type":"quantum-entangle"}"#).is_err());
        assert!(decode(b"not json").is_err());
    }
}
