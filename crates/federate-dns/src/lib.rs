//! federate-dns: authoritative Federate DNS server.
//!
//! Behavior:
//! - loads the signed root zone (verified against the pinned root key) via
//!   the shared resolution engine; DNS never trusts unverified root data
//! - answers names under valid Federate TLDs (`.fed`, `.pagina`, `.rosa`,
//!   `.cara`, `.mosca`, `.busca`, and anything else in the signed root zone)
//!   with the IPs of *multiple healthy gateway nodes* from the node directory
//! - never returns one hardcoded IP; answers use a low TTL (30s)
//! - forwards every other name to upstream DNS (1.1.1.1 / 8.8.8.8) so normal
//!   internet resolution is never broken
//! - listens on UDP **and** TCP (RFC 7766 length-prefixed framing); stub
//!   resolvers that retry over TCP get the same answers
//!
//! DNS answers *where a name should go* (gateways). Gateways then do the full
//! root → TLD → domain → manifest → block verification chain.

pub mod wire;

use federate_directory::{DirectoryClient, NodeRole};
use federate_resolution::Resolver;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use wire::{DnsQuery, QueryType};

pub const DEFAULT_TTL_SECS: u32 = 30;
pub const DEFAULT_UPSTREAM: &str = "1.1.1.1:53";
/// Max A/AAAA records per answer. Keeps plain-UDP responses safely under the
/// classic 512-byte limit (no EDNS yet), so no client is ever forced onto
/// the TCP retry path; 8 gateways is plenty for failover.
pub const MAX_ANSWERS: usize = 8;
/// Max DNS packets handled concurrently. Each forwarded query holds a UDP
/// socket for up to 3s; without a bound, a packet flood turns into unbounded
/// task and file-descriptor growth. Excess packets wait in the OS buffer
/// (and are dropped there under real overload, which is the correct UDP
/// behavior).
pub const MAX_INFLIGHT_QUERIES: usize = 512;
/// Max concurrent TCP DNS connections (separate pool from UDP so a TCP
/// connection flood cannot starve UDP service, and vice versa).
pub const MAX_TCP_CONNECTIONS: usize = 128;
/// A TCP connection is dropped after this long without a complete query.
pub const TCP_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Largest TCP DNS message we accept. Queries are tiny; this is just a
/// sanity bound so a client cannot make us allocate 64KB per connection.
pub const MAX_TCP_MESSAGE: usize = 4096;

pub struct DnsServer {
    /// Shared, signature-verifying resolution engine (root zone source).
    pub resolver: Arc<Resolver>,
    pub directory: DirectoryClient,
    pub upstream: SocketAddr,
    pub ttl: u32,
    /// Healthy gateway IPs, refreshed in the background from the directory.
    gateways: RwLock<Vec<IpAddr>>,
    /// When set, Federate names are answered with exactly these IPs
    /// instead of directory gateways. Used by the local resolver service,
    /// which serves the content itself on loopback (with local TLS); which
    /// names are Federate still comes from the signed root zone.
    fixed_answers: Vec<IpAddr>,
}

impl DnsServer {
    pub fn new(
        resolver: Arc<Resolver>,
        directory: DirectoryClient,
        upstream: SocketAddr,
    ) -> Arc<Self> {
        Self::with_fixed_answers(resolver, directory, upstream, Vec::new())
    }

    /// A server that answers Federate names with `fixed` (e.g. 127.0.0.1
    /// for a loopback gateway) instead of directory gateway IPs.
    pub fn with_fixed_answers(
        resolver: Arc<Resolver>,
        directory: DirectoryClient,
        upstream: SocketAddr,
        fixed: Vec<IpAddr>,
    ) -> Arc<Self> {
        Arc::new(Self {
            resolver,
            directory,
            upstream,
            ttl: DEFAULT_TTL_SECS,
            gateways: RwLock::new(Vec::new()),
            fixed_answers: fixed,
        })
    }

    /// Current healthy gateway IPs (may be empty right after startup).
    pub async fn gateway_ips(&self) -> Vec<IpAddr> {
        self.gateways.read().await.clone()
    }

    /// Refresh healthy gateways from the node directory (best first; the
    /// directory ranks by health then latency).
    pub async fn refresh_gateways(&self) {
        match self.directory.list(Some(NodeRole::Gateway), true).await {
            Ok(nodes) => {
                let ips: Vec<IpAddr> = nodes.iter().flat_map(|n| n.registration.ips()).collect();
                if ips.is_empty() {
                    tracing::warn!("directory returned no healthy gateway IPs");
                }
                *self.gateways.write().await = ips;
            }
            Err(e) => tracing::warn!("gateway refresh from directory failed: {e}"),
        }
    }

    /// Is this name under a resolvable TLD in the *verified* root zone?
    pub async fn is_federate_name(&self, name: &str) -> bool {
        let tld = match name.trim_end_matches('.').rsplit('.').next() {
            Some(t) if !t.is_empty() => t.to_ascii_lowercase(),
            _ => return false,
        };
        match self.resolver.root().await {
            Ok(zone) => zone
                .lookup_tld(&tld)
                .map(|rec| rec.status.is_resolvable() && !rec.is_expired())
                .unwrap_or(false),
            Err(e) => {
                tracing::error!("no verified root zone available: {e}");
                false
            }
        }
    }

    /// Run the DNS server on UDP and TCP at `listen`. Also spawns root-zone
    /// + gateway refresh loops.
    pub async fn run(self: Arc<Self>, listen: SocketAddr) -> federate_core::Result<()> {
        let socket = Arc::new(UdpSocket::bind(listen).await?);
        let tcp = tokio::net::TcpListener::bind(listen).await?;
        tracing::info!(
            "federate DNS listening on udp://{listen} + tcp://{listen} (upstream {})",
            self.upstream
        );
        {
            let server = self.clone();
            tokio::spawn(async move { server.run_tcp(tcp).await });
        }

        // Background: keep gateways fresh (DNS TTL is 30s; refresh faster).
        {
            let server = self.clone();
            tokio::spawn(async move {
                loop {
                    server.refresh_gateways().await;
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            });
        }
        // Background: keep the verified root zone fresh.
        {
            let server = self.clone();
            tokio::spawn(async move {
                loop {
                    if let Err(e) = server.resolver.refresh_root().await {
                        tracing::warn!("root zone refresh failed: {e}");
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            });
        }

        let inflight = Arc::new(tokio::sync::Semaphore::new(MAX_INFLIGHT_QUERIES));
        let mut buf = [0u8; 1500];
        loop {
            let (len, peer) = socket.recv_from(&mut buf).await?;
            // Backpressure: stop reading new packets while MAX_INFLIGHT_QUERIES
            // handlers are already running. Semaphore is never closed, so
            // acquire cannot fail.
            let permit = inflight.clone().acquire_owned().await.expect("semaphore");
            let packet = buf[..len].to_vec();
            let server = self.clone();
            let socket = socket.clone();
            tokio::spawn(async move {
                if let Some(reply) = server.handle_packet(&packet).await {
                    socket.send_to(&reply, peer).await.ok();
                }
                drop(permit);
            });
        }
    }

    /// Accept loop for TCP DNS (RFC 7766: each message is prefixed with a
    /// two-byte big-endian length). Same query handling as UDP.
    async fn run_tcp(self: Arc<Self>, listener: tokio::net::TcpListener) {
        let conns = Arc::new(tokio::sync::Semaphore::new(MAX_TCP_CONNECTIONS));
        loop {
            let Ok((stream, peer)) = listener.accept().await else {
                continue;
            };
            // Backpressure: stop accepting while MAX_TCP_CONNECTIONS handlers
            // are running. Semaphore is never closed, so acquire cannot fail.
            let permit = conns.clone().acquire_owned().await.expect("semaphore");
            let server = self.clone();
            tokio::spawn(async move {
                if let Err(e) = server.serve_tcp_conn(stream).await {
                    tracing::debug!("tcp dns connection from {peer} ended: {e}");
                }
                drop(permit);
            });
        }
    }

    /// Serve length-prefixed DNS queries on one TCP connection until the
    /// client closes, sends garbage, or goes idle.
    async fn serve_tcp_conn(&self, mut stream: tokio::net::TcpStream) -> std::io::Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        loop {
            let mut len_buf = [0u8; 2];
            match tokio::time::timeout(TCP_IDLE_TIMEOUT, stream.read_exact(&mut len_buf)).await {
                Ok(Ok(_)) => {}
                // Idle timeout or clean close: just drop the connection.
                _ => return Ok(()),
            }
            let len = u16::from_be_bytes(len_buf) as usize;
            if len == 0 || len > MAX_TCP_MESSAGE {
                return Ok(());
            }
            let mut packet = vec![0u8; len];
            match tokio::time::timeout(TCP_IDLE_TIMEOUT, stream.read_exact(&mut packet)).await {
                Ok(Ok(_)) => {}
                _ => return Ok(()),
            }
            let Some(reply) = self.handle_packet(&packet).await else {
                return Ok(());
            };
            if reply.len() > u16::MAX as usize {
                return Ok(());
            }
            stream
                .write_all(&(reply.len() as u16).to_be_bytes())
                .await?;
            stream.write_all(&reply).await?;
        }
    }

    /// Handle one raw DNS packet; returns the reply packet.
    pub async fn handle_packet(&self, packet: &[u8]) -> Option<Vec<u8>> {
        let query = DnsQuery::parse(packet).ok()?;

        if self.is_federate_name(&query.name).await {
            // Answer with multiple healthy gateway IPs, low TTL (or the
            // fixed override for loopback-gateway deployments).
            let ips = if self.fixed_answers.is_empty() {
                self.gateway_ips().await
            } else {
                self.fixed_answers.clone()
            };
            let (v4, v6): (Vec<IpAddr>, Vec<IpAddr>) = ips.into_iter().partition(|ip| ip.is_ipv4());
            let mut answers = match query.qtype {
                QueryType::A => v4,
                QueryType::Aaaa => v6,
                QueryType::Other(_) => vec![],
            };
            // Cap answers so the response always fits a plain 512-byte UDP reply.
            answers.truncate(MAX_ANSWERS);
            if answers.is_empty() && matches!(query.qtype, QueryType::A) {
                tracing::warn!(
                    "no healthy gateways to answer {}; answering SERVFAIL",
                    query.name
                );
                return Some(wire::build_servfail(packet, &query));
            }
            tracing::debug!(
                "{} -> {} gateway record(s), ttl {}",
                query.name,
                answers.len(),
                self.ttl
            );
            return Some(wire::build_response(packet, &query, &answers, self.ttl));
        }

        // Not a Federate name: forward to upstream DNS.
        match self.forward_upstream(packet, query.id).await {
            Some(reply) => Some(reply),
            None => Some(wire::build_servfail(packet, &query)),
        }
    }

    /// Forward a query to the configured upstream resolver over UDP (TCP
    /// clients get their answer re-framed; a truncated upstream answer keeps
    /// its TC bit, so real resolvers know to ask upstream directly). The
    /// socket is `connect`ed to upstream so the kernel drops datagrams from
    /// any other source, and we additionally require the reply's transaction
    /// ID to match the query; together these block off-path answer spoofing.
    async fn forward_upstream(&self, packet: &[u8], query_id: u16) -> Option<Vec<u8>> {
        let bind = if self.upstream.is_ipv6() {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        let socket = UdpSocket::bind(bind).await.ok()?;
        socket.connect(self.upstream).await.ok()?;
        socket.send(packet).await.ok()?;
        let mut buf = [0u8; 4096];
        let deadline = std::time::Duration::from_secs(3);
        let started = std::time::Instant::now();
        // Ignore stray datagrams with the wrong ID until the deadline.
        loop {
            let remaining = deadline.checked_sub(started.elapsed())?;
            let len = tokio::time::timeout(remaining, socket.recv(&mut buf))
                .await
                .ok()?
                .ok()?;
            if len >= 2 && u16::from_be_bytes([buf[0], buf[1]]) == query_id {
                return Some(buf[..len].to_vec());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use federate_client::NodeClient;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// End-to-end TCP framing: a non-Federate query arrives length-prefixed
    /// over TCP, is forwarded to (a fake) upstream over UDP, and the answer
    /// comes back length-prefixed on the same connection.
    #[tokio::test]
    async fn tcp_query_roundtrip_via_upstream() {
        // Fake upstream: echoes whatever it receives (same transaction ID,
        // so the forwarder accepts it as the answer).
        let upstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (len, peer) = upstream.recv_from(&mut buf).await.unwrap();
            upstream.send_to(&buf[..len], peer).await.unwrap();
        });

        // Server with an unreachable bootstrap: no verified root zone, so
        // every name is non-Federate and gets forwarded.
        let dir = std::env::temp_dir().join(format!("fed-dns-tcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let resolver = Arc::new(
            federate_resolution::Resolver::new(NodeClient::new("http://127.0.0.1:1"), &dir, None)
                .unwrap(),
        );
        let server = DnsServer::new(
            resolver,
            DirectoryClient::new("http://127.0.0.1:1"),
            upstream_addr,
        );

        // One-connection TCP listener wired straight to serve_tcp_conn.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let conn_server = server.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            conn_server.serve_tcp_conn(stream).await.ok();
        });

        let query = wire::build_query(0x4242, "example.com", 1);
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(&(query.len() as u16).to_be_bytes())
            .await
            .unwrap();
        stream.write_all(&query).await.unwrap();

        let mut len_buf = [0u8; 2];
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stream.read_exact(&mut len_buf),
        )
        .await
        .expect("reply within 5s")
        .unwrap();
        let len = u16::from_be_bytes(len_buf) as usize;
        let mut reply = vec![0u8; len];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, query, "echoed upstream answer relayed over TCP");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Garbage TCP framing must close the connection, never hang or panic.
    #[tokio::test]
    async fn tcp_zero_length_frame_closes_connection() {
        let dir = std::env::temp_dir().join(format!("fed-dns-tcp0-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let resolver = Arc::new(
            federate_resolution::Resolver::new(NodeClient::new("http://127.0.0.1:1"), &dir, None)
                .unwrap(),
        );
        let server = DnsServer::new(
            resolver,
            DirectoryClient::new("http://127.0.0.1:1"),
            "127.0.0.1:1".parse().unwrap(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let conn_server = server.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            conn_server.serve_tcp_conn(stream).await.ok();
        });
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(&0u16.to_be_bytes()).await.unwrap();
        // Server must close; read returns 0 bytes (EOF) instead of hanging.
        let mut buf = [0u8; 2];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut buf))
            .await
            .expect("close within 5s")
            .unwrap();
        assert_eq!(n, 0, "connection closed on zero-length frame");
        std::fs::remove_dir_all(&dir).ok();
    }
}
