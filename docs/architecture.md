# Architecture

> [Versão em português (pt-BR)](pt-BR/architecture.md)

## Layers

Resolution is deliberately **not** baked into the HTTP gateway. The central
engine (`federate-resolution`) is reused by every current and future consumer:

```
                    ┌─────────────────────────┐
  browser ──────────▶ federate-gateway (HTTP) │──┐
  CLI/desktop ──────▶ federated local API     │──┤
  future DNS ───────▶ federate-dns (boundary) │──┼──▶ federate-resolution
  future publish ───────────────────────────────┤        │
  future peer/CDN ──────────────────────────────┘        ▼
                                              federate-root / naming / manifest
                                                         │
                                              federate-client ──▶ Node 1
                                              federate-storage ──▶ local block cache
```

## Separated concerns → crates

| Concern | Crate |
|---|---|
| 1. Root zone loading/caching | `federate-root` |
| 2. TLD validation | `federate-naming` |
| 3. Domain record resolution | `federate-resolution` |
| 4. Manifest fetching/caching | `federate-resolution` + `federate-manifest` |
| 5. Content block fetching/caching | `federate-storage` + `federate-client` |
| 6. Hash verification | `federate-storage` (BLAKE3, verified on fetch AND on cache read) |
| 7. HTTP gateway serving | `federate-gateway` |
| 8. Future DNS resolver | `federate-dns` (boundary crate, see dns-resolver.md) |
| 9. Future peer/CDN discovery | Node 1 `/v1/nodes`, `/v1/peers` stubs + `nodes` field on `DomainRecord` |

Plus: `federate-core` (types/errors/config), `federate-identity` (Ed25519 keys),
`federate-client` (Node 1 API client), `federate-cli`, `federated` (daemon),
`federate-server` (Node 1).

## Resolution flow

```
Host: home.fed, Path: /
  → FederateDomain::parse       (naming: TLD validation)
  → Resolver.root()             (memory → disk cache → Node 1)
  → RootZone.lookup("home.fed") (domain record: manifest hash, NOT an IP)
  → Resolver.manifest(hash)     (cache → Node 1, hash-verified)
  → Manifest.resolve_path("/")  ("/" → entry file → content hash)
  → Resolver.block(hash)        (block cache → Node 1, hash-verified)
  → gateway serves bytes with guessed MIME
```

Domain records resolve to **identities** (manifest hash today; owner, service,
and node identities are placeholder fields for later phases), never directly
to public IPs.

## Offline resilience

Root zone, manifests, and blocks are all cached on disk. When Node 1 is
unreachable, previously visited sites keep working from cache.

## Why DNS alone is not enough

A DNS resolver only answers *where a name should go* (for Federate: 127.0.0.1).
The daemon/runtime still handles root zone validation, domain record
resolution, manifests, content hashes, cache, peer discovery, CDN, replication,
node identity, publishing, and serving content to the browser. See
[dns-resolver.md](dns-resolver.md).
