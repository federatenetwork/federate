# Federate Network

> [Versão em português (pt-BR)](pt-BR/README.md)

A human alternative web protocol/runtime that runs on top of the existing internet.

Normal browsers. No ports in URLs. Open `http://home.fed`.

```
domain → local Federate resolver/daemon → Federate root zone → domain record
       → signed manifest → content hashes → content blocks → browser response
```

## Components

| Binary | Role |
|---|---|
| `federate-server` | Node 1 - root registry, root zone, manifests/blocks, node directory, bootstrap |
| `federated` | Local desktop daemon - browser gateway on `127.0.0.1:80`, local API on `:7777` |
| `federate-dnsd` | Federate DNS node - answers Federate TLDs with healthy gateway IPs (anyone can run) |
| `federate-gatewayd` | Public gateway node - serves Federate sites to browsers (anyone can run) |
| `federate-noded` | Multi-role node - gateway/dns/storage/cdn/search/bootstrap/root-mirror |
| `federate-searchd` | Search node - indexes public pages, `/v1/search` |
| `federate` | CLI - status, doctor, resolve, cache, open, node/dns/gateway/directory commands |

## Quick start (local dev)

```sh
cargo build --release
./target/release/federate-server --listen 127.0.0.1:9000 &          # Node 1 (dev)
sudo ./target/release/federated --bootstrap http://127.0.0.1:9000    # daemon on port 80
```

Add hosts-file mappings ([hosts-setup.md](hosts-setup.md)), then open **http://home.fed**.

## Docs

- [decentralization.md](decentralization.md) - what is/isn't decentralized, chain of trust
- [nodes.md](nodes.md) - running your own node, roles, config, registration
- [dns-nodes.md](dns-nodes.md) - running a Federate DNS node
- [gateway-nodes.md](gateway-nodes.md) - running a gateway node
- [storage-cdn-nodes.md](storage-cdn-nodes.md) - storage/CDN nodes
- [root-mirrors.md](root-mirrors.md) - mirroring the signed root zone
- [node-directory.md](node-directory.md) - node registration, health, discovery API
- [architecture.md](architecture.md) - crates, layers, resolution engine
- [protocol.md](protocol.md) - root zone, manifests, content addressing
- [manifesto.md](manifesto.md) - why Federate exists
- [markdown-pages.md](markdown-pages.md) - official pages as markdown + the `fed-md.js` renderer
- [dns-resolver.md](dns-resolver.md) - planned local DNS resolver
- [deployment-vps.md](deployment-vps.md) - deploying Node 1
- [desktop-setup.md](desktop-setup.md) - friend onboarding
- [hosts-setup.md](hosts-setup.md) - hosts-file mappings
- [port-80-setup.md](port-80-setup.md) - portless URLs
- [https-local.md](https-local.md) - internal HTTPS / local CA plans
- [tld-hierarchy.md](tld-hierarchy.md) - root registry, TLD operators, delegation
- [signatures.md](signatures.md) - chain of trust, canonical signing
- [blocked-tlds.md](blocked-tlds.md) - IANA/reserved/policy blocklists
- [tld-marketplace-roadmap.md](tld-marketplace-roadmap.md) - future marketplace phases
- [troubleshooting.md](troubleshooting.md)

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
