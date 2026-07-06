# Planned Local DNS Resolver

> [Versão em português (pt-BR)](../pt-BR/dns-resolver.md)

Not implemented in the MVP. The MVP uses hosts-file mappings. This document
plus the `federate-dns` crate define the boundary so DNS can be added without
touching the gateway.

## What it will do

- Listen on localhost (e.g. `127.0.0.1:53`) or an OS-configured resolver address.
- Answer Federate TLDs (every resolvable TLD in the signed root zone; 23 official today):
  - gateway mode: return `127.0.0.1` so the local `federated` gateway serves the site
  - future modes: return remote gateway IPs or local service IPs
- **Forward all other queries** to the user's normal upstream resolver -
  normal internet DNS must never break.
- Use the same resolution engine (`federate-resolution`) as the HTTP gateway
  for anything beyond name→IP (e.g. checking whether a domain exists).
- Be installed automatically by the future desktop installer (replacing manual
  hosts-file edits; roadmap phase 3).

## OS integration plans

- **macOS**: `/etc/resolver/fed` (and one per TLD) pointing at the local resolver, giving per-TLD resolution without touching global DNS.
- **Linux**: systemd-resolved routing domains (`~fed`, `~pagina`, …) via `resolvectl`, or an NSS/dnsmasq entry.
- **Windows**: NRPT rules (Name Resolution Policy Table) per TLD.

## Why DNS alone is not the Federate runtime

DNS only answers **where a name should go**. The daemon/runtime still handles:

- root zone validation
- domain record resolution
- manifests
- content hashes
- cache
- peer discovery / future CDN / replication
- node identity
- publishing
- serving content to the browser

The `federate-dns` crate currently ships the `FederateNameResolver` trait and a
`StubResolver` that pins the contract: Federate TLD → `Some(127.0.0.1)`,
everything else → `None` (forward upstream).
