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
| `federate` | CLI - status, doctor, resolve, cache, node/dns/gateway/directory tools |
| `federate-dnsd` | DNS node (UDP+TCP 53) - answers Federate TLDs with healthy gateway IPs |
| `federate-gatewayd` | Public gateway node - serves verified sites to browsers |
| `federate-noded` | Multi-role node - gateway/dns/storage/cdn/search/root-mirror from one config |
| `federate-searchd` | Search node - no ads, no tracking, no AI training |

## Quick start (local dev)

```sh
cargo build --release
./target/release/federate-server --listen 127.0.0.1:9000 &          # Node 1 (dev)
sudo ./target/release/federated --bootstrap http://127.0.0.1:9000    # daemon on port 80
```

Add hosts-file mappings ([hosts-setup.md](docs/en-US/hosts-setup.md)), then open **http://home.fed**.

## Docs

- [architecture.md](docs/en-US/architecture.md) - crates, layers, resolution engine
- [protocol.md](docs/en-US/protocol.md) - root zone, manifests, content addressing
- [manifesto.md](docs/en-US/manifesto.md) - why Federate exists
- [dns-resolver.md](docs/en-US/dns-resolver.md) - planned local DNS resolver
- [deployment-vps.md](docs/en-US/deployment-vps.md) - deploying Node 1
- [desktop-setup.md](docs/en-US/desktop-setup.md) - friend onboarding
- [hosts-setup.md](docs/en-US/hosts-setup.md) - hosts-file mappings
- [port-80-setup.md](docs/en-US/port-80-setup.md) - portless URLs
- [https-local.md](docs/en-US/https-local.md) - internal HTTPS / local CA plans
- [tld-hierarchy.md](docs/en-US/tld-hierarchy.md) - root registry, TLD operators, delegation
- [signatures.md](docs/en-US/signatures.md) - chain of trust, canonical signing
- [blocked-tlds.md](docs/en-US/blocked-tlds.md) - IANA/reserved/policy blocklists
- [tld-marketplace-roadmap.md](docs/en-US/tld-marketplace-roadmap.md) - future marketplace phases
- [troubleshooting.md](docs/en-US/troubleshooting.md)

## TLDs

`.fed` official · `.pagina` personal sites · `.rosa` creative spaces · `.cara` identity · `.mosca` weird internet · `.types` typography

## Roadmap

1. **Phase 1 (this repo)**: Node 1, local daemon, hosts-file setup, internal root, five TLDs, static sites, normal browser access.
2. Publishing: `federate deploy ./dist --domain example.pagina`
3. Real local DNS resolver, automatic OS integration, no manual hosts edits.
4. Friend nodes, peer discovery, user-hosted content.
5. Replication, pinning, distributed cache/CDN, nearest-node selection.
6. Registry UI, domain ownership, TLD applications, governance.
7. Desktop installer, local Federate Root CA, HTTPS for internal domains.
8. Mobile clients.

## License

[MIT](LICENSE)
