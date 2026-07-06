# Running a Federate node

> [Versão em português (pt-BR)](pt-BR/nodes.md)

Anyone can run Federate infrastructure. A node is identified by an Ed25519
keypair generated on first run; its public key is its `node_id`.

## Roles

| Role | What it does |
|---|---|
| `root-authority` | Signs TLD records and the root zone. **Only the official Federate root can run this** - the directory rejects the role from any other key. |
| `root-mirror` | Serves signed root zone copies. Cannot create or modify TLDs. |
| `dns` | Answers Federate DNS queries with healthy gateway IPs; forwards everything else upstream. |
| `gateway` | Receives browser HTTP requests and serves verified Federate sites. |
| `storage` | Stores content-addressed blocks. |
| `cdn` | Caches popular blocks (LRU, size-capped) and serves them fast. |
| `search` | Indexes public pages (no ads, no tracking, no AI training, opt-out honored). |
| `bootstrap` | Helps new nodes discover the network. |
| `origin` | Hosts original content for a domain/site. |

## Config file

Every node reads a TOML config (default `federate.toml`):

```toml
[node]
roles = ["gateway", "cdn"]
region = "br-sp"
public_ip = "x.x.x.x"
# listen = "0.0.0.0:8080"       # HTTP service (health, blocks, gateway…)
# dns_listen = "0.0.0.0:5353"   # DNS, UDP+TCP (dns role only)

[network]
bootstrap = "https://federate.network"
root_key = "..."                 # pin the Federate Root public key (recommended)
# directory = "https://federate.network"  # node directory (defaults to bootstrap)
# upstream_dns = "1.1.1.1:53"

[capacity]
storage_gb = 100
bandwidth_mbps = 500
```

## Run it

```sh
federate-noded --config federate.toml
# or override roles:
federate-noded --config federate.toml --roles gateway,dns,cdn
# or via the CLI:
federate node run --roles gateway,dns,cdn
```

Dedicated single-role daemons also exist: `federate-dnsd`,
`federate-gatewayd`, `federate-searchd`.

## Registration

On startup a node signs a registration with its private key and sends it to
the node directory, then re-registers every 60s (heartbeat). The registration
contains: `node_id`, `public_key`, `roles`, public IPs, `region`, `version`,
`capacity`, `health_endpoint`, and the signature. Unsigned or tampered
registrations are rejected.

```sh
federate node register --config federate.toml
```

## Health API

Every node exposes:

- `GET /health` → `ok`
- `GET /status` → node_id, roles, region, version, capacity, uptime
- `GET /roles` → roles list

The directory's health checker polls `/health` and marks nodes **online**,
**degraded** (1-2 consecutive failures), or **offline** (3+).

## CLI

```sh
federate node status  --node http://45.1.1.1:8080
federate node roles   --node http://45.1.1.1:8080
federate node health  --node http://45.1.1.1:8080
federate node list --role gateway
federate directory list --role dns --healthy
federate dns test home.fed --server 45.1.1.1:53
federate gateway test home.fed --gateway http://45.1.1.1:8080
```
