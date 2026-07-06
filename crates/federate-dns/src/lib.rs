//! federate-dns — authoritative Federate DNS server.
//!
//! Behavior:
//! - loads the signed root zone (verified against the pinned root key) via
//!   the shared resolution engine — DNS never trusts unverified root data
//! - answers names under valid Federate TLDs (`.fed`, `.pagina`, `.rosa`,
//!   `.cara`, `.mosca`, `.busca`, and anything else in the signed root zone)
//!   with the IPs of *multiple healthy gateway nodes* from the node directory
//! - never returns one hardcoded IP; answers use a low TTL (30s)
//! - forwards every other name to upstream DNS (1.1.1.1 / 8.8.8.8) so normal
//!   internet resolution is never broken
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

pub struct DnsServer {
    /// Shared, signature-verifying resolution engine (root zone source).
    pub resolver: Arc<Resolver>,
    pub directory: DirectoryClient,
    pub upstream: SocketAddr,
    pub ttl: u32,
    /// Healthy gateway IPs, refreshed in the background from the directory.
    gateways: RwLock<Vec<IpAddr>>,
}

impl DnsServer {
    pub fn new(
        resolver: Arc<Resolver>,
        directory: DirectoryClient,
        upstream: SocketAddr,
    ) -> Arc<Self> {
        Arc::new(Self {
            resolver,
            directory,
            upstream,
            ttl: DEFAULT_TTL_SECS,
            gateways: RwLock::new(Vec::new()),
        })
    }

    /// Current healthy gateway IPs (may be empty right after startup).
    pub async fn gateway_ips(&self) -> Vec<IpAddr> {
        self.gateways.read().await.clone()
    }

    /// Refresh healthy gateways from the node directory (best first — the
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
                .map(|rec| rec.status.is_resolvable())
                .unwrap_or(false),
            Err(e) => {
                tracing::error!("no verified root zone available: {e}");
                false
            }
        }
    }

    /// Run the UDP DNS server. Also spawns root-zone + gateway refresh loops.
    pub async fn run(self: Arc<Self>, listen: SocketAddr) -> federate_core::Result<()> {
        let socket = Arc::new(UdpSocket::bind(listen).await?);
        tracing::info!(
            "federate DNS listening on udp://{listen} (upstream {})",
            self.upstream
        );

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

        let mut buf = [0u8; 1500];
        loop {
            let (len, peer) = socket.recv_from(&mut buf).await?;
            let packet = buf[..len].to_vec();
            let server = self.clone();
            let socket = socket.clone();
            tokio::spawn(async move {
                if let Some(reply) = server.handle_packet(&packet).await {
                    socket.send_to(&reply, peer).await.ok();
                }
            });
        }
    }

    /// Handle one raw DNS packet; returns the reply packet.
    pub async fn handle_packet(&self, packet: &[u8]) -> Option<Vec<u8>> {
        let query = DnsQuery::parse(packet).ok()?;

        if self.is_federate_name(&query.name).await {
            // Answer with multiple healthy gateway IPs, low TTL.
            let ips = self.gateway_ips().await;
            let (v4, v6): (Vec<IpAddr>, Vec<IpAddr>) = ips.into_iter().partition(|ip| ip.is_ipv4());
            let answers = match query.qtype {
                QueryType::A => v4,
                QueryType::Aaaa => v6,
                QueryType::Other(_) => vec![],
            };
            if answers.is_empty() && matches!(query.qtype, QueryType::A) {
                tracing::warn!("no healthy gateways to answer {} — SERVFAIL", query.name);
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

    /// Forward a query to the configured upstream resolver. The socket is
    /// `connect`ed to upstream so the kernel drops datagrams from any other
    /// source, and we additionally require the reply's transaction ID to match
    /// the query — together these block off-path answer spoofing.
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
