# Federate Network: current-state report

Date: 2026-07-06 (updated 2026-07-07 with the persistent runtime-mutable registry). Base HEAD: `0981c8a` plus the uncommitted registry/mutation work described below. Factual snapshot.

## Executive summary

The repo is a working single-operator overlay network with a genuinely native protocol path and a complete, tested signature chain. Root zone, TLD records, delegated TLD registries, domain records, manifests, and content blocks are all real, signed, verified fail-closed, and resolvable end to end over the native protocol with HTTP as fallback. Operator and owner tooling exists and works from independent machines. NEW: the root registry is persistent and runtime-mutable AND the only TLD source of truth: no hardcoded TLD list exists in runtime code. A new node initializes an empty registry, TLDs arrive exclusively through `federate root seed` (external TOML data file) or signed `tld create/reserve/block/delegate` mutations, and every later change arrives as a signed, nonce-protected, versioned, audited mutation or a site package ingest, with no code edits, no recompiles, no restarts. What does NOT exist: payments, key rotation, runtime abuse-report channel, native browser, non-HTML runtime, and any actual public deployment. 105 tests, all validation commands pass.

## What is real today

- Full verified resolution chain, both doors (native `fed://` path and HTTP gateway), one engine (`federate-resolution`).
- **Persistent, runtime-mutable root registry, sole TLD source of truth** (`federate-mutation` + federate-server): durable state under `data_dir/registry/` (signed zone, delegated registries, content stores, append-only signed audit log, mutation history, per-version snapshots), fail-closed re-verification on every load, atomic writes, no private keys in records. The server never creates TLDs from code: a missing registry initializes EMPTY; `federate root init` + `federate root seed --file seeds/official-tlds.toml` (refuses populated registries; --force adds missing only) and `tld create/reserve/block/delegate` mutations are the only ways TLDs exist. Live-verified: init, seed of 23 TLDs, re-seed refusal, runtime create/reserve, publish + native fetch under a runtime-created TLD, restart preserving all of it.
- **Signed mutation API**: envelope with server-issued single-use nonce (challenge-response), 5-minute timestamp window, self-certifying BLAKE3 mutation ids persisted across restarts, per-target monotonic versions, authorization against current signed state (root / TLD operator / domain owner), domain status transition matrix, root-signed audit event per accepted mutation, strict root_version increase (client rollback protection preserved). Endpoints: POST /v1/mutations/nonce, /v1/mutations, /v1/ingest/package; GET /v1/mutations/:id, /v1/mutations/target/:kind/:id, /v1/registry/{status,audit,verify}; POST /v1/registry/snapshot.
- **Site package ingest + publishing CLI**: `federate publish package ./dist --domain x.pagina` (one step), `federate registry submit-package`, `federate domain update/suspend/reinstate`, `federate tld delegate`, `federate mutation nonce/inspect`, `federate registry status/audit/snapshot/verify`. Live-verified end to end: publish, native fetch, suspend (resolution blocked), reinstate, runtime TLD delegation, wrong-key 403, server restart with full state preserved.
- Native protocol v1 (framed JSON over TCP, port 4077): handshake with version negotiation, root/manifest/block/registry/record/status messages, served by Node 1 and noded, consumed by the resolver and the CLI. Verified live repeatedly with HTTP dead.
- Delegated TLDs: signed `TldRegistry`, 4 registry modes, fail-closed verification, per-TLD offline cache, version rollback protection. Delegations are created at runtime (`federate tld delegate`); the old `.femboy` code seed is gone.
- Operator/owner tooling: `federate site package` (blocks + owner-signed manifest, `--install` into node stores), `federate operator sign-record / build-registry / verify-registry`, noded `registry_files` serving. Verified live: one noded served registry + manifest + blocks natively with zero Node 1 involvement.
- DNS server: UDP + TCP, port-53 capable, answers Federate TLDs with up to 8 healthy gateway IPs (TTL 30s), forwards everything else upstream with spoofing guards, SERVFAIL when no gateways.
- Node directory: signed registrations (SSRF-guarded), health checking, role queries, signed block announcements, persistence across restarts, 5000-node cap, stale pruning.
- Security posture: canonical-JSON Ed25519 signing, BLAKE3 content addressing verified on fetch AND cache read, root key pinning (flag or TOFU persisted), root zone + registry rollback protection, expiry fail-closed (unparseable timestamp counts as expired), path-traversal-proof content addresses, capped reads everywhere, per-connection and inflight limits.

## What is partially implemented

- **Publishing**: real at runtime now (package ingest + signed mutations; `sites/` is first-boot seed only). Still partial administratively: official-TLD registration is first-come with no payment/identity binding, no rate limiting on the ingest endpoints, and no web UI.
- **Registry/domain lifecycle**: statuses fully enforced at resolution AND changeable at runtime via signed mutations (suspend/reinstate/revoke, TLD status changes, delegation). Still missing: application/approval workflow and payments; `tld apply` and `/v1/applications` remain stubs (`tld approve` now points at `tld delegate`).
- **CDN/storage**: LRU cache, fetch-on-miss, signed announcements, provider ranking (region + latency) all real; but no replication targets, no pinning, and directory-based provider discovery for manifests/registries uses only configured defaults, not per-hash announcements.
- **Search**: real crawler through the verifying resolver, opt-out honored, TF ranking, no ads/tracking/AI-training. But the index is in-memory only, rebuilds every 10 minutes from scratch, ranking is naive, no UI site is wired to `fed.busca` in the repo, and only root-zone (official TLD) domains are crawled.
- **Deployment**: complete docs (Hetzner-style VPS, hardened systemd units, Caddy host-routing, ufw incl. 4077/tcp, port-53 freeing, key + registry backup, rollback, explicit root init/seed steps), Dockerfile, launchd plist. Never executed against a real server.
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

## Crates and binaries (27)

**Libraries** (all production-quality, all used):

| Crate | Purpose | Status |
|---|---|---|
| federate-core | errors, config, canonical JSON signing | real; used by all |
| federate-identity | Ed25519 keys on disk (0600), sign/verify | real |
| federate-naming | TLD/label naming rules, record types, statuses, expiry (no TLD list; the set is database state) | real |
| federate-uri | `fed://` parsing, HTTP-to-URI translation | real |
| federate-protocol | wire messages v0+v1, framing, negotiation | real |
| federate-transport | framed TCP, timeouts/caps, serve loop, NodeService trait | real |
| federate-root | RootZone, TldRecord, blocklists, disk cache | real |
| federate-registry | TldRegistry (delegated), RegistryView, HTTP registry client, file loader | real |
| federate-manifest | signed site manifests, path-to-hash mapping | real |
| federate-mutation | signed mutation envelopes, nonce store, signed audit events, persistent RegistryStore (state + logs + snapshots + apply path), seed-file init | real; 30 tests incl. native-protocol e2e |
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
| federate-server | Node 1: persistent runtime-mutable registry (NEVER seeds TLDs from code; empty init + explicit seed commands), signed mutation + package ingest endpoints, serves native+HTTP, hosts the directory | real; single root authority |
| federated | local daemon: gateway :80, API :7777, native discovery | real |
| federate (CLI) | 20+ command groups incl. fetch --trace, node ping, delegated-registry, operator, site, publish, registry, mutation, root init/seed/status, domain update/suspend/reinstate, tld create/reserve/block/delegate | real; `tld apply` still informational |
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

- TLDs are DATABASE records only: no hardcoded TLD list exists in runtime code (FEDERATE_TLDS / SEED_DELEGATED_TLDS removed; a source-scan test enforces absence). The 23 official TLDs live in `seeds/official-tlds.toml` (plain data) and enter the registry only via `federate root init` + `federate root seed` or `federate tld create|reserve|block|delegate` mutations. The server initializes an EMPTY registry on a missing state dir and never creates TLDs from code.
- Blocked: full IANA list (`blocked_tlds.txt`); reserved: 8 names; policy/brand-safety lists exist but are empty.
- Operators CAN operate their own TLD today: sign records, build/verify the registry, serve it from their own noded (`delegated_native`), all off-root. Live-verified.
- Delegations are created at runtime now: `federate tld delegate` (root-key-signed mutation). `delegated_manifest` operators re-pin their registry hash with an `update_registry_pointer` mutation (operator-signed, version-monotonic). `SEED_DELEGATED_TLDS` only matters on first boot. Still no application/approval/payment flow around it.
- Domain issuing: real cryptographically AND administratively at runtime (owner-signed publish/update mutations, operator-signed issue mutations, package ingest); official-TLD registration is first-come and free in this phase. The `sites/` scan with the dev owner key is first-boot seed only.

## Trust and signatures: verified checklist

All genuinely implemented and covered by tests: root zone signature (+ trust-anchor mismatch rejection), TLD record signature, domain record signature (operator key, TLD-consistency), delegated registry signature (+ cross-TLD smuggling rejection), manifest signature (owner key + domain match + content address), block hash verification (fetch and cache read, eviction on tamper), root zone rollback, delegated registry rollback, key pinning (flag/TOFU persisted; cached registries re-verified against the CURRENT operator key), expiration fail-closed everywhere, status gating everywhere, and NOW nonce-protected signed mutation APIs (challenge-response, timestamp window, persistent replay history, per-target version rollback rejection, authorization matrix, signed audit chain with before/after state hashes, tampered-state-fails-boot). Missing: revocation of keys (only records/statuses) and key rotation.

## DNS state

Real server: UDP + TCP (RFC 7766 framing), verified-zone-gated TLD matching, multiple gateway IPs (max 8, A/AAAA split), TTL 30s, upstream forwarding with connected-socket + transaction-ID checks, SERVFAIL fallback, bounded concurrency on both transports. Missing for public use: EDNS(0), any DNSSEC story for the compat door, no live deployment, the gateway list depends on a populated directory, and the "local OS resolver" integration is docs-only (users need a hosts file or DNS pointed at a Federate node).

## Gateway state

Pure adapter: HTTP Host+path becomes a `FederateUri` into the same resolver; no side-channel resolution path exists. GET/HEAD only, 2KB URI cap, hash-as-strong-ETag with 304s, `nosniff`, `no-referrer`, HTML-escaped error pages per failure class (including a security-failure page naming the failed layer). Risky remainders: plain HTTP (no TLS for Federate names; the local-CA plan is docs-only), no rate limiting, no response size shaping beyond the 64 MiB block cap, and content is whatever publishers sign (no content policy layer).

## Node system

Roles: root-authority (root-key-gated), root-mirror, dns, gateway, storage, cdn, search, bootstrap, origin. Registration signed + verified (node_id equals pubkey, IPs validated, health endpoint must point at the node's own IP). Health loop 15s, online/degraded/offline at 1/3 failures, 24h stale pruning, snapshot persistence with re-verification. Native comms: all content-plane requests; the directory plane is still HTTP. Provider discovery: per-block via signed announcements; real. Root mirrors: real (a noded role serving the locally verified zone; clients re-verify and rollback-protect). Placeholder: `/v1/peers` stub, `GetProviders` unused.

## Search

Exists (`federate-search` + searchd + noded role). Indexes every HTML file of every resolvable root-zone domain through the verifying resolver every 10 minutes; delegated domains are NOT crawled (the indexer iterates `zone.domains` only). Opt-out honored (robots/federate noindex, quote-style-proof). No ads, tracking, personalization, or AI training, by code and policy constants. Missing: persistence, delegated-TLD crawling, real ranking, pagination, a `fed.busca` frontend site, opt-out at TLD/registry level.

## Deployment

Docs and units complete on paper: hardened systemd (NoNewPrivileges, ProtectSystem=strict, UMask=0077), Caddy Host-routing with the catch-all that makes `http://home.fed` work, ufw list, port-53 freeing, key storage guidance (0600, offline root-key backup), rollback procedure, external validation checklist. Not deployed anywhere. The previously flagged staleness is fixed: ufw list now includes **4077/tcp**, the federate-server unit passes `--native-listen 0.0.0.0:4077`, and the docs cover backing up `data/registry/` as the authoritative state; bootstrap `native_nodes` still fills only as native-port nodes register.

## Tests and validation

105 test functions across 21 crates (mutation 30, resolution 17, registry 10, naming 6, directory 5, root 5, uri 5, dns 5, protocol 4, gateway 3, search 3, others 1-2). Strong coverage: the whole trust chain including adversarial cases (forged zones/registries/records/blocks, replay/rollback, expiry, traversal, SSRF, XSS-escaping, TCP-DNS framing abuse), plus the full mutation surface: first-boot seed, restart persistence, tampered-state-fails-boot, unsigned/wrong-signer/replayed/stale/rollback rejection, owner/operator/root authorization (incl. cross-TLD denial), status transition matrix, delegated pointer rollback, a runtime-published package resolving over the NATIVE protocol through the real verifying resolver, and the TLD source-of-truth suite: empty explicit init, seed-file application, re-seed refusal, --force adds-missing-only, seed/mutation TLD persistence across restart, blocked_tlds rejection, a runtime-created TLD resolving natively with zero code change, and a source scan proving no hardcoded TLD list token exists in any crate. Zero tests: the 7 binaries' main.rs wiring (server seed/route assembly, noded roles, CLI handlers), federate-client, block announce/serve loops, gateway ETag-vs-live-server integration. No harness spawning real binaries in CI (the full publish/suspend/restart flow was verified live against a real server + CLI during development).

Validation run 2026-07-07, all green:

```
cargo fmt --all --check      ok (no diff)
cargo clippy --workspace --all-targets -- -D warnings   ok
cargo test --workspace       105 passed, 0 failed
cargo build --release        ok
```

## Reality check

| Area | Real | Partial | Stub | Docs only | Notes |
|---|---|---|---|---|---|
| root/TLD registry | x | | | | signed, verified, rollback-protected, PERSISTENT, runtime-mutable via signed mutations |
| delegated TLDs | x | | | | 4 modes, fail-closed, cached, rollback; delegation + pointer updates now runtime mutations |
| native protocol | x | | | | v1; GetProviders inert; directory plane not native |
| native transport | x | | | | framed TCP with limits; QUIC docs-only |
| DNS | x | | | | UDP+TCP, multi-gateway, forwarding; EDNS missing, not deployed |
| gateway | x | | | | pure adapter; no TLS/rate-limit |
| node directory | x | | | | HTTP-only plane; /v1/peers stub |
| storage/CDN | | x | | | cache+announce+rank real; replication/pinning absent |
| search | | x | | | works, in-memory, official TLDs only, no frontend |
| publishing/deploy | | x | | | runtime publish/ingest real (CLI + API, live-verified); still first-come/free, no UI, zero live deploys |
| TLD operator tooling | x | | | | sign/build/verify/serve, live-verified |
| abuse/enforcement | | x | | | blocklists at creation + runtime suspend/revoke/reinstate mutations; no report channel |
| payments/transactions | | | x | | placeholder JSON fields |
| native browser | | | | x | |
| non-HTML runtime | | | | x | |
| public deployment | | | | x | docs/units refreshed (4077, native-listen, registry backups); never executed |

## Biggest gaps (top 8)

Former gaps 1 (runtime mutation) and 2 (official-TLD publishing path) are DONE: persistent registry, signed mutation API with nonce/challenge, package ingest, publishing CLI, all tested and live-verified. Remaining:

1. **Key rotation/revocation**: a single static root key and operator keys forever; a leaked key is game over. State: docs-only. Next: cross-signed transition records for root and operators, honored by resolvers. **Large.**
2. **No public deployment**: nothing validated outside localhost. Deploy docs/units were refreshed (4077/tcp in ufw, `--native-listen` in the unit, registry backup guidance) but never executed. Next: one real VPS deploy following the doc. **Small-medium.**
3. **Replication/pinning**: content survives only where cached; if the origin dies, uncached content dies. State: fetch-on-miss CDN only. Next: pinning sets + replication targets per manifest, using existing announcements. **Medium.**
4. **Directory/discovery still HTTP + single point**: one directory on Node 1; native plane absent; `GetProviders` unused. Next: serve provider queries natively and allow multiple directories. **Medium.**
5. **Binary-level integration tests**: 7 binaries untested; regressions in main.rs wiring are only caught manually. Next: a spawn-binaries e2e test (server + noded + CLI publish + fetch) in CI. **Medium.**
6. **Delegated content in search + search persistence**: delegated domains are invisible in `.busca`; the index is lost on restart. Next: crawl via registry enumeration; persist the index. **Small.**
7. **TLS story for Federate names**: a plain-HTTP browser door; modern browsers are increasingly hostile to it. State: local-CA plan docs-only. Next: decide local-CA vs native-client-first, prototype accordingly. **Large.**
8. **Non-HTML/native content model**: the manifest is a file tree with extension-guessed MIME; the runtime is browser HTML. State: roadmap doc. Next: typed content metadata in the manifest (optional field, signature-compatible) as the boundary for a future runtime. **Medium.**

New, created by this phase (smaller): rate limiting on the nonce/mutation/ingest endpoints before public exposure; snapshot/audit-log retention policy; a marketplace/application flow on top of the now-working mutations.

## Recommended next steps

The previous "single best next step" (persistent, runtime-mutable root registry with a signed ingest/mutation API) is DONE: seed is first-boot only, `data_dir/registry/` is the source of truth, publishing/delegation/enforcement are signed nonce-protected audited mutations, and everything old (seed sites, delegated resolution, native fetch, HTTP gateway) still works. See docs/en-US/root-registry.md, mutations.md, publishing.md, security.md (with pt-BR twins).

Next three:

1. First real VPS deployment (gap 2), now that the deploy docs/units carry 4077 + `--native-listen` + registry backups.
2. Binary-level e2e test harness in CI (gap 5) covering server boot, publish, restart persistence, and fetch.
3. Key rotation records for root + operators (gap 1) before any key matters in production.
