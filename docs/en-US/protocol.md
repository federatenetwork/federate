# Federate Protocol (MVP)

> [Versão em português (pt-BR)](../pt-BR/protocol.md)

## Root zone

Structured, serializable, cacheable, signable JSON served at `GET /v1/root`:

```json
{
  "network": "federate",
  "root_version": 1,
  "generated_at": "2026-07-03T…",
  "tlds": [{ "name": "fed", "purpose": "…" }, …],
  "domains": {
    "home.fed": {
      "domain": "home.fed",
      "manifest_hash": "<blake3>",
      "owner": "<node id>",
      "nodes": [],
      "updated_at": "…"
    }
  },
  "signature": "<ed25519 hex, MVP placeholder>",
  "root_key": "<root verifying key hex>"
}
```

## TLDs

`.fed`, `.pagina`, `.rosa`, `.cara`, `.mosca`, `.busca`, `.types`, defined in `federate-naming`.

## Domain records

Domains resolve to identities, not IPs:

```
domain → domain record → manifest hash → content hashes → content blocks
```

`owner` and `nodes` are placeholders for future service/node identities.

## Manifests

Every site has one. Content-addressed by the BLAKE3 hash of its JSON bytes,
served at `GET /v1/manifest/{hash}`:

```json
{
  "domain": "home.fed",
  "version": 1,
  "entry": "index.html",
  "files": { "index.html": "<blake3>", "style.css": "<blake3>" },
  "owner": "<placeholder>",
  "signature": "<placeholder>"
}
```

Path mapping: `/` → entry; `/x` → `x` or `x/index.html`; `/x/` → `x/index.html`.

## Content blocks

Each static file is one content-addressed block (BLAKE3 hex), served at
`GET /v1/block/{hash}`. Clients MUST verify the hash of every downloaded
manifest and block before use, and re-verify blocks on cache read.

## Node 1 API

| Endpoint | Purpose |
|---|---|
| `GET /health` | liveness |
| `GET /v1/status` | node status |
| `GET /v1/bootstrap` | bootstrap metadata (root url/version/key; future mirrors + bootstrap nodes) |
| `GET /v1/root` | root zone |
| `GET /v1/tlds` | TLD definitions |
| `GET /v1/domain/{fqdn}` | single domain record |
| `GET /v1/manifest/{hash}` | signed manifest |
| `GET /v1/block/{hash}` | content block |
| `GET /v1/nodes` | node discovery (stub - phase 4) |
| `GET /v1/peers` | peer/CDN discovery (stub - phase 5) |

## Daemon local API (`127.0.0.1:7777`)

`GET /health`, `GET /status`, `GET /resolve?domain=&path=`, `GET /root`,
`GET /cache/list`, `DELETE /cache/clear`.
