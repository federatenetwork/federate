# TLD Marketplace Roadmap

> [Versão em português (pt-BR)](pt-BR/tld-marketplace-roadmap.md)

Federate controls the root; TLDs will be delegated to operators; operators
sell domains inside their TLDs. **No payments are implemented today.** This
document is the plan.

## Phase 1: now (this repo)

Static official TLDs (`.fed .pagina .rosa .cara .mosca .busca .types`),
blocklists (`blocked_tlds.txt` + reserved/policy/brand-safety), root-managed
domain records, full Ed25519 chain of trust, seed example of a delegated TLD
(`.femboy`) whose resolution returns `DelegatedRegistryNotImplemented`.
Admin mutations are seed-data-only (edit blocklists / sites, restart Node 1).

## Phase 2: TLD applications and admin approval

`federate tld apply <tld>` submits a signed application (validated against
all blocklists) to Node 1; `pending` TLD records appear in the root zone;
root admin approves/rejects (`federate tld approve --owner <pk> --operator
<pk>`). Mutation APIs use signed requests with nonce/challenge replay
protection (see docs/signatures.md). Still no money.

## Phase 3: TLD purchase / payment integration

Pricing metadata becomes real; applications carry payment; expiration/renewal
enforcement begins (expired TLDs stop resolving after grace). Payment rails
deliberately unspecified until here.

## Phase 4: TLD operator dashboard

Web dashboard for operators: issue/suspend/revoke domains under their TLD,
manage operator keys, publish registry endpoints, view audit logs.

## Phase 5: domain sales inside delegated TLDs

Operators price and sell domains (`eu.femboy`) to registrants. Domain records
signed by the operator key; manifests signed by the buyer's owner key.
Registrant-facing flows in CLI + dashboard.

## Phase 6: external delegated registries

The resolver implements `delegated_http` (operator-hosted registry API) and
`delegated_manifest` (signed registry manifest distributed like content).
Verification path is already fixed: root-signed TLD record names the operator
key; every domain record must verify against it. `DelegatedRegistryNotImplemented`
disappears.

## Phase 7: federated root mirrors and signed delegation

Multiple root mirrors serve the same root-signed zone (mirrors are trustless
because everything is signed). Bootstrap metadata lists mirrors; daemons
fail over. Root key rotation and multisig for high-value TLDs land here.
