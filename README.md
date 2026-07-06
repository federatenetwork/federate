# Federate Network

> [Versão em português (pt-BR)](README.pt-BR.md)

A human alternative web protocol/runtime that runs on top of the existing internet.

Normal browsers. No ports in URLs. Open `http://home.fed`.

```
domain → local Federate resolver/daemon → Federate root zone → domain record
       → signed manifest → content hashes → content blocks → browser response
```

## Components

| Binary | Role |
|---|---|
| `federate-server` | Node 1 - public bootstrap/control-plane server (root zone, manifests, blocks) |
| `federated` | Local desktop daemon - browser gateway on `127.0.0.1:80`, local API on `:7777` |
| `federate` | CLI - status, doctor, resolve, cache, open |

## Quick start (local dev)

```sh
cargo build --release
./target/release/federate-server --listen 127.0.0.1:9000 &          # Node 1 (dev)
sudo ./target/release/federated --bootstrap http://127.0.0.1:9000    # daemon on port 80
```

Add hosts-file mappings ([hosts-setup.md](docs/hosts-setup.md)), then open **http://home.fed**.

## Docs

- [architecture.md](docs/architecture.md) - crates, layers, resolution engine
- [protocol.md](docs/protocol.md) - root zone, manifests, content addressing
- [manifesto.md](docs/manifesto.md) - why Federate exists
- [dns-resolver.md](docs/dns-resolver.md) - planned local DNS resolver
- [deployment-vps.md](docs/deployment-vps.md) - deploying Node 1
- [desktop-setup.md](docs/desktop-setup.md) - friend onboarding
- [hosts-setup.md](docs/hosts-setup.md) - hosts-file mappings
- [port-80-setup.md](docs/port-80-setup.md) - portless URLs
- [https-local.md](docs/https-local.md) - internal HTTPS / local CA plans
- [tld-hierarchy.md](docs/tld-hierarchy.md) - root registry, TLD operators, delegation
- [signatures.md](docs/signatures.md) - chain of trust, canonical signing
- [blocked-tlds.md](docs/blocked-tlds.md) - IANA/reserved/policy blocklists
- [tld-marketplace-roadmap.md](docs/tld-marketplace-roadmap.md) - future marketplace phases
- [troubleshooting.md](docs/troubleshooting.md)

## TLDs

`.fed` official · `.pagina` personal sites · `.rosa` creative spaces · `.cara` identity · `.mosca` weird internet · `.tipos`/`.types` typography

## Roadmap

1. **Phase 1 (this repo)**: Node 1, local daemon, hosts-file setup, internal root, five TLDs, static sites, normal browser access.
2. Publishing: `federate deploy ./dist --domain example.pagina`
3. Real local DNS resolver, automatic OS integration, no manual hosts edits.
4. Friend nodes, peer discovery, user-hosted content.
5. Replication, pinning, distributed cache/CDN, nearest-node selection.
6. Registry UI, domain ownership, TLD applications, governance.
7. Desktop installer, local Federate Root CA, HTTPS for internal domains.
8. Mobile clients.
