# Node directory

> [Versão em português (pt-BR)](pt-BR/node-directory.md)

The node directory tracks live Federate infrastructure nodes. Node 1 hosts
the official directory; `federate-directory` is a library, so other directory
deployments are possible later.

The directory is **infrastructure discovery only**. It never decides what
names or content are valid; that authority stays with the signed root zone.

## What it tracks per node

- `node_id` (= public key)
- `public_key`
- public IPs
- `region`
- `roles`
- health status (`online` / `degraded` / `offline`)
- `latency_ms`
- `capacity` (storage_gb, bandwidth_mbps)
- `last_seen`

## Registration

`POST /v1/nodes/register` with a signed `NodeRegistration`. The directory:

- verifies the Ed25519 signature over canonical JSON
- requires `node_id == public_key`
- rejects the `root-authority` role from any key other than the Federate
  Root Key

Nodes re-register every 60 seconds.

## Health checking

The directory polls each node's `{health_endpoint}/health` every 15 seconds:

- 200 → **online** (latency recorded)
- 1-2 consecutive failures → **degraded**
- 3+ → **offline** (excluded from healthy listings and DNS answers)

## Stale nodes, limits, persistence

- Nodes not seen for **24 hours** (no successful health check, no
  re-registration) are removed entirely, including their block-provider
  entries. They can always re-register.
- The directory tracks at most **5000 nodes**; further new registrations are
  rejected (refreshes of known nodes always succeed). Registrations are
  self-signed, so the cap bounds memory against mass fake registrations.
- Node 1 persists the node table to `data/directory-nodes.json`
  (write-then-rename). On restart every snapshot entry is re-verified -
  a tampered snapshot cannot inject unverifiable registrations.

## API

| Endpoint | Purpose |
|---|---|
| `POST /v1/nodes/register` | signed node registration |
| `GET /v1/nodes?role=gateway&healthy=true` | list nodes (healthy gateways, DNS nodes, mirrors, bootstrap…) |
| `GET /v1/nodes/:id` | one node |
| `POST /v1/nodes/announce-blocks` | **signed** storage/CDN block announcements |
| `GET /v1/providers/:hash?role=cdn` | providers for a block |

Healthy listings are sorted best-first: online before degraded, then lowest
latency. This is exactly what DNS nodes consume to answer `home.fed` with
multiple gateway IPs.

## Anti-abuse rules enforced at registration

The directory is discovery-only, but it still refuses input that could be used
to attack it or other nodes:

- **Signed registrations.** `node_id` must equal the public key, and the
  registration must be signed by that key. Tampered registrations are rejected.
- **`root-authority` is pinned.** Only the Federate Root Key may register the
  `root-authority` role; every other key is refused.
- **No SSRF via `health_endpoint`.** The endpoint must be an `http(s)` URL whose
  host is one of the node's own declared `public_ips`, and every `public_ip`
  must parse as a real IP. This stops a node from aiming the directory's health
  checker (or gateway block-fetches) at a third party or a cloud metadata IP.
- **Signed block announcements.** `POST /v1/nodes/announce-blocks` carries an
  Ed25519 signature by the announcing node; the node must already be
  registered, and malformed block hashes are dropped. Nobody can stuff another
  node's provider list. Block bytes are still re-hashed on every fetch, so a
  lying provider only wastes one request before it is skipped.

## CLI

```sh
federate directory list --role gateway
federate directory list --role dns --healthy
federate directory list --role storage
federate directory list --role cdn
```
