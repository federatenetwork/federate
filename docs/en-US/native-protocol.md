# The native Federate protocol

> [Versão em português (pt-BR)](../pt-BR/native-protocol.md)

The protocol Federate nodes and native clients speak to each other. The
HTTP endpoints (`/v1/root`, `/v1/block/:hash`, ...) are the compatibility
surface for browsers and plain tooling; **this protocol is the native
surface the network is built around**.

Crates:

- `federate-protocol`: messages, framing, version negotiation
- `federate-transport`: how frames travel (framed TCP today, QUIC planned)

## Session shape (v0)

Every session starts with a handshake:

```
client                          node
  | Hello {versions, node_id,     |
  |        agent}                 |
  |------------------------------>|
  |            Welcome {version,  |
  |             node_id, agent,   |
  |             capabilities}     |
  |<------------------------------|
  |  ... request/response loop ...|
```

- version negotiation picks the highest shared version; no shared version
  answers `Error { code: unsupported }` and closes
- `node_id` is the peer's public key (hex): identity, not authority. What a
  node may claim is still bounded by the signatures on the data itself.
- capabilities tell the client which requests are worth making
  (`root`, `manifests`, `blocks`, `providers`, `tld-registries`)

## Requests and responses (v0)

| Request | Response | Notes |
|---|---|---|
| `GetRoot` | `Root { zone_json }` | receiver MUST verify the zone signature against its pinned root key |
| `GetManifest { hash }` | `Manifest { hash, bytes }` | receiver MUST verify bytes hash to the content address |
| `GetBlock { hash }` | `Block { hash, bytes }` | receiver MUST verify bytes hash to the content address |
| `GetProviders { hash }` | `Providers { hash, nodes_json }` | advisory; fetched blocks are hash-verified anyway |
| `GetStatus` | `Status { roles, region, root_version, ... }` | diagnostics |
| anything | `Error { code, detail }` | `unsupported`, `not-found`, `bad-request`, `unavailable` |

Added in **v1** (delegated TLD registries; sent only on sessions negotiated
at v1 or newer):

| Request | Response | Notes |
|---|---|---|
| `GetTldRegistry { tld }` | `TldRegistry { tld, registry_json }` | receiver MUST verify the operator signature against the operator key in the root-signed TLD record |
| `GetDomainRecord { fqdn }` | `DomainRecord { fqdn, record_json }` | one operator-signed record; proves issuance, not freshness (only the full registry carries the rollback version) |

Nodes that answer these advertise the `tld-registries` capability. A v1
client on a v0 session sticks to the v0 message set; version negotiation
already handles the rest.

Planned for later versions: peer discovery exchange, signed handshakes
(proof of key possession), capability-scoped rate limits, push/subscribe for
zone updates.

## Framing and encoding (v0)

- one message = 4-byte big-endian length prefix + JSON body
- frame cap: 68 MiB (blocks are capped at 64 MiB; envelope needs headroom)
- JSON now, deliberately: trivial to debug, portable everywhere. A binary
  encoding can arrive as a later protocol version through the same
  negotiation (v1 added the delegated registry messages this way), so
  choosing JSON today costs nothing tomorrow.

## Transport

`federate-transport` is message-oriented on purpose: callers only ever see
`send(Message)` / `recv() -> Message`. Today that runs over TCP (default
port **4077**, which is 0xFED) with:

- per-operation timeouts (15s)
- per-frame size validation before allocation
- bounded concurrent connections (256) and requests per connection (10k)

QUIC/UDP is the planned second transport; because the API is
message-oriented, swapping the socket does not touch protocol logic or any
caller. IPv6 works wherever the bind address is IPv6 (tokio dual-stack).

## Trust model

Transport carries **zero** trust. Root zones, TLD records, domain records,
manifests, and blocks are verified by the receiver against the pinned root
key and content addresses, exactly like the HTTP path. The protocol moves
bytes; signatures decide what is valid. A malicious node can refuse to
answer; it cannot forge an answer that verifies.

## The native fetch path

The whole resolution chain prefers the native protocol, not just blocks.

Root zones and manifests:

1. **local cache** (zone signature / content hash re-verified)
2. **native providers** (`GetRoot` / `GetManifest`), in configured order
3. **HTTP compatibility endpoint** of the bootstrap node, last

Delegated TLD registries (v1): `delegated_manifest` registries travel as
content-addressed manifests through the exact path above; live registries
are fetched with `GetTldRegistry` from the TLD's own `registry_providers`
first, then from its HTTP `registry_endpoint`, operator-signature verified
either way, with per-TLD offline caching and version rollback protection
(see [tld-hierarchy.md](tld-hierarchy.md)).

Content blocks:

1. **local cache** (hash re-verified on read)
2. **native providers**: directory-announced nodes that declared a
   `native_port`, best-ranked first, then the configured default native
   providers
3. **HTTP providers**: the same nodes' compatibility endpoints
4. **HTTP origin** (Node 1): compatibility fallback of last resort

A provider is an **untrusted distributor**: a forged answer fails signature
or hash verification and the fetch moves to the next provider. Failing
everything native falls back to HTTP; failing everything returns an error,
never unverified bytes. `federate fetch fed://... --trace` prints each step,
including which transport actually delivered the bytes;
`federate providers <hash>` lists announced providers and their transports.

## Discovering native peers

`/v1/bootstrap` advertises `native_port` (the answering node's own native
listener) and `native_nodes` (`host:port` of other healthy native
listeners). `federated` and `federate fetch` read that answer once and go
native for everything after; `--native-provider host:port` (daemon) and
`--provider host:port` (CLI) add providers by hand, and node configs take
`native_providers = ["host:port"]` under `[network]`. Discovery is the one
HTTP call a fresh client needs; data never depends on it, and every fetched
byte is verified regardless of who advertised the provider.

## Serving it

`federate-server` (Node 1) and `federate-noded` both listen natively
(default port `4077`) and answer `GetRoot`, `GetManifest`, `GetBlock`,
`GetTldRegistry`, `GetDomainRecord`, and `GetStatus` from the same verified
stores their HTTP routes use. The root
authority is a Federate node first; HTTP is its compatibility door. There is
one resolution engine and one set of stores; native and compatibility
surfaces are two doors into the same room.
