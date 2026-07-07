# Federate Network: current-state report

Date: 2026-07-06. HEAD: `0981c8a`. Workspace clean. Factual snapshot; no changes proposed here are implemented.

## Executive summary

The repo is a working single-operator overlay network with a genuinely native protocol path and a complete, tested signature chain. Root zone, TLD records, delegated TLD registries, domain records, manifests, and content blocks are all real, signed, verified fail-closed, and resolvable end to end over the native protocol with HTTP as fallback. Operator and owner tooling exists and works from independent machines. What does NOT exist: any runtime mutation (everything is re-signed at server startup from disk), payments, key rotation, abuse enforcement, native browser, non-HTML runtime, and any actual public deployment. The trust model is strong; the lifecycle/governance model is startup-time-only. 75 tests, all validation commands pass.

## What is real today

- Full verified resolution chain, both doors (native `fed://` path and HTTP gateway), one engine (`federate-resolution`).
- Native protocol v1 (framed JSON over TCP, port 4077): handshake with version negotiation, root/manifest/block/registry/record/status messages, served by Node 1 and noded, consumed by the resolver and the CLI. Verified live repeatedly with HTTP dead.
- Delegated TLDs: signed `TldRegistry`, 4 registry modes, fail-closed verification, per-TLD offline cache, version rollback protection. The `.femboy` seed and `eu.femboy` resolve through the delegated path.
- Operator/owner tooling: `federate site package` (blocks + owner-signed manifest, `--install` into node stores), `federate operator sign-record / build-registry / verify-registry`, noded `registry_files` serving. Verified live: one noded served registry + manifest + blocks natively with zero Node 1 involvement.
- DNS server: UDP + TCP, port-53 capable, answers Federate TLDs with up to 8 healthy gateway IPs (TTL 30s), forwards everything else upstream with spoofing guards, SERVFAIL when no gateways.
- Node directory: signed registrations (SSRF-guarded), health checking, role queries, signed block announcements, persistence across restarts, 5000-node cap, stale pruning.
- Security posture: canonical-JSON Ed25519 signing, BLAKE3 content addressing verified on fetch AND cache read, root key pinning (flag or TOFU persisted), root zone + registry rollback protection, expiry fail-closed (unparseable timestamp counts as expired), path-traversal-proof content addresses, capped reads everywhere, per-connection and inflight limits.

## What is partially implemented

- **Publishing**: the owner/operator side is real; but Node 1 still generates ALL official-TLD content from its local `sites/` dir with dev keys at startup. No ingest API. A stranger cannot publish under an official TLD without filesystem access to Node 1.
- **Registry/domain lifecycle**: statuses (suspended/revoked/expired...) fully enforced at resolution, but nothing can change a status at runtime; it requires editing seed data and restarting `federate-server`. Marketplace endpoints (`/v1/applications`, `tld apply/approve/block/reserve`) are stubs printing explanations.
- **CDN/storage**: LRU cache, fetch-on-miss, signed announcements, provider ranking (region + latency) all real; but no replication targets, no pinning, and directory-based provider discovery for manifests/registries uses only configured defaults, not per-hash announcements.
- **Search**: real crawler through the verifying resolver, opt-out honored, TF ranking, no ads/tracking/AI-training. But the index is in-memory only, rebuilds every 10 minutes from scratch, ranking is naive, no UI site is wired to `fed.busca` in the repo, and only root-zone (official TLD) domains are crawled.
- **Deployment**: complete docs (Hetzner-style VPS, hardened systemd units, Caddy host-routing, ufw, port-53 freeing, key backup, rollback), Dockerfile, launchd plist. Never executed against a real server; and the deploy docs predate the native listener: **no `4077/tcp` in the ufw list and no `--native-listen` in the systemd unit**.
- **Protocol**: `GetProviders`/`Providers` messages are defined and documented, served by nobody, used by nobody.

## What is only stubbed / docs only

- Payments/pricing: `pricing`/`renewal` JSON placeholder fields on records. Nothing else.
- TLD marketplace: roadmap doc + stub endpoints.
- Native browser: `future-federate-browser.md` only.
- Non-HTML runtime: `non-html-runtime-roadmap.md` only. The manifest is a file tree with MIME guessed by extension.
- Key rotation / recovery keys / multisig: "Future work" section in signatures.md only.
- Abuse/enforcement: blocklists (IANA 1437 entries, reserved 8, policy/brand-safety empty placeholders) enforced at TLD creation; no runtime enforcement mechanism, no report channel.
- Local OS DNS integration: `dns-resolver.md` plan; today it is hosts-file or pointing at a Federate DNS node.
- HTTPS for internal domains / local CA: `https-local.md` plan only. The gateway serves plain HTTP.
- QUIC transport: doc note only.

## Crates and binaries (26)

**Libraries** (all production-quality, all used):

| Crate | Purpose | Status |
|---|---|---|
| federate-core | errors, config, canonical JSON signing | real; used by all |
| federate-identity | Ed25519 keys on disk (0600), sign/verify | real |
| federate-naming | TLD/label rules, 23 official TLDs, record types, statuses, expiry | real |
| federate-uri | `fed://` parsing, HTTP-to-URI translation | real |
| federate-protocol | wire messages v0+v1, framing, negotiation | real |
| federate-transport | framed TCP, timeouts/caps, serve loop, NodeService trait | real |
| federate-root | RootZone, TldRecord, blocklists, disk cache | real |
| federate-registry | TldRegistry (delegated), RegistryView, HTTP registry client, file loader | real |
| federate-manifest | signed site manifests, path-to-hash mapping | real |
| federate-storage | BLAKE3, traversal-proof BlockStore | real |
| federate-client | HTTP compatibility client (capped reads) | real; explicitly the fallback layer |
| federate-resolution | THE engine: chain verification, native-first fetch, caches, rollback | real; heart of the system, 17 tests |
| federate-directory | node registry, health checks, providers, HTTP API | real |
| federate-cdn | LRU block cache, provider ranking, block fetch | real |
| federate-search | index/query, opt-out | real but minimal |
| federate-bootstrap | `/v1/bootstrap` types, native peer discovery | real |
| federate-node | shared node runtime: config, registration, heartbeat, health router | real |
| federate-dns | DNS server UDP+TCP, wire codec (hand-rolled, no EDNS) | real |
| federate-gateway | HTTP adapter over the resolver, ETag, security headers, error pages | real |

**Binaries**:

| Binary | Purpose | Status |
|---|---|---|
| federate-server | Node 1: builds+signs everything from disk at startup, serves native+HTTP, hosts the directory | real for dev/single-operator; startup-time-only mutation; in-memory stores |
| federated | local daemon: gateway :80, API :7777, native discovery | real |
| federate (CLI) | 20+ command groups incl. fetch --trace, node ping, delegated-registry, operator, site | real; some subcommands are informational stubs (tld apply/approve) |
| federate-noded | multi-role node (gateway/dns/storage/cdn/search/root-mirror/bootstrap), native listener, registry_files | real |
| federate-dnsd | standalone DNS node | real |
| federate-gatewayd | standalone gateway node | real |
| federate-searchd | standalone search node | real |

Duplication/confusion worth noting: dnsd/gatewayd/searchd overlap noded's roles (noded can do everything they do); they hand-assemble `NodeConfig` structs and each grew the new fields mechanically. Candidates for consolidation into noded. `federate-registry::RegistryView` / `DelegatedRegistryClient` partially duplicate routing logic now living in the resolver (`locate_record`); RegistryView is used only by its own tests.

## Current architecture (as implemented)

```
                         TRUST (signatures decide)
Federate Root Key (ed25519, pinned via --root-key or TOFU)
  └─ signs RootZone (version, rollback-protected) and every TldRecord
       ├─ root_managed TLD: DomainRecords in the zone, signed by the official operator key
       └─ delegated TLD:    operator key signs TldRegistry (+ its DomainRecords)
            modes: delegated_manifest (hash-pinned) | delegated_native | delegated_http
       DomainRecord names the owner key, which signs the Manifest (content-addressed)
            Manifest maps path to BLAKE3 block hash; blocks verified on every read

                         DATA MOVEMENT (native first)
fed://label.tld/path -- federate-uri --> federate-resolution
  root:     memory -> disk cache -> native GetRoot -> HTTP /v1/root
  registry: (mode-routed) manifest path | native GetTldRegistry -> HTTP endpoint -> cached
  manifest: disk cache -> native GetManifest -> HTTP /v1/manifest
  block:    cache -> directory-announced native providers -> default native
            -> HTTP providers -> HTTP origin
consumers: HTTP gateway (adapter), federated daemon, CLI, DNS (TLD existence),
           search crawler

                         NODES
Node 1 (federate-server): root authority + origin + directory + native listener :4077
federate-noded: roles by config; every node: signed registration + heartbeat + /health
DNS nodes answer Federate TLDs with healthy gateway IPs from the directory
```

## Native protocol state

Implemented and served: `Hello/Welcome` (version negotiation, capabilities), `GetRoot/Root`, `GetManifest/Manifest`, `GetBlock/Block`, `GetStatus/Status`, v1 `GetTldRegistry/TldRegistry`, `GetDomainRecord/DomainRecord`, structured `Error`. Working native flows: full resolution of any domain (root-managed or delegated) with zero HTTP, proven by tests and live runs. Fallbacks: every fetch layer falls back to HTTP compatibility endpoints. Still HTTP-only: node directory (registration, health checks, provider lists, announcements), bootstrap discovery (one `/v1/bootstrap` call), delegated_http registries by design, search API, daemon API. Defined but inert: `GetProviders`. Docs-only: QUIC, signed handshakes, push/subscribe, binary encoding.

## TLD and registry state

- 23 official TLDs seeded root-managed; 1 delegated seed (`.femboy`, delegated_manifest, own operator key, expiry 2027).
- Blocked: full IANA list (`blocked_tlds.txt`); reserved: 8 names; policy/brand-safety lists exist but are empty.
- Operators CAN operate their own TLD today: sign records, build/verify the registry, serve it from their own noded (`delegated_native`), all off-root. Live-verified.
- Limit: getting a delegation created still means editing federate-server seed code (`SEED_DELEGATED_TLDS`) and restarting. No application/approval flow.
- Domain issuing: real cryptographically (operator tooling), dev-only administratively (official-TLD domains come from Node 1's `sites/` scan with a single dev owner key).

## Trust and signatures: verified checklist

All genuinely implemented and covered by tests: root zone signature (+ trust-anchor mismatch rejection), TLD record signature, domain record signature (operator key, TLD-consistency), delegated registry signature (+ cross-TLD smuggling rejection), manifest signature (owner key + domain match + content address), block hash verification (fetch and cache read, eviction on tamper), root zone rollback, delegated registry rollback, key pinning (flag/TOFU persisted; cached registries re-verified against the CURRENT operator key), expiration fail-closed everywhere, status gating everywhere. Missing: revocation of keys (only records/statuses), rotation, nonce-protected mutation APIs (none exist yet; docs mark them as a MUST before mutation lands).

## DNS state

Real server: UDP + TCP (RFC 7766 framing), verified-zone-gated TLD matching, multiple gateway IPs (max 8, A/AAAA split), TTL 30s, upstream forwarding with connected-socket + transaction-ID checks, SERVFAIL fallback, bounded concurrency on both transports. Missing for public use: EDNS(0), any DNSSEC story for the compat door, no live deployment, the gateway list depends on a populated directory, and the "local OS resolver" integration is docs-only (users need a hosts file or DNS pointed at a Federate node).

## Gateway state

Pure adapter: HTTP Host+path becomes a `FederateUri` into the same resolver; no side-channel resolution path exists. GET/HEAD only, 2KB URI cap, hash-as-strong-ETag with 304s, `nosniff`, `no-referrer`, HTML-escaped error pages per failure class (including a security-failure page naming the failed layer). Risky remainders: plain HTTP (no TLS for Federate names; the local-CA plan is docs-only), no rate limiting, no response size shaping beyond the 64 MiB block cap, and content is whatever publishers sign (no content policy layer).

## Node system

Roles: root-authority (root-key-gated), root-mirror, dns, gateway, storage, cdn, search, bootstrap, origin. Registration signed + verified (node_id equals pubkey, IPs validated, health endpoint must point at the node's own IP). Health loop 15s, online/degraded/offline at 1/3 failures, 24h stale pruning, snapshot persistence with re-verification. Native comms: all content-plane requests; the directory plane is still HTTP. Provider discovery: per-block via signed announcements; real. Root mirrors: real (a noded role serving the locally verified zone; clients re-verify and rollback-protect). Placeholder: `/v1/peers` stub, `GetProviders` unused.

## Search

Exists (`federate-search` + searchd + noded role). Indexes every HTML file of every resolvable root-zone domain through the verifying resolver every 10 minutes; delegated domains are NOT crawled (the indexer iterates `zone.domains` only). Opt-out honored (robots/federate noindex, quote-style-proof). No ads, tracking, personalization, or AI training, by code and policy constants. Missing: persistence, delegated-TLD crawling, real ranking, pagination, a `fed.busca` frontend site, opt-out at TLD/registry level.

## Deployment

Docs and units complete on paper: hardened systemd (NoNewPrivileges, ProtectSystem=strict, UMask=0077), Caddy Host-routing with the catch-all that makes `http://home.fed` work, ufw list, port-53 freeing, key storage guidance (0600, offline root-key backup), rollback procedure, external validation checklist. Not deployed anywhere. Concrete staleness found: firewall/docs/units omit native port **4077/tcp**; the federate-server unit does not pass `--native-listen`; bootstrap `native_nodes` will be empty until nodes with native ports register.

## Tests and validation

75 test functions across 20 crates (resolution 17, registry 10, naming 6, directory 5, root 5, uri 5, dns 5, protocol 4, gateway 3, search 3, others 1-2). Strong coverage: the whole trust chain including adversarial cases (forged zones/registries/records/blocks, replay/rollback, expiry, traversal, SSRF, XSS-escaping, TCP-DNS framing abuse). Zero tests: all 7 binaries (main.rs logic like federate-server's build_store, noded role assembly, CLI handlers), federate-client, block announce/serve loops, gateway ETag-vs-live-server integration. No integration/e2e harness spawning real binaries (done manually, repeatedly, in dev).

Validation run 2026-07-06, all green:

```
cargo fmt --all --check      ok (no diff)
cargo clippy --workspace --all-targets -- -D warnings   ok
cargo test --workspace       75 passed, 0 failed
cargo build --release        ok
```

## Reality check

| Area | Real | Partial | Stub | Docs only | Notes |
|---|---|---|---|---|---|
| root/TLD registry | x | | | | signed, verified, rollback-protected; mutation = restart |
| delegated TLDs | x | | | | 4 modes, fail-closed, cached, rollback; delegation creation is seed-code |
| native protocol | x | | | | v1; GetProviders inert; directory plane not native |
| native transport | x | | | | framed TCP with limits; QUIC docs-only |
| DNS | x | | | | UDP+TCP, multi-gateway, forwarding; EDNS missing, not deployed |
| gateway | x | | | | pure adapter; no TLS/rate-limit |
| node directory | x | | | | HTTP-only plane; /v1/peers stub |
| storage/CDN | | x | | | cache+announce+rank real; replication/pinning absent |
| search | | x | | | works, in-memory, official TLDs only, no frontend |
| publishing/deploy | | x | | | operator/owner tooling real; official-TLD publish = Node 1 filesystem; zero live deploys |
| TLD operator tooling | x | | | | sign/build/verify/serve, live-verified |
| abuse/enforcement | | | x | | creation-time blocklists only; no runtime mechanism |
| payments/transactions | | | x | | placeholder JSON fields |
| native browser | | | | x | |
| non-HTML runtime | | | | x | |
| public deployment | | | | x | docs complete but stale (port 4077); never executed |

## Biggest gaps (top 10)

1. **Runtime mutation of the root registry**: everything requires restarting Node 1. Blocks delegation issuance, status changes, official-domain publishing. State: startup-only build_store. Next: persistent signed registry state + admin-signed mutation API with nonce/challenge (docs already mandate this). **Large.**
2. **Official-TLD publishing path**: the `sites/` scan + dev owner key means only Node 1's filesystem publishes. State: owner tooling exists but Node 1 cannot ingest a package. Next: an ingest endpoint (or native message) accepting a site package + operator-signed record, verified before acceptance. **Medium.**
3. **Key rotation/revocation**: a single static root key and operator keys forever; a leaked key is game over. State: docs-only. Next: cross-signed transition records for root and operators, honored by resolvers. **Large.**
4. **No public deployment**: nothing validated outside localhost; deploy docs stale (port 4077). State: docs+units ready-ish. Next: one real Hetzner deploy following the doc, fixing it (add 4077, native-listen) as found. **Small-medium.**
5. **Replication/pinning**: content survives only where cached; if the origin dies, uncached content dies. State: fetch-on-miss CDN only. Next: pinning sets + replication targets per manifest, using existing announcements. **Medium.**
6. **Directory/discovery still HTTP + single point**: one directory on Node 1; native plane absent; `GetProviders` unused. Next: serve provider queries natively and allow multiple directories. **Medium.**
7. **Binary-level integration tests**: 7 binaries untested; regressions in main.rs wiring are only caught manually. Next: a spawn-binaries e2e test (server + noded + CLI fetch) in CI. **Medium.**
8. **Delegated content in search + search persistence**: delegated domains are invisible in `.busca`; the index is lost on restart. Next: crawl via registry enumeration; persist the index. **Small.**
9. **TLS story for Federate names**: a plain-HTTP browser door; modern browsers are increasingly hostile to it. State: local-CA plan docs-only. Next: decide local-CA vs native-client-first, prototype accordingly. **Large.**
10. **Non-HTML/native content model**: the manifest is a file tree with extension-guessed MIME; the runtime is browser HTML. State: roadmap doc. Next: typed content metadata in the manifest (optional field, signature-compatible) as the boundary for a future runtime. **Medium.**

## Recommended next steps

**Single best next step: gaps 1+2 together, a persistent, runtime-mutable root registry with a signed ingest/mutation API.** Everything else queues behind it: delegation issuance, real publishing, marketplace, and a Node 1 that does not rebuild the universe from `sites/` on every restart. It also forces the nonce/replay design the docs already require. Concretely: persist zone + registries to disk as the source of truth, add operator/owner-signed mutation requests (publish site package, update record, delegate TLD) with challenge-response, keep startup seeding only as first-boot bootstrap.

Next three after that:

1. First real VPS deployment (gap 4), fixing the stale deploy docs (port 4077, `--native-listen`) as executed.
2. Binary-level e2e test harness in CI (gap 7) so the deploy target stays honest.
3. Key rotation records for root + operators (gap 3) before any key matters in production.
