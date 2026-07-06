# HANDOFF: Decentralized infrastructure roles (2026-07-06)

## Goal delivered

Federate Network no longer depends on a single VPS for everything.
`federate.network` (Node 1) stays the official **root authority** for TLDs;
everything else (DNS, gateways, storage, CDN, search, bootstrap, root
mirroring) can now be run by anyone as a Federate node.

Core principle enforced in code:

> Federate Root decides what is valid.
> Federate nodes distribute, resolve, cache, mirror, search, and serve valid data.

## Status

- `cargo build --workspace` - clean, 0 warnings/errors
- `cargo test --workspace` - 18 tests, all pass
- End-to-end smoke-tested with real processes (see "Verification" below)
- Nothing committed yet - all changes are in the working tree

## New crates

| Crate | What it does |
|---|---|
| `federate-registry` | Registry model over the signed root zone: `RegistryView::locate_domain` routes root-managed vs delegated TLDs; `DelegatedRegistryClient` fetches operator-registry domain records and verifies against operator key from the root-signed TLD record. |
| `federate-node` | Generic node runtime: `NodeConfig` (TOML: `[node]` roles/region/public_ip/listen/dns_listen, `[network]` bootstrap/directory/root_key/upstream_dns, `[capacity]`), `NodeRuntime` (identity, signed registration builder, 60s re-registration heartbeat loop, standard `/health` `/status` `/roles` axum router). |
| `federate-directory` | The node directory. `NodeRole` enum (root-authority, root-mirror, dns, gateway, storage, cdn, search, bootstrap, origin), `NodeRegistration` (signed: node_id=public_key, roles, public_ips, region, version, capacity, health_endpoint, ed25519 signature over canonical JSON), `Directory` (register w/ signature verification, list by role, `healthy()` sorted online→degraded→latency, block-provider tracking via `announce_blocks`/`providers_for_block`), `health_check_loop` (polls `{health_endpoint}/health`; 200→online, 1-2 fails→degraded, 3+→offline), axum `router()` + `DirectoryClient`. **Rejects `root-authority` role from any key except the pinned Federate Root Key.** |
| `federate-cdn` | `rank_providers` (online first, same region, lowest latency), `fetch_block_from` (hash-verified provider fetch), `CdnCache` (size-capped LRU over `BlockStore`, JSON index `cdn-index.json`, `cached_hashes()` for announcements). |
| `federate-search` | `SearchIndex` (inverted index, TF × matched-terms ranking), opt-out honored (`<meta name="federate" content="noindex">` or robots noindex), `index_from_resolver` (indexes only via the signature-verifying resolver), axum router `/v1/search` + `/v1/search/stats`. Policy: no ads, no tracking, no AI training (surfaced in every search response). |
| `federate-bootstrap` | `BootstrapInfo` (root key/version/url, root_mirrors, dns_nodes, gateway_nodes, directory_nodes, bootstrap_nodes) + `BootstrapClient::fetch`/`discover`. |

## New binaries

| Binary | What it does |
|---|---|
| `federate-dnsd` | DNS node anyone can run. Args: `--listen` (default 0.0.0.0:5353; production 53), `--bootstrap`, `--directory`, `--upstream` (default 1.1.1.1:53), `--root-key`, `--public-ip` (enables directory registration + health API on `--health-listen`, default :8053), `--region`. |
| `federate-gatewayd` | Public gateway node. Same flag pattern; `--listen` default 0.0.0.0:8080, health API default :8081. Resolver runs with directory-backed block fetching. |
| `federate-noded` | Multi-role node from `federate.toml` (or `--roles gateway,dns,cdn` override). One HTTP listener merges: health router + `/v1/block/:hash` (storage serves cache; cdn fetch-on-miss) + `/v1/root` (root-mirror, serves only locally verified zone) + search routes + `/v1/bootstrap` relay + gateway fallback. DNS role adds UDP listener. Announces cached blocks to directory every 60s. **Refuses `root-authority` role.** |
| `federate-searchd` | Standalone search node; reindexes every `--reindex-secs` (600). |

## Rewritten: federate-dns

Was a stub returning 127.0.0.1. Now a real authoritative UDP DNS server:

- `wire.rs` - minimal hand-rolled DNS codec (parse question, build A/AAAA
  answers with compression pointer, SERVFAIL, `build_query`/`parse_answers`
  for CLI testing). Round-trip test included.
- `DnsServer` - verifies root zone signature (via shared `Resolver`; never
  trusts unverified root data), answers any name under a resolvable TLD in
  the signed zone with **multiple healthy gateway IPs from the directory**
  (never one hardcoded IP), **TTL 30s**, SERVFAIL when no healthy gateway,
  forwards all non-Federate names verbatim to upstream (3s timeout).
  Background loops: gateway list refresh every 10s, root zone every 60s.

## Modified existing code

- `Cargo.toml` (workspace) - 6 new lib crates + 4 new bins as members;
  added `toml = "0.8"` workspace dep.
- `federate-storage` - added `BlockStore::remove` (for CDN eviction).
- `federate-client` - added `pub async fn get_json(url)` helper.
- `federate-resolution` - `Resolver` gained optional directory support:
  `with_directory(DirectoryClient, region)`. `block()` now tries ranked
  CDN/storage/origin providers first (each response hash-verified; bad
  providers skipped), falls back to Node 1. Added public
  `fetch_and_cache_block` (CDN fetch-on-miss path).
- `federate-server` (Node 1) - hosts the official node directory:
  `Store.directory`, merged `federate_directory::router`, spawns
  `health_check_loop` (15s). `/v1/bootstrap` now returns live
  root_mirrors/dns_nodes/gateway_nodes/bootstrap_nodes from the directory.
  Removed the `/v1/nodes` stub (real endpoint now).
- `federate-cli` - new subcommands:
  - `federate node register|status|roles|health|list|run --roles ...`
    (`run` spawns `federate-noded`)
  - `federate dns test <domain> --server ip:port` (sends real DNS query via
    `federate_dns::wire`, prints A/AAAA + TTL)
  - `federate gateway test <domain> --gateway url` (Host-header request)
  - `federate directory list --role gateway|dns|storage|cdn [--healthy]`

## Docs written

- `docs/decentralization.md` - what is/isn't decentralized, why TLD authority
  stays root-signed, chain-of-trust diagram, data-flow diagram
- `docs/nodes.md` - roles table, config format, registration, health API, CLI
- `docs/dns-nodes.md` - DNS node behavior + how to run one
- `docs/gateway-nodes.md` - gateway verification chain + how to run one
- `docs/storage-cdn-nodes.md` - storage vs cdn, trust model, provider selection
- `docs/root-mirrors.md` - mirrors distribute, can't modify (signature makes cheating pointless)
- `docs/node-directory.md` - tracked fields, registration, health checking, API
- `docs/README.md` - updated binary table + doc index
- `deploy/federate.toml.example` - annotated node config

## Security model (all enforced in code)

- node registrations signed by node key; directory verifies before accepting
- root zone + TLD records signed by Federate Root Key; verified against
  pinned trust anchor everywhere (DNS, gateway, mirror, search, CLI)
- domain records signed by TLD Operator Key; manifests by Domain Owner Key
- every block hash-verified on fetch AND cache read; bad providers detected
  and skipped
- `root-authority` role restricted to the root key at directory level, and
  refused outright by `federate-noded`
- root mirrors serve only locally-verified zones; consumers re-verify anyway

## Verification performed

Real-process smoke tests (all passed):

1. `federate-server` (:9100) + `federate-gatewayd` (:9180, registered) +
   `federate-dnsd` (:9153): gateway appears in directory as online;
   `federate dns test home.fed` → gateway IP, TTL 30s;
   `federate dns test example.com` → forwarded upstream;
   `federate gateway test home.fed` → 200 OK, 2977 bytes, full chain verified.
2. `federate-noded` with roles gateway,cdn,root-mirror,search,dns on one box:
   `/health` ok, `/roles` correct, `/v1/root` mirror serves signed zone,
   gateway serves home.fed, `/v1/search?q=federate` returns ranked result
   with policy block, DNS answers home.fed (after ~10s gateway-refresh
   warm-up) and forwards example.com, node listed in directory with all 5 roles.

## Known gaps / next steps

- DNS is UDP-only (no TCP fallback / truncation handling); fine for A/AAAA.
- DNS gateway cache is empty for the first ~10s after start (SERVFAIL until
  first directory refresh). Could refresh once before binding.
- Directory is in-memory (state lost on Node 1 restart; nodes re-register
  within 60s so it self-heals).
- `NodeRuntime` health_endpoint assumes `http://{public_ip}:{listen port}` -
  storage/CDN block URLs reuse health_endpoint as base (works because noded
  serves blocks + health on one listener).
- Search crawls only `/` per domain (manifest path walk not yet wired).
- Delegated registry resolution still phase-6 stub (unchanged behavior).
- Nothing committed to git yet.

---

# HARDENING REVIEW (2026-07-06)

Inspection + fixes on top of the handoff above. Build/tests/clippy all clean:
`cargo build --release` ok, `cargo test --workspace` = **25 passing**,
`cargo clippy --workspace --all-targets` = **0 warnings**, `cargo fmt` clean.

## What was broken

1. **Path traversal + panic in the block store.** `BlockStore` built file paths
   straight from caller-supplied hash strings (`self.dir.join(&hash[..2])…`).
   A hash like `../../etc/x` escaped the block dir, and a multi-byte value
   (e.g. `é`) panicked on `hash[..2]`. The same unvalidated hashes reached
   `/v1/block/:hash`, `/v1/manifest/:hash`, and CDN fetch URLs.
2. **DNS answer spoofing.** The upstream forwarder used an *unconnected* UDP
   socket and returned the first datagram received, without checking the DNS
   transaction ID, so an off-path attacker could inject forged answers for
   `google.com` et al.
3. **Unauthenticated block announcements.** `POST /v1/nodes/announce-blocks`
   took `{node_id, blocks}` with no signature, so anyone could stuff any
   node's provider list (provider-map poisoning / DoS).
4. **SSRF via `health_endpoint`.** Registrations carried an arbitrary URL that
   the directory's health checker and gateway block-fetches would GET every
   few seconds: a node could point it at `169.254.169.254` or someone else's
   server. IPs weren't validated either.
5. **Broken manifest cache.** The resolver cached *re-serialized* manifest JSON
   under the original hash, so the cached bytes never re-hashed to that hash:
   every read failed verification, was evicted, and re-fetched. The cache path
   was also built from an unvalidated hash (traversal).
6. **Search only crawled `/`.** The manifest path-walk was stubbed, so only each
   site's entry page was ever indexed.
7. **`.busca` referenced but undefined.** DNS/docs named `.busca` as a Federate
   search TLD, but it wasn't in the signed root, so it could never resolve.
8. **Missing gateway response hardening / tests.** No `nosniff`, and no tests
   for XSS-escaping, path/hash traversal, DNS gateway selection, or the
   delegated-registry router.

## What was fixed

- `federate-storage`: added `is_valid_hash` (64 lowercase hex); `block_path`
  returns `None` for anything else; `verify`/`get`/`put`/`remove`/`has` all
  reject malformed hashes. New traversal/panic test.
- `federate-dns`: `forward_upstream` now `connect`s the socket to upstream and
  loops until it sees a reply whose transaction ID matches the query
  (bounded by the 3s deadline).
- `federate-directory`: new **signed** `BlockAnnounce` (Ed25519 by the node
  key); `announce_blocks` verifies it, requires prior registration, and drops
  malformed hashes. `NodeRegistration::verify` now requires valid `public_ips`
  and a `health_endpoint` whose host is one of them (anti-SSRF). New tests for
  gateway selection (multi + offline-excluded), signed announce, and SSRF.
- `federate-client` / `federate-resolution`: manifests fetched as raw
  hash-verified bytes and cached verbatim (real cache hits); hash validated
  before any cache path is built.
- `federate-gateway`: content responses now send `X-Content-Type-Options:
  nosniff` and `Referrer-Policy: no-referrer`. New XSS-escape test.
- `federate-search`: indexer walks every `.html`/`.htm` file in the verified
  manifest via the new `Resolver::site_files`, not just `/`.
- `federate-naming`: added the official root-managed `.busca` TLD.
- `federate-registry`: 3 new tests (root-managed routing, delegated routing,
  unknown-TLD/missing-domain errors).
- Docs: `deployment-hetzner.md` (DNS port 53 UDP+TCP, gateway/node deploy,
  ufw firewall, key storage/backups, logs), `node-directory.md` (anti-abuse
  rules), and this handoff.

## Remaining risks

- **`health_endpoint` can still be a private/loopback IP.** We tie the host to a
  declared `public_ip` but do not block RFC-1918 / link-local ranges, because
  local-dev nodes legitimately use `127.0.0.1`. On a public directory, front it
  behind a checker that refuses private ranges, or add an allowlist.
- **DNS is UDP-only.** No TCP/53 or truncation (TC-bit) handling; fine for
  single A/AAAA answers, but large answer sets get truncated silently.
- **Directory is in-memory.** State is lost on Node 1 restart (nodes
  re-register within 60s; providers within ~60s). No persistence yet.
- **No registration replay window.** `registered_at`/`announced_at` are signed
  but not checked against a freshness window or nonce, so an old signed
  registration could be replayed. Low impact (same key, same node), but worth a
  timestamp/nonce check before opening the directory to the public internet.
- **Delegated registry resolution is still a phase-6 stub.**
- **Node 1 block/manifest maps are in-memory** and rebuilt from `sites/` at
  startup; large corpora will want on-disk storage.

## Commands

Local:
```sh
cargo fmt --all
cargo clippy --workspace --all-targets    # 0 warnings
cargo test --workspace                     # 25 passing
cargo build --release
# smoke: Node 1 + gateway + DNS
./target/release/federate-server --listen 127.0.0.1:9100 &
./target/release/federate-gatewayd --listen 0.0.0.0:9180 --bootstrap http://127.0.0.1:9100 \
  --public-ip 127.0.0.1 --health-listen 0.0.0.0:9181 &
./target/release/federate-dnsd --listen 0.0.0.0:9153 --bootstrap http://127.0.0.1:9100 &
./target/release/federate dns test home.fed --server 127.0.0.1:9153
./target/release/federate gateway test home.fed --gateway http://127.0.0.1:9180
./target/release/federate root verify --bootstrap http://127.0.0.1:9100
```

Hetzner: see `docs/deployment-hetzner.md` §8-10 (DNS port 53, gateway node,
ufw firewall, key backups).

---

# ENGINEERING REVIEW ROUND 2 (2026-07-06)

Full-repo review pass on top of the hardening above. Build/clippy/fmt clean,
42 tests passing, ETag flow smoke-tested with real processes.

## Changed

- **Gateway ETag/304**: `Resolved::Content` now carries the block's content
  hash; the gateway sends it as a strong `ETag` and answers `304 Not
  Modified` to matching `If-None-Match` (strong, weak `W/`, lists, `*`).
  Content-addressed hashes are perfect validators: same hash means
  byte-identical bytes. Verified live: 304 on match, 200 on mismatch.
- **DNS flood bound**: `DnsServer::run` caps concurrent packet handlers at
  `MAX_INFLIGHT_QUERIES` (512) with a semaphore. Before, every UDP packet
  spawned a task, and forwarded queries each held a socket for up to 3s, so
  a flood meant unbounded task/fd growth.
- **CDN cache I/O**: `CdnCache::get` no longer rewrites the whole JSON index
  on every read (recency stays in memory; index persists on `put`). Eviction
  loop now stops at the size target instead of scanning the full list. Index
  writes are now write-then-rename (crash cannot truncate).
- **Delegated registry client**: `fetch_domain` now goes through
  `federate_client::get_json` (10s timeout + 4MB cap) instead of an
  uncapped per-call reqwest client. That was the last uncapped cross-node
  fetch in the codebase.
- **Immutable cache headers**: `/v1/block/:hash` and `/v1/manifest/:hash`
  (Node 1 and noded) send `Cache-Control: public, max-age=31536000,
  immutable`; the URL is the content address, so the response can never
  change.
- **CLI timeouts**: all CLI HTTP calls share one client with a 15s timeout
  (bare `reqwest::get` has none, so a dead node used to hang commands
  forever).
- **CI**: `.github/workflows/ci.yml` runs fmt check, clippy `-D warnings`,
  tests, and a release build on push/PR.

## Remaining risks / TODOs (carried + new)

- `health_endpoint` may still be a private/loopback IP (deliberate, for
  local dev); front a public directory with a private-range filter.
- DNS is UDP-only (no TCP fallback / TC bit).
- Node 1 block/manifest maps are in-memory, rebuilt from `sites/` at startup.
- No registration replay window (timestamp/nonce check) yet.
- Delegated registry resolution is still a phase-6 stub.
- `desired_tlds` at the repo root is an unreferenced scratch wordlist;
  decide whether it belongs in docs or should be deleted.
