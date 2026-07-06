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
  (`root`, `manifests`, `blocks`, `providers`)

## Requests and responses (v0)

| Request | Response | Notes |
|---|---|---|
| `GetRoot` | `Root { zone_json }` | receiver MUST verify the zone signature against its pinned root key |
| `GetManifest { hash }` | `Manifest { hash, bytes }` | receiver MUST verify bytes hash to the content address |
| `GetBlock { hash }` | `Block { hash, bytes }` | receiver MUST verify bytes hash to the content address |
| `GetProviders { hash }` | `Providers { hash, nodes_json }` | advisory; fetched blocks are hash-verified anyway |
| `GetStatus` | `Status { roles, region, root_version, ... }` | diagnostics |
| anything | `Error { code, detail }` | `unsupported`, `not-found`, `bad-request`, `unavailable` |

Planned for later versions: peer discovery exchange, signed handshakes
(proof of key possession), capability-scoped rate limits, push/subscribe for
zone updates.

## Framing and encoding (v0)

- one message = 4-byte big-endian length prefix + JSON body
- frame cap: 68 MiB (blocks are capped at 64 MiB; envelope needs headroom)
- JSON now, deliberately: trivial to debug, portable everywhere. A binary
  encoding can arrive as protocol version 1 through the same negotiation, so
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

## Serving it

`federate-noded` listens natively on `native_listen` (default
`0.0.0.0:4077`) and answers `GetRoot`, `GetBlock`, and `GetStatus` from the
same verified stores its HTTP routes use. There is one resolution engine and
one set of stores; native and compatibility surfaces are two doors into the
same room.
