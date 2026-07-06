# Decentralization model

> [Versão em português (pt-BR)](../pt-BR/decentralization.md)

Federate Network separates **authority** from **infrastructure**.

## Core principle

> Federate Root decides what is valid.
> Federate nodes distribute, resolve, cache, mirror, search, and serve valid data.

`federate.network` (Node 1) remains the official root authority for TLDs.
Everything else (DNS, gateways, storage, CDN, search, bootstrap, root
mirroring) can be run by anyone.

## What is decentralized today

| Function | Who can run it | Binary / role |
|---|---|---|
| DNS resolution | anyone | `federate-dnsd` / `dns` |
| HTTP gateways | anyone | `federate-gatewayd` / `gateway` |
| Block storage | anyone | `federate-noded` / `storage` |
| CDN / caching | anyone | `federate-noded` / `cdn` |
| Search | anyone | `federate-searchd` / `search` |
| Bootstrap | anyone | `federate-noded` / `bootstrap` |
| Root zone mirroring | anyone | `federate-noded` / `root-mirror` |
| Origin hosting | domain owners | `federate-noded` / `origin` |

## What is NOT decentralized yet

TLD authority. The Federate Root Key alone controls and signs:

- official TLDs
- delegated TLDs (and their operator keys)
- blocked TLDs
- reserved TLDs
- TLD ownership
- TLD operator keys

Why: a namespace needs a single, consistent, abuse-resistant source of truth
before it can be safely federated further. Because every TLD record is
*signed* by the root key, no node (mirror, DNS, gateway, or Node 1 itself)
can forge or alter TLD data without detection. Decentralizing the transport
first, and governance later, keeps the network honest at every step.

## Chain of trust

```
Federate Root Key
  └─ signs TLD records (root zone)
       └─ TLD Operator Key signs domain records
            └─ Domain Owner Key signs manifests
                 └─ manifests map paths → BLAKE3 content hashes
                      └─ every block is hash-verified
```

Rules enforced everywhere in code:

- all root data must be signed by the Federate Root Key
- all TLD records must be signed by the Federate Root Key
- all domain records must be signed by the TLD Operator Key
- all manifests must be signed by the Domain Owner Key
- all blocks must be verified by hash
- all node registrations must be signed by the node key
- nodes are never trusted blindly - verification happens at the consumer

## How data flows

```
browser ──DNS──▶ federate-dnsd ──▶ node directory (healthy gateways)
browser ──HTTP──▶ gateway node ──▶ resolution engine
                                     ├─ signed root zone (Node 1 or mirror)
                                     ├─ signed manifests
                                     └─ blocks (CDN/storage/origin providers, Node 1 fallback)
```

Browsers only ever talk to gateways. Gateways understand Federate manifests,
signatures, blocks, and replicas.

## Running your own node

See:

- [nodes.md](nodes.md) - node roles, registration, config
- [dns-nodes.md](dns-nodes.md)
- [gateway-nodes.md](gateway-nodes.md)
- [storage-cdn-nodes.md](storage-cdn-nodes.md)
- [root-mirrors.md](root-mirrors.md)
- [node-directory.md](node-directory.md)
