# Protocolo Federate (MVP)

> [English version](../en-US/protocol.md)

## Zona raiz

JSON estruturado, serializável, cacheável e assinável, servido em `GET /v1/root`:

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

`.fed`, `.pagina`, `.rosa`, `.cara`, `.mosca`, `.busca`, `.types`, definidos em `federate-naming`.

## Registros de domínio

Domínios resolvem para identidades, não para IPs:

```
domain → domain record → manifest hash → content hashes → content blocks
```

`owner` e `nodes` são campos reservados para futuras identidades de serviço/nó.

## Manifests

Todo site tem um. Endereçado por conteúdo via hash BLAKE3 dos seus bytes JSON,
servido em `GET /v1/manifest/{hash}`:

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

Mapeamento de caminhos: `/` → entry; `/x` → `x` ou `x/index.html`; `/x/` → `x/index.html`.

## Blocos de conteúdo

Cada arquivo estático é um bloco endereçado por conteúdo (BLAKE3 em hex),
servido em `GET /v1/block/{hash}`. Os clientes DEVEM verificar o hash de todo
manifest e bloco baixado antes de usá-lo, e reverificar os blocos na leitura
do cache.

## API do Node 1

| Endpoint | Finalidade |
|---|---|
| `GET /health` | liveness |
| `GET /v1/status` | status do nó |
| `GET /v1/bootstrap` | metadados de bootstrap (url/versão/chave da raiz; futuros mirrors + nós de bootstrap) |
| `GET /v1/root` | zona raiz |
| `GET /v1/tlds` | definições de TLD |
| `GET /v1/domain/{fqdn}` | registro de um único domínio |
| `GET /v1/manifest/{hash}` | manifest assinado |
| `GET /v1/block/{hash}` | bloco de conteúdo |
| `GET /v1/nodes` | descoberta de nós (stub - fase 4) |
| `GET /v1/peers` | descoberta de peers/CDN (stub - fase 5) |

## API local do daemon (`127.0.0.1:7777`)

`GET /health`, `GET /status`, `GET /resolve?domain=&path=`, `GET /root`,
`GET /cache/list`, `DELETE /cache/clear`.
