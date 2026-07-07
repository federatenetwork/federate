# Federate as an overlay network

> [Versão em português (pt-BR)](../pt-BR/overlay-network.md)

Federate is an **overlay**: a complete network built on top of the existing
internet's packet transport. The underlay (IP, routing, physical links)
stays as it is; every layer above it is Federate's own.

## Layer map

| Layer | Normal internet | Federate |
|---|---|---|
| Addressing | URLs + DNS names | `fed://` URIs ([federate-uri.md](federate-uri.md)) |
| Naming authority | ICANN/registrars | signed root zone + delegated TLD operators with signed registries ([tld-hierarchy.md](tld-hierarchy.md)) |
| Name resolution | DNS | signed-zone resolution engine (`federate-resolution`) |
| Application protocol | HTTP(S) | Federate protocol ([native-protocol.md](native-protocol.md)) |
| Trust | CAs + TLS channel trust | per-object signatures + content addressing ([signatures.md](signatures.md)) |
| Content location | origin servers | any provider; hash decides validity ([storage-cdn-nodes.md](storage-cdn-nodes.md)) |
| Discovery | search engines + ads | node directory + `.busca` (no ads/tracking/AI training) |
| Transport | TCP/QUIC | same TCP/QUIC (underlay, reused) |
| Packets, routing, physics | IP/BGP/fiber | **out of scope, reused as-is** |

The bottom rows are the point: Federate reuses packet delivery and replaces
everything people actually touch.

## Federated naming

The namespace itself is federated, not just the infrastructure. The root
signs which TLDs exist and who operates them; a delegated TLD's operator
signs its own registry and issues domains without asking the root; each
domain owner signs their own manifest. Root-managed and delegated domains
resolve through the same engine and the same verification rules; only where
the record lives differs. The root's power over a delegated TLD is exactly
the delegation record: status, expiry, revocation. It cannot forge or edit
domains inside a delegation, and it does not have to be consulted for them.

## Roles in the overlay

Every node is a first-class overlay participant
([nodes.md](nodes.md)): root-authority (only the root key), root-mirror,
dns, gateway, storage, cdn, search, bootstrap, origin. Discovery of nodes is
the directory's job; validity of data never is. That split (availability
from nodes, authority from signatures) is what lets strangers serve each
other content safely.

## Content model

Content is addressed by hash and served by whoever has it. Fetching
prefers the overlay's own transport: local cache, then native-protocol
providers, then HTTP compatibility providers, then the HTTP origin
([native-protocol.md](native-protocol.md)). Today: origin +
CDN fetch-on-miss + signed provider announcements + LRU caches. The model
extends to replication, pinning, and nearest-provider selection without new
trust decisions, because a block's identity IS its hash: replication is
pure availability engineering.

## Two doors, one network

- **Native door**: `fed://` + Federate protocol; what native clients and the
  future browser use.
- **Compatibility door**: DNS bridge + HTTP gateway
  ([browser-compatibility.md](browser-compatibility.md)); what lets any
  phone or browser reach the same content today.

Both doors end at the same resolution engine and the same verification
chain. Removing the compatibility door someday would not change the network;
removing the native core would leave nothing.
