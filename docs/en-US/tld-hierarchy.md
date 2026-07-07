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
| `official` | Operated by Federate Network itself (23 TLDs today; full set in `federate-naming`) |
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

## How delegated registries resolve

Delegated TLDs resolve for real. A delegated TLD publishes a **signed TLD
registry**: a document listing every domain record the operator has issued,
signed by the operator key named in the root-signed TLD record. The
`registry_type` on the TLD record says how that registry is distributed:

| Mode | Where the registry lives | Update model |
|---|---|---|
| `root_managed` | domain records sit in the signed root zone itself | root re-signs the zone (official TLDs) |
| `delegated_manifest` | content-addressed registry manifest, pinned by `registry_manifest_hash` | operator publishes new bytes, root re-signs the TLD record with the new hash |
| `delegated_native` | native Federate registry providers in `registry_providers` (`GetTldRegistry`) | operator updates freely; clients enforce version rollback protection |
| `delegated_http` | operator HTTP endpoint in `registry_endpoint` (`/v1/tld-registry/:tld`) | compatibility twin of `delegated_native`, same signed document |

Resolving `fed://eu.femboy` (`.femboy` is the neutral dev seed delegation):

1. parse the URI; extract `eu.femboy` and `.femboy`,
2. load and verify the signed root zone against the pinned root key,
3. verify the `.femboy` TLD record (root-signed) and its status/expiry,
4. fetch the `.femboy` registry through its mode and **verify the operator
   signature** against the operator key in the TLD record,
5. find `eu.femboy` in the registry and verify the record's operator
   signature, status, and expiry,
6. continue down the unchanged manifest/content path (owner-signed
   manifest, hash-verified blocks).

Failure behavior is asymmetric on purpose:

- **signature invalid anywhere** (registry, record): fail closed, security
  error, nothing served;
- **registry unreachable**: fall back to the last verified cached registry;
  with nothing cached, a clear "delegated registry unavailable" answer;
- **TLD expired/revoked/disabled/suspended**: fail closed before the
  registry is even fetched;
- **domain simply absent**: the normal domain-not-found answer.

Live registries (`delegated_native`/`delegated_http`) carry a monotonic
`version`; clients reject a correctly signed but older registry than one
they already verified, so neither the operator's host nor a mirror can
rewind the namespace (same rollback rule as the root zone).

This is why the root controls **TLD existence but not every domain
forever**: once `.femboy` is delegated, the operator key issues and signs
domains under it without asking the root; the root's only levers are the
delegation record itself (status, expiry, revocation). Inspect and verify
the whole chain with `federate delegated-registry inspect femboy` and
`federate delegated-registry verify femboy`. See
[signatures.md](signatures.md) and
[tld-marketplace-roadmap.md](tld-marketplace-roadmap.md).
