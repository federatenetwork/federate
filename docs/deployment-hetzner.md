# Deploying Node 1 on Hetzner

> [Versão em português (pt-BR)](pt-BR/deployment-hetzner.md)

Node 1 = `federate-server` behind Caddy at `https://federate.network`.

## 1. Build release binaries

On the server (or cross-compile locally for `x86_64-unknown-linux-gnu`):

```sh
cargo build --release -p federate-server
```

## 2. Copy to the server

```sh
scp target/release/federate-server root@<hetzner-ip>:/usr/local/bin/
rsync -a sites/ root@<hetzner-ip>:/var/lib/federate/sites/
scp blocked_tlds.txt root@<hetzner-ip>:/var/lib/federate/blocked_tlds.txt
rsync -a data/blocked/ root@<hetzner-ip>:/var/lib/federate/blocked/
```

`blocked_tlds.txt` (the IANA collision blocklist) is **required**: the server
refuses to start without it, so no restart can ever run without public-DNS
collision protection.

## 3. Create a user and systemd service

```sh
ssh root@<hetzner-ip>
useradd -r -m -d /var/lib/federate federate
chown -R federate:federate /var/lib/federate
cp deploy/systemd/federate-server.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now federate-server
systemctl status federate-server
```

The unit runs `federate-server --listen 127.0.0.1:9000 --sites-dir /var/lib/federate/sites`.

## 4. Point DNS at the server

At your domain registrar, add for `federate.network`:

```
A     federate.network   <hetzner-ipv4>
AAAA  federate.network   <hetzner-ipv6>   (optional)
```

## 5. Reverse proxy + Let's Encrypt with Caddy

`federate.network` is a real public domain, so normal Let's Encrypt works.
Caddy handles certificates automatically:

```sh
apt install caddy
cp deploy/caddy/Caddyfile /etc/caddy/Caddyfile
systemctl reload caddy
```

(Nginx + certbot works equally; Caddy is the zero-config option.)

## 6. Verify

```sh
curl https://federate.network/health          # -> ok
curl https://federate.network/v1/status
curl https://federate.network/v1/root | head
```

## 7. Point local daemons at Node 1

Friends run `federated` with the default bootstrap, which is already
`https://federate.network`. Done; see [desktop-setup.md](desktop-setup.md).

## Updating sites

Re-rsync `sites/` and `systemctl restart federate-server`; it rebuilds the
root zone, manifests, and blocks at startup.

## 8. Running a DNS node (port 53)

A DNS node answers Federate TLDs with the IPs of healthy gateway nodes and
forwards everything else to upstream. Production DNS needs UDP **and** TCP 53.

```sh
scp target/release/federate-dnsd root@<ip>:/usr/local/bin/
# Bind low port 53 without root:
setcap 'cap_net_bind_service=+ep' /usr/local/bin/federate-dnsd
federate-dnsd \
  --listen 0.0.0.0:53 \
  --bootstrap https://federate.network \
  --root-key <federate-root-public-key-hex> \
  --upstream 1.1.1.1:53 \
  --public-ip <this-node-ipv4> --region <region>
```

- `--root-key` **must** be the real Federate Root public key. Without it the
  node trust-on-first-use pins whatever the first zone advertises; fine for a
  demo, unsafe for production.
- `--public-ip` must be a real IP of *this* box: the directory rejects a
  registration whose `health_endpoint` host is not one of the node's declared
  IPs (anti-SSRF), so a mismatch means the node never appears as healthy.
- Upstream forwarding connects the socket to the resolver and checks the DNS
  transaction ID, so off-path answer spoofing is rejected.

## 9. Running a gateway / storage / CDN node

```sh
federate-gatewayd --listen 0.0.0.0:80 --bootstrap https://federate.network \
  --root-key <hex> --public-ip <ip> --region <region>
# or a multi-role node from a config file:
federate-noded --config /etc/federate/federate.toml
```

`federate-noded` refuses the `root-authority` role, and the directory rejects
that role from any key except the pinned Federate Root Key, so no node can forge
root authority.

## 10. Firewall (ufw example)

```sh
ufw allow 22/tcp                 # ssh
ufw allow 80,443/tcp             # gateway + Caddy HTTPS
ufw allow 53/udp                 # DNS (DNS nodes only)
ufw allow 53/tcp                 # DNS TCP fallback (DNS nodes only)
# Node 1 listens on 127.0.0.1:9000 (behind Caddy); do NOT expose it.
ufw enable
```

## Key storage & backups

Private keys live under the server `--data-dir` (default `.federate-server`,
or `/var/lib/federate/data` in the unit): `root/`, `official-operator/`,
`dev-owner/`, per-TLD operator keys. They are written `0600` and are **never**
served by any API.

- Back up `root/identity.key` offline. Losing it means you can never sign a new
  root zone; leaking it compromises the whole namespace.
- The `.service` unit runs as the unprivileged `federate` user with
  `ProtectSystem=strict`, `ReadWritePaths=/var/lib/federate`, `PrivateTmp`,
  and `UMask=0077`.
- Verify permissions after first start:
  `find /var/lib/federate/data -name identity.key -exec ls -l {} +`
  - every key must be `-rw-------` and owned by `federate`.
- Suggested backup (keys + node directory snapshot, NOT the block cache):
  `tar czf federate-backup.tgz -C /var/lib/federate data` stored off-box.
  The root zone, manifests, and blocks are rebuilt from `sites/` at startup.

## Restart behavior

`Restart=on-failure` + `RestartSec=3` in every unit. The server rebuilds and
re-signs the root zone at startup; `root_version` is derived from the clock,
so daemons (which reject zones older than one they already verified) always
accept the new zone after a restart. Registered nodes are persisted in
`data/directory-nodes.json` and re-verified on load; nodes also re-register
every ~60 s.

## Logs

`journalctl -u federate-server -f` (and `-u federate-dnsd`, `-u federated`).
Set verbosity with `Environment=RUST_LOG=debug` in the unit.
