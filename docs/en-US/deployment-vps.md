# Deploying Node 1 on a VPS

> [Versão em português (pt-BR)](../pt-BR/deployment-vps.md)

This is the production runbook for the first real deployment: one VPS from
any provider (Hetzner, DigitalOcean, Vultr, OVH, AWS Lightsail, ...) running
Ubuntu or Debian with the whole stack behind the public domain
`federate.network`. The only provider requirements: a public IPv4, root SSH,
and no provider firewall blocking ports 53/80/443.

What runs on the box:

| Service | Binds | Purpose |
|---|---|---|
| `federate-server` | 127.0.0.1:9000 | Node 1: signed root zone, registry, node directory, bootstrap |
| Caddy | 0.0.0.0:80 + 443 | TLS for `https://federate.network`, routes every other Host on port 80 to the gateway |
| `federate-gatewayd` | 127.0.0.1:8080 (+ health 0.0.0.0:8081) | Serves Federate sites after full signature verification |
| `federate-dnsd` | 0.0.0.0:53 UDP + TCP (+ health 0.0.0.0:8053) | Answers Federate TLDs with healthy gateway IPs, forwards everything else upstream |

The DNS server speaks **UDP and TCP on port 53** (TCP uses RFC 7766
length-prefixed framing). Answers are capped at 8 records so plain-UDP
replies always fit 512 bytes; TCP exists for stub resolvers that insist on
it and for tooling like `dig +tcp`.

End-to-end flow this enables from any external device:

1. Device sets its DNS server to the VPS IP.
2. Browser opens `http://home.fed`.
3. `federate-dnsd` answers `home.fed` with the gateway IP (this VPS).
4. Browser sends `Host: home.fed` to port 80; Caddy hands it to
   `federate-gatewayd`.
5. The gateway verifies root zone signature, TLD record, domain record,
   manifest signature, and block hashes, then serves the page.
6. `google.com` etc. keep working: non-Federate names are forwarded to
   upstream DNS with anti-spoofing checks.

---

## 0. Deployment checklist

- [ ] Build release binaries (§1)
- [ ] Create Linux user + directories (§2)
- [ ] Copy binaries, sites, blocklists (§3)
- [ ] Install systemd units + node env file (§4)
- [ ] Install Caddy with the host-routing Caddyfile (§5)
- [ ] Configure firewall / open ports (§6)
- [ ] Set DNS records for `federate.network` (§7)
- [ ] Start everything, pin the root key (§8)
- [ ] Run health checks on the box (§9)
- [ ] Test DNS + gateway from outside (§10)
- [ ] First friends-only test from a phone (§11)

Commands below assume Ubuntu 22.04/24.04 or Debian 12 as root. Replace
`<VPS_IP>` with the server's public IPv4 everywhere.

## 1. Build release binaries

On the server (or cross-compile locally for `x86_64-unknown-linux-gnu`):

```sh
apt update && apt install -y build-essential pkg-config curl git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
git clone https://github.com/c3b/federatenetwork /opt/federatenetwork
cd /opt/federatenetwork
cargo build --release -p federate-server -p federate-dnsd -p federate-gatewayd -p federate-cli
```

## 2. Create user and directories

```sh
useradd -r -m -d /var/lib/federate federate
mkdir -p /etc/federate
```

`federate-dnsd` and `federate-gatewayd` use systemd `DynamicUser` +
`StateDirectory`, so they need no user of their own.

## 3. Copy binaries and data

From the repo checkout (local or `/opt/federatenetwork`):

```sh
install -m 755 target/release/federate-server  /usr/local/bin/
install -m 755 target/release/federate-dnsd    /usr/local/bin/
install -m 755 target/release/federate-gatewayd /usr/local/bin/
install -m 755 target/release/federate         /usr/local/bin/
rsync -a sites/ /var/lib/federate/sites/
install -m 644 blocked_tlds.txt /var/lib/federate/blocked_tlds.txt
rsync -a data/blocked/ /var/lib/federate/blocked/
chown -R federate:federate /var/lib/federate
```

`blocked_tlds.txt` (the IANA collision blocklist) is **required**: the server
refuses to start without it, so no restart can ever run without public-DNS
collision protection.

## 4. systemd units + node environment

```sh
cp deploy/systemd/federate-server.service   /etc/systemd/system/
cp deploy/systemd/federate-dnsd.service     /etc/systemd/system/
cp deploy/systemd/federate-gatewayd.service /etc/systemd/system/
cp deploy/federate-node.env.example         /etc/federate/node.env
chmod 600 /etc/federate/node.env
```

Edit `/etc/federate/node.env`:

- `PUBLIC_IP=<VPS_IP>` (must be the real public IP: the directory rejects a
  registration whose health endpoint host is not one of the node's declared
  IPs, anti-SSRF)
- `ROOT_KEY=` leave for §8 (printed by the server on first start)
- `REGION=` e.g. `de-fsn`

### Free port 53 (Ubuntu ships systemd-resolved on it)

`systemd-resolved` holds `127.0.0.53:53`, which conflicts with binding
`0.0.0.0:53`. Move the box's own resolution off the stub listener:

```sh
mkdir -p /etc/systemd/resolved.conf.d
printf '[Resolve]\nDNS=1.1.1.1 9.9.9.9\nDNSStubListener=no\n' \
  > /etc/systemd/resolved.conf.d/federate.conf
ln -sf /run/systemd/resolve/resolv.conf /etc/resolv.conf
systemctl restart systemd-resolved
```

## 5. Caddy (TLS + host routing on port 80)

```sh
apt install -y caddy
cp deploy/caddy/Caddyfile /etc/caddy/Caddyfile
systemctl reload caddy
```

The Caddyfile routes by Host header: `federate.network` goes to
`federate-server` (with automatic Let's Encrypt), **every other Host on
port 80 goes to the gateway**. That catch-all is what serves
`http://home.fed`.

No Caddy? Run the gateway directly on port 80 instead: edit
`federate-gatewayd.service` to `--listen 0.0.0.0:80` (the unit already
grants `CAP_NET_BIND_SERVICE`), and use nginx/certbot or nothing for the
`federate.network` API.

## 6. Firewall

```sh
ufw allow 22/tcp        # ssh
ufw allow 80/tcp        # Caddy -> gateway + ACME
ufw allow 443/tcp       # Caddy -> Node 1 API over TLS
ufw allow 53/udp        # Federate DNS
ufw allow 53/tcp        # Federate DNS (TCP fallback)
ufw allow 8081/tcp      # gateway health endpoint (directory health checks)
ufw allow 8053/tcp      # dns node health endpoint
ufw allow 4077/tcp      # native Federate protocol (Node 1 listener)
ufw enable
```

Node 1 itself stays on 127.0.0.1:9000 behind Caddy; never expose it.

## 7. DNS records for federate.network

At the registrar:

```
A     federate.network   <VPS_IP>
AAAA  federate.network   <VPS_IPv6>    (optional)
```

Wait for propagation (`dig federate.network` from your laptop shows
`<VPS_IP>`), or Let's Encrypt issuance in §5 fails until it does.

## 8. Start services and pin the root key

Order matters only for convenience; everything retries.

```sh
systemctl daemon-reload
systemctl enable --now federate-server
journalctl -u federate-server -n 20 --no-pager
```

The startup log prints the root key:

```
root zone signed: T TLDs, N domains, M blocks (root key <64-hex>)
```

Put that hex into `/etc/federate/node.env` as `ROOT_KEY=...`, then:

```sh
systemctl enable --now federate-gatewayd federate-dnsd
```

Pinning matters: with `ROOT_KEY` set, a node rejects any zone not signed by
that exact key. Without it the node trust-on-first-use pins whatever the
first fetched zone advertises (fine for demos, unsafe in production).

## 9. Health checks on the box

```sh
curl -s https://federate.network/health            # -> ok
curl -s https://federate.network/v1/status | head  # root_version, tlds, ...
curl -s http://127.0.0.1:8081/health               # gateway -> ok
curl -s http://127.0.0.1:8053/health               # dns node -> ok
curl -s -H "Host: home.fed" http://127.0.0.1:8080/ | head -3   # site HTML
/usr/local/bin/federate directory list --bootstrap https://federate.network
# expect: gateway + dns nodes listed, status online (give it ~30s after start)
```

The DNS node answers SERVFAIL for Federate names during its first ~10s
(until its first gateway-list refresh). Wait, then:

```sh
dig @127.0.0.1 home.fed +short        # -> <VPS_IP>
dig @127.0.0.1 home.fed +tcp +short   # same answer over TCP
dig @127.0.0.1 google.com +short      # -> real Google IPs (forwarded)
```

## 10. External validation (run from your laptop, NOT the VPS)

```sh
dig @<VPS_IP> home.fed            # -> <VPS_IP>, TTL 30, flags include aa
dig @<VPS_IP> home.fed +tcp       # same over TCP 53
dig @<VPS_IP> google.com          # -> forwarded upstream answer
curl -H "Host: home.fed" http://<VPS_IP>/          # -> site HTML, 200
curl -sI -H "Host: home.fed" http://<VPS_IP>/ | grep -i etag   # content hash
curl https://federate.network/v1/root | head      # signed zone over TLS
```

Then the real browser test:

1. On a laptop or phone, set the DNS server to `<VPS_IP>`
   (Wi-Fi settings, or on Android: Private DNS off + manual DNS via the
   network settings; on iOS: Wi-Fi > Configure DNS > Manual).
2. Open `http://home.fed`.
3. The page loads through the gateway; verification failures would render
   an error page, never unverified content.
4. Open a normal site (google.com) to confirm forwarding does not break the
   rest of the internet.

Watch it happen in the logs:

```sh
journalctl -u federate-dnsd -f       # queries + gateway refreshes
journalctl -u federate-gatewayd -f   # HTTP serving
journalctl -u federate-server -f     # registry + directory + health checks
```

## 11. First external test (friends-only)

Send a friend two things: `<VPS_IP>` and the root key hex.

1. Friend sets device DNS to `<VPS_IP>` (or router DNS for the whole home).
2. Friend opens `http://home.fed` and, e.g., `http://joao.pagina`.
3. Friend restores their DNS afterwards (this is a test, not a commitment).

Friends running the desktop daemon instead of raw DNS:

```sh
federated --bootstrap https://federate.network --root-key <hex>
```

pins the root key explicitly and verifies every layer locally; see
[desktop-setup.md](desktop-setup.md).

What to collect from testers: does `home.fed` load, does normal browsing
still work, how slow does it feel, exact time of any failure (so you can
match `journalctl` output).

## 12. Rollback

Binaries are stateless; state lives in `/var/lib/federate*` and is either
re-derivable (root zone, manifests, blocks rebuild from `sites/` at every
start) or self-healing (nodes re-register within ~60s).

Roll back a bad binary:

```sh
systemctl stop federate-server federate-gatewayd federate-dnsd
# keep the previous binary around at deploy time:
#   cp /usr/local/bin/federate-server /usr/local/bin/federate-server.prev
cp /usr/local/bin/federate-server.prev /usr/local/bin/federate-server
systemctl start federate-server federate-gatewayd federate-dnsd
```

Roll back a bad site publish:

```sh
rsync -a --delete <good-sites-checkout>/sites/ /var/lib/federate/sites/
systemctl restart federate-server
```

The re-signed zone gets a new `root_version` (derived from the clock), so
daemons accept it; they only reject zones **older** than one they verified.

Take DNS out of service without touching the rest:

```sh
systemctl stop federate-dnsd    # testers' devices fall back to their
                                # secondary DNS for normal internet
```

Total teardown: `systemctl disable --now federate-server federate-dnsd
federate-gatewayd`, remove the ufw rules, delete `/var/lib/federate*`.
Testers just remove the custom DNS from their devices.

**Never lose `/var/lib/federate/data/root/identity.key`** (see backups
below): binaries and sites are replaceable, the root key is not.

## Key storage & backups

Private keys live under the server data dir (`/var/lib/federate/data`):
`root/`, `official-operator/`, `dev-owner/`, per-TLD operator keys. They are
written `0600` and never served by any API.

- Back up `root/identity.key` offline. Losing it means never signing a new
  root zone; leaking it compromises the whole namespace.
- Verify permissions after first start:
  `find /var/lib/federate/data -name identity.key -exec ls -l {} +`
  (every key `-rw-------`, owned by `federate`).
- Suggested backup (keys + persistent registry + directory snapshot, NOT
  block caches):
  `tar czf federate-backup.tgz -C /var/lib/federate data`, stored off-box.
  `data/registry/registry.redb` IS the authoritative network state (an
  embedded redb database: records, zone versions, mutation history, audit
  log, nonces); back it up like a database, ideally with
  `federate registry backup` / `restore` (see
  [backups.md](backups.md)). Nodes upgraded from the old JSON layout run
  `federate registry migrate-json-to-redb` once (see
  [migrations.md](migrations.md)).

## Restart behavior

`Restart=on-failure` + `RestartSec=3` in every unit. The server NEVER
creates TLDs from code: initialize and seed the registry explicitly before
first start (`federate root init` + `federate root seed --file
seeds/official-tlds.toml --data-dir /var/lib/federate/data`), then every
boot loads `data/registry/` as the source of truth, re-verified against the
root key (see [root-registry.md](root-registry.md)). Root zone versions increase strictly
across mutations and restarts, so daemons (which reject zones older than
one they already verified) always accept the current zone. Registered nodes
persist in `data/directory-nodes.json` and re-verify on load; nodes also
re-register every ~60s.

## Logs

`journalctl -u federate-server -f` (also `-u federate-dnsd`,
`-u federate-gatewayd`). Verbosity: `Environment=RUST_LOG=debug` in the unit.

## Shared VPS deployment (Docker + existing reverse proxy)

This is how the FIRST real deployment of Node 1 was executed (2026-07-07,
Hetzner VPS at 195.201.171.223): a shared box where ports 80/443 belong to
an existing Traefik, ufw is active, root/sudo was not available, and a
dozen other services must keep working. Everything runs as Docker
containers under a normal user in the `docker` group; docker-published
ports bypass ufw, so no firewall edits were needed for 53/4077.

The pieces live in `deploy/docker/`: `Dockerfile` (all four binaries +
seeds + blocklists), `docker-compose.yml`, `traefik-federate-catchall.yml`,
`entrypoint.sh`.

Layout on the box: `~/federate/src` (source), `~/federate/data/{node1,gatewayd,dnsd}`
(state; keys and registry.redb live in node1), `~/federate/backups`.

```sh
# 1. build the image (on the box)
cd ~/federate/src
docker build -f deploy/docker/Dockerfile -t federate:latest .

# 2. explicit registry bootstrap (one-shot containers, server not running)
docker run --rm --user 1001:1001 -e HOME=/tmp \
  -v $HOME/federate/data/node1:/var/lib/federate/data \
  federate:latest federate root init --data-dir /var/lib/federate/data
docker run --rm --user 1001:1001 -e HOME=/tmp \
  -v $HOME/federate/data/node1:/var/lib/federate/data \
  federate:latest federate root seed \
  --file /var/lib/federate/seeds/official-tlds.toml --data-dir /var/lib/federate/data

# 3. configure and start the stack
cp ~/federate/src/deploy/docker/docker-compose.yml ~/federate/
cat > ~/federate/.env <<ENV
PUBLIC_IP=195.201.171.223
REGION=de-fsn
FEDERATE_UID=1001
FEDERATE_GID=1001
FEDERATE_DATA=/home/c3b/federate/data
DNS_UPSTREAM=1.1.1.1:53
ROOT_KEY=<hex printed by root init>
ENV
chmod 600 ~/federate/.env
cd ~/federate && docker compose up -d

# 4. publish the demo site through the ingest API
docker run --rm --user 1001:1001 -e HOME=/tmp --network federate_federate \
  -v $HOME/federate/cli:/keys federate:latest \
  federate publish package /var/lib/federate/sites/home-fed \
  --domain home.fed --key-dir /keys/owner --bootstrap http://federate-server:9600

# 5. browser door: lowest-priority catch-all on the existing Traefik
docker run --rm -v /opt/traefik/dynamic:/dyn \
  -v $HOME/federate/src/deploy/docker:/srcd federate:latest \
  cp /srcd/traefik-federate-catchall.yml /dyn/90-federate-catchall.yml
```

Port map: Node 1 HTTP on 127.0.0.1:9600 (loopback only), native protocol
0.0.0.0:4077 (public), gateway 127.0.0.1:8095 (Traefik catch-all routes
every unclaimed Host there, priority 1), DNS on PUBLIC_IP:53 udp+tcp
(binding the specific public IP leaves systemd-resolved on 127.0.0.53
untouched), health endpoints on PUBLIC_IP:8081/8053.

One real-world gap this deploy surfaced: Docker excludes same-bridge
traffic from published-port DNAT, so Node 1's health probes to the sibling
nodes' public health endpoints (the only host the registration SSRF guard
accepts) timed out and the nodes decayed to offline, which empties DNS
answers. Fix shipped in `entrypoint.sh` + compose: the server container
gets NET_ADMIN, installs two DNAT rules redirecting exactly those probe
destinations to the siblings' static container IPs (172.30.77.11/12), then
drops privileges to RUN_AS. Nothing on the host changes.

Backups: `~/federate/backup.sh` (installed in the user crontab, daily at
04:20) runs `federate registry backup` into `~/federate/backups/` plus a
keys+content tarball, keeping the last 14 of each. Private keys are 0600
files in the data volume; they are never inside the image or the database.

Verification actually run from an external machine:

```sh
dig @195.201.171.223 home.fed          # -> 195.201.171.223 (see caveat)
dig @195.201.171.223 google.com        # -> upstream-forwarded answer
curl -H "Host: home.fed" http://195.201.171.223/   # -> the home.fed page via Traefik
federate node ping --addr 195.201.171.223:4077     # -> native handshake, v1, root-authority
federate fetch fed://home.fed/ --provider 195.201.171.223:4077 \
  --root-key <root key hex>            # -> full chain verified, 2557 bytes
```

Caveat found during verification: some access networks intercept ALL
port-53 traffic (the giveaway: answers carry the `ad` flag and EDNS, which
this DNS server does not emit) and answer NXDOMAIN for Federate names from
the public roots while google.com still resolves. On such networks test
DNS from another vantage point; the native protocol (4077) and the HTTP
door (80) are unaffected.

### Encrypted DNS (DoH): the setting that works on ANY network

Many ISPs and home routers silently intercept every packet on port 53 and
answer from the public DNS, where Federate TLDs do not exist (the
giveaway: NXDOMAIN answers with the `ad` flag, EDNS, and a round-trip far
below the real network RTT). Plain DNS to the node is dead on such
networks, no matter what the device configures. DNS-over-HTTPS rides on
port 443 and cannot be intercepted.

The stack above includes `federate-doh` (a DoH terminator forwarding to
`federate-dnsd`), and `traefik-federate-network.yml` routes
`https://federate.network/dns-query` to it with a Let's Encrypt
certificate. The same file exposes Node 1's bootstrap API at
`https://federate.network`, which makes the CLI's DEFAULT bootstrap URL
live: `federate fetch fed://home.fed/` works with zero flags.

User setup, once, no router or ISP involvement:

- **macOS/iOS**: install `deploy/federate-dns.mobileconfig` (double-click,
  then System Settings, General, Device Management, Install). System-wide.
- **Chrome/Edge/Firefox**: Settings, Secure DNS, custom provider:
  `https://federate.network/dns-query`.
- **Verification from any machine**:
  `curl --doh-url https://federate.network/dns-query http://home.fed/`

Plain port-53 DNS (195.201.171.223) keeps working for networks that do
not intercept; hosts-file mappings ([hosts-setup.md](hosts-setup.md))
remain the last-resort fallback.

Phone/desktop onboarding: install the DoH profile above (or set the
device's DNS server to 195.201.171.223 on networks without interception),
then open `http://home.fed`.

## Scaling out (later)

Anyone can add capacity without touching Node 1: more gateway nodes
(`federate-gatewayd` on other VPSes, registered via `--public-ip`), more DNS
nodes, storage/CDN/search via `federate-noded`; see [nodes.md](nodes.md).
The directory health-checks them and DNS starts answering with every healthy
gateway automatically.
