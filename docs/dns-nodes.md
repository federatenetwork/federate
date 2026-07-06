# DNS nodes

> [Versão em português (pt-BR)](pt-BR/dns-nodes.md)

`federate-dnsd` is an authoritative DNS server for Federate TLDs. Anyone can
run one.

## Behavior

For a query like `home.fed`:

1. Confirm `.fed` exists in the **signed root zone** (signature verified
   against the pinned Federate Root Key; an unverifiable zone is never used).
2. Ask the node directory for **healthy gateway nodes**.
3. Return **multiple** A/AAAA records, never one hardcoded IP:

   ```
   home.fed  A  45.1.1.1
   home.fed  A  45.2.2.2
   home.fed  A  45.3.3.3
   TTL: 30 seconds
   ```

   The directory ranks gateways by health then latency; the low TTL means
   failed gateways drop out of answers within seconds.

4. Any non-Federate name (`google.com`, …) is forwarded verbatim to upstream
   DNS (`1.1.1.1:53` by default, `--upstream 8.8.8.8:53` to change), so
   normal internet resolution is never broken.

If no healthy gateway exists, the server answers SERVFAIL rather than a stale
or invented IP.

Operational limits (current implementation):

- Answers are capped at **8 records** so every response fits a plain
  512-byte UDP reply.
- **UDP only** for now - no TCP listener, no EDNS. Truncation never happens
  because of the answer cap, but resolvers that insist on TCP retry will not
  get an answer yet.
- TLDs whose root-zone record is expired (`expires_at` in the past) are
  treated as non-Federate and forwarded upstream like any other name.
- Upstream forwarding uses a fresh connected socket per query (random source
  port) and requires a matching DNS transaction ID, so off-path spoofed
  answers are dropped.

DNS only answers *where a name should go*. Gateways still verify the full
root → TLD → domain → manifest → block chain before serving anything.

## Run one

```sh
federate-dnsd \
  --listen 0.0.0.0:53 \
  --bootstrap https://federate.network \
  --root-key <FEDERATE_ROOT_PUBLIC_KEY_HEX> \
  --public-ip <YOUR_PUBLIC_IP> \
  --region br-sp
```

- `--root-key` pins the trust anchor (strongly recommended; otherwise the key
  is pinned on first use).
- `--public-ip` registers the node in the directory (role `dns`) and starts
  the health API (`--health-listen`, default `0.0.0.0:8053`).
- Port 53 needs privileges; use `setcap` on Linux or run the systemd unit.

Test it:

```sh
federate dns test home.fed --server <ip>:53
federate dns test example.com --server <ip>:53   # forwarded upstream
```
