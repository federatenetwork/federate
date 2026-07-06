//! federate-transport: moving `federate-protocol` messages between peers.
//!
//! The protocol crate defines *what* is said; this crate defines *how it
//! travels*. Today that is length-prefixed frames over TCP (dual-stack,
//! IPv6-ready because tokio binds whatever address you give it). The API is
//! deliberately message-oriented, not stream-oriented, so a QUIC transport
//! can replace the socket underneath without touching protocol logic or
//! callers: everything above this crate only ever sees
//! `send(Message) / recv() -> Message`.
//!
//! Safety posture: every read is capped (frame length limit from
//! `federate-protocol`), every operation has a deadline, and the serve loop
//! bounds concurrent connections. Transport carries no trust: callers verify
//! signatures and hashes on what arrives, exactly as they do for HTTP.

use federate_core::{FederateError, Result};
use federate_protocol as proto;
use federate_protocol::Message;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

/// Per-operation deadline (connect, one send, one recv).
pub const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Max simultaneous native-protocol connections a node serves.
pub const MAX_CONNECTIONS: usize = 256;
/// Max requests served on one connection before it is closed (keeps a
/// misbehaving peer from squatting a connection slot forever).
pub const MAX_REQUESTS_PER_CONNECTION: usize = 10_000;

/// One message-oriented connection to a peer.
pub struct Connection {
    stream: TcpStream,
}

impl Connection {
    /// Dial a peer and complete the `Hello`/`Welcome` handshake. Returns the
    /// connection and the peer's `Welcome`.
    pub async fn connect(
        addr: impl ToSocketAddrs,
        identity: &federate_identity::NodeIdentity,
        agent: &str,
    ) -> Result<(Self, Message)> {
        let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .map_err(|_| FederateError::Network("connect timeout".into()))?
            .map_err(|e| FederateError::Network(format!("connect failed: {e}")))?;
        let mut conn = Self { stream };
        conn.send(&Message::Hello {
            versions: proto::SUPPORTED_VERSIONS.to_vec(),
            node_id: identity.node_id(),
            agent: agent.to_string(),
        })
        .await?;
        let welcome = conn.recv().await?;
        match &welcome {
            Message::Welcome { version, .. } => {
                if !proto::SUPPORTED_VERSIONS.contains(version) {
                    return Err(FederateError::Network(format!(
                        "peer chose unsupported protocol version {version}"
                    )));
                }
            }
            Message::Error { code, detail } => {
                return Err(FederateError::Network(format!(
                    "handshake rejected: {code:?}: {detail}"
                )));
            }
            other => {
                return Err(FederateError::Network(format!(
                    "expected Welcome, got {other:?}"
                )));
            }
        }
        Ok((conn, welcome))
    }

    pub async fn send(&mut self, msg: &Message) -> Result<()> {
        let frame = proto::encode(msg)?;
        tokio::time::timeout(IO_TIMEOUT, self.stream.write_all(&frame))
            .await
            .map_err(|_| FederateError::Network("send timeout".into()))?
            .map_err(|e| FederateError::Network(format!("send failed: {e}")))?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Message> {
        let mut prefix = [0u8; 4];
        tokio::time::timeout(IO_TIMEOUT, self.stream.read_exact(&mut prefix))
            .await
            .map_err(|_| FederateError::Network("recv timeout".into()))?
            .map_err(|e| FederateError::Network(format!("recv failed: {e}")))?;
        let len = proto::frame_len(prefix)?;
        let mut body = vec![0u8; len];
        tokio::time::timeout(IO_TIMEOUT, self.stream.read_exact(&mut body))
            .await
            .map_err(|_| FederateError::Network("recv timeout".into()))?
            .map_err(|e| FederateError::Network(format!("recv failed: {e}")))?;
        proto::decode(&body)
    }

    /// One request/response exchange.
    pub async fn request(&mut self, msg: &Message) -> Result<Message> {
        self.send(msg).await?;
        self.recv().await
    }
}

/// What a node answers native-protocol requests with. Implementations map
/// requests onto their local stores/resolvers; the serve loop handles
/// handshake, framing, limits, and errors.
#[async_trait::async_trait]
pub trait NodeService: Send + Sync + 'static {
    /// This node's identity (echoed in Welcome).
    fn node_id(&self) -> String;
    /// Capabilities advertised in Welcome.
    fn capabilities(&self) -> Vec<proto::Capability>;
    /// Answer one already-handshaken request. Return `Error` messages for
    /// failures; returning Err(..) closes the connection.
    async fn handle(&self, request: Message) -> Message;
}

// Re-exported so implementors write #[federate_transport::async_trait]
// without depending on the async-trait crate directly.
pub use async_trait::async_trait;

/// Serve the native protocol on `listener`. Each connection: handshake with
/// version negotiation, then a request loop until the peer closes, errors,
/// or hits the per-connection request cap.
pub async fn serve(listener: TcpListener, service: Arc<dyn NodeService>, agent: String) {
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            continue;
        };
        let permit = slots.clone().acquire_owned().await.expect("semaphore");
        let service = service.clone();
        let agent = agent.clone();
        tokio::spawn(async move {
            let mut conn = Connection { stream };
            if let Err(e) = serve_conn(&mut conn, service, &agent).await {
                tracing::debug!("native connection from {peer} ended: {e}");
            }
            drop(permit);
        });
    }
}

async fn serve_conn(
    conn: &mut Connection,
    service: Arc<dyn NodeService>,
    agent: &str,
) -> Result<()> {
    // Handshake: first message must be Hello with a shared version.
    let hello = conn.recv().await?;
    let Message::Hello { versions, .. } = hello else {
        conn.send(&Message::Error {
            code: proto::ErrorCode::BadRequest,
            detail: "session must start with Hello".into(),
        })
        .await?;
        return Ok(());
    };
    let Some(version) = proto::negotiate(proto::SUPPORTED_VERSIONS, &versions) else {
        conn.send(&Message::Error {
            code: proto::ErrorCode::Unsupported,
            detail: format!(
                "no shared protocol version (ours: {:?})",
                proto::SUPPORTED_VERSIONS
            ),
        })
        .await?;
        return Ok(());
    };
    conn.send(&Message::Welcome {
        version,
        node_id: service.node_id(),
        agent: agent.to_string(),
        capabilities: service.capabilities(),
    })
    .await?;

    for _ in 0..MAX_REQUESTS_PER_CONNECTION {
        let request = match conn.recv().await {
            Ok(m) => m,
            Err(_) => return Ok(()), // peer closed or timed out: normal end
        };
        let response = service.handle(request).await;
        conn.send(&response).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use federate_identity::NodeIdentity;

    struct EchoBlocks;

    #[async_trait]
    impl NodeService for EchoBlocks {
        fn node_id(&self) -> String {
            "ee".repeat(32)
        }
        fn capabilities(&self) -> Vec<proto::Capability> {
            vec![proto::Capability::Blocks]
        }
        async fn handle(&self, request: Message) -> Message {
            match request {
                Message::GetBlock { hash } => Message::Block {
                    hash,
                    bytes: b"block bytes".to_vec(),
                },
                _ => Message::Error {
                    code: proto::ErrorCode::Unsupported,
                    detail: "test service only serves blocks".into(),
                },
            }
        }
    }

    #[tokio::test]
    async fn handshake_and_request_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, Arc::new(EchoBlocks), "test-node/0".into()));

        let dir = std::env::temp_dir().join(format!("fed-transport-{}", std::process::id()));
        let id = NodeIdentity::load_or_create(&dir).unwrap();
        let (mut conn, welcome) = Connection::connect(addr, &id, "test-client/0")
            .await
            .unwrap();
        match welcome {
            Message::Welcome {
                version,
                capabilities,
                ..
            } => {
                assert_eq!(version, 0);
                assert_eq!(capabilities, vec![proto::Capability::Blocks]);
            }
            other => panic!("expected Welcome, got {other:?}"),
        }

        let resp = conn
            .request(&Message::GetBlock {
                hash: "0".repeat(64),
            })
            .await
            .unwrap();
        match resp {
            Message::Block { bytes, .. } => assert_eq!(bytes, b"block bytes"),
            other => panic!("expected Block, got {other:?}"),
        }

        // Unknown request type answered with a structured error, connection stays up.
        let resp = conn.request(&Message::GetRoot).await.unwrap();
        assert!(matches!(
            resp,
            Message::Error {
                code: proto::ErrorCode::Unsupported,
                ..
            }
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn version_mismatch_rejected() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, Arc::new(EchoBlocks), "test-node/0".into()));

        let mut conn = Connection {
            stream: TcpStream::connect(addr).await.unwrap(),
        };
        conn.send(&Message::Hello {
            versions: vec![9999],
            node_id: "aa".repeat(32),
            agent: "future-client".into(),
        })
        .await
        .unwrap();
        let resp = conn.recv().await.unwrap();
        assert!(matches!(
            resp,
            Message::Error {
                code: proto::ErrorCode::Unsupported,
                ..
            }
        ));
    }
}
