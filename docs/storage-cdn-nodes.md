# Storage and CDN nodes

Storage and CDN nodes hold content-addressed blocks and serve them to
gateways. They are **never returned to browsers by DNS** — browsers talk to
gateways; gateways fetch blocks from providers.

## Roles

- `storage` — durably stores blocks it has been given and serves them at
  `GET /v1/block/:hash`.
- `cdn` — same API, but fetch-on-miss: a requested block it doesn't have is
  pulled (hash-verified) from other providers or Node 1, cached, and served.
  The cache is LRU-evicted and capped by `[capacity] storage_gb`.

Both announce their cached block hashes to the node directory every minute,
so gateways can discover them as providers (`GET /v1/providers/:hash`).

## Trust model

Providers are never trusted:

- every block a gateway fetches is re-verified against its BLAKE3 hash
- a provider returning wrong bytes is detected immediately and skipped
- the CDN cache itself re-verifies blocks on read (disk corruption is caught)

## Provider selection

Gateways rank providers: online before degraded, same region first, then
lowest latency — and fail over down the list, ending at Node 1 (origin of
official content).

## Run one

```toml
# federate.toml
[node]
roles = ["cdn"]          # or ["storage"], or ["storage", "cdn"]
region = "br-sp"
public_ip = "x.x.x.x"

[network]
bootstrap = "https://federate.network"
root_key = "..."

[capacity]
storage_gb = 100
bandwidth_mbps = 500
```

```sh
federate-noded --config federate.toml
```
