# The Federate TLD Hierarchy

> [Versão em português (pt-BR)](../pt-BR/tld-hierarchy.md)

Federate Network runs a parallel web namespace with its own root. The root
layer does not sell every domain directly forever; it controls **which TLDs
exist**, which are reserved or blocked, and **who operates** each delegated
TLD.

```
Federate Root Registry      controls TLDs (exists? official? delegated? blocked?)
  → TLD Operator            controls domains inside that TLD
    → Domain Registrant     owns a specific domain
      → Site/Manifest Owner controls the site's signed manifest and content
```

These layers are never collapsed into one flat registry.

## Federate Root Registry

Controlled by Federate Network, initially served by Node 1 at
`federate.network`. It maintains: active/official/delegated/reserved/blocked/
disabled/pending/expired/revoked TLDs, the IANA blocklist, Federate reserved
names, policy and brand-safety blocklists, TLD ownership records, operator
public keys, registry endpoints/manifests, policies, expiration/renewal
metadata, pricing metadata placeholders, and audit events.

It answers: does this TLD exist? what status? who owns it? who operates it?
where is its registry? is it allowed to exist? does it collide with public
ICANN/IANA DNS? is it blocked for safety/legal/governance reasons?

## TLD statuses

| Status | Meaning |
|---|---|
| `official` | Operated by Federate Network itself (`.fed .pagina .rosa .cara .mosca .busca .types`) |
| `delegated` | Operated by a user/operator (e.g. `.femboy`) |
| `reserved` | Cannot be purchased - infrastructure/governance/safety/future use (`root`, `admin`, `registry`, …) |
| `blocked` | Cannot be created - public DNS collision, brand, phishing, policy (`com`, `net`, `dev`, `app`, …) |
| `disabled` | Exists but temporarily not resolvable |
| `pending` | Application exists, not approved |
| `expired` | Ownership/lease expired |
| `revoked` | Removed by root governance (abuse, nonpayment, legal, emergency) |

## Roles and keys

- **TLD owner** (`owner_public_key` on the TLD record): the economic/legal
  owner of the TLD.
- **TLD operator** (`operator_public_key`): the key authorized to run the
  TLD's registry and sign domain records under it. Often the same key as the
  owner, but separable (an owner can hire an operator).
- **Domain registrant/owner** (`owner_public_key` on the domain record): the
  key authorized to publish/update that domain's manifest.
- **Site/manifest owner**: signs the manifest; must be the domain owner key.

Example: Federate delegates `.femboy` to a user. That user becomes the TLD
Operator. Other users then register `eu.femboy`, `joao.femboy`, `wiki.femboy`
- issued by the `.femboy` operator, **not** by Federate. Federate's root only
records that `.femboy` exists, who owns/operates it, what registry it uses,
and whether it is active.

## Why this avoids collisions with the normal internet

Every TLD creation is validated against `blocked_tlds.txt`: the full public
IANA/ICANN TLD list. `.com`, `.net`, `.org`, `.br`, `.dev`, `.app`, `.live`,
`.page`, `.google`, `.bank`, `.gov`, … can never exist inside Federate. A
Federate name therefore never shadows a real internet name, and the future
local DNS resolver can safely forward everything non-Federate to normal DNS.
See [blocked-tlds.md](blocked-tlds.md).

## How delegated registries will resolve (future)

Today only `root_managed` registries resolve (domains live in the signed root
zone). For a delegated TLD, the resolver already:

1. finds the TLD record in the root zone,
2. confirms it is delegated and active,
3. reads its `registry_type` / `registry_endpoint` / `registry_manifest_hash`,
4. …and stops with a structured `DelegatedRegistryNotImplemented` error and a
   clear error page.

In phase 6 the resolver will fetch domain records from the operator's registry
(`delegated_http` endpoint, or a signed `delegated_manifest`), verify each
record against the operator key authorized in the root-signed TLD record, and
continue down the normal manifest/content path. The chain of trust never
changes: root signs the TLD record; the operator key named there signs domain
records. See [signatures.md](signatures.md) and
[tld-marketplace-roadmap.md](tld-marketplace-roadmap.md).
