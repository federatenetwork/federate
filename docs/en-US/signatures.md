# Signatures & Chain of Trust

> [Versão em português (pt-BR)](../pt-BR/signatures.md)

## Keys in one paragraph

A keypair has a **private key** (secret, proves ownership by producing
signatures) and a **public key** (a public identifier, safe to publish -
knowing it lets anyone *verify* signatures but never *create* them). In
Federate, public keys ARE the identity layer: TLD owners, TLD operators, and
domain owners are all public keys. Ownership of anything is proven by a
signature from the matching private key.

## The chain of trust

```
Federate Root Key
  → TLD Record        (signed by the Root Key)
    → TLD Registry    (delegated TLDs only: signed by the TLD operator key)
      → Domain Record (signed by the TLD operator key named in the TLD record)
        → Site Manifest (signed by the domain owner key named in the domain record)
          → Content Blocks (verified by BLAKE3 hash listed in the manifest)
```

For root-managed TLDs the registry layer is the signed root zone itself; for
delegated TLDs it is a separate operator-signed registry document (see
[tld-hierarchy.md](tld-hierarchy.md)). Either way, each layer's signer is
named by the layer above it, so no layer can forge the one below.

Node 1 is a **distributor of signed data, not a trusted authority**. The
daemon trusts valid signatures and content hashes, never server responses.
A compromised or impersonated Node 1 cannot forge any record without the
corresponding private keys; the daemon rejects the data and keeps serving the
last verified cached zone.

### 1. Federate Root Key

Top authority. Signs the root zone and every TLD record (official and
delegated). The **public** root key is configured in `federated` via
`--root-key <hex>`, or pinned on first use (TOFU) and persisted to
`<data-dir>/trusted-root-key`. The **private** root key lives only on the
root registry host (`.federate-server/root/identity.key` in dev) and is never
embedded in the daemon or exposed by any API.

### 2. TLD records

Signed by the Root Key. The daemon rejects unsigned or invalidly signed TLD
records.

### 3. TLD registries (delegated TLDs)

Signed by the TLD operator key named in the root-signed TLD record. The
verifier checks: the registry is for the expected TLD; the claimed signer
equals the authorized operator key; the signature verifies; every entry
inside actually belongs to that TLD (a registry for `.a` cannot smuggle
records for `.b`). Live registries also carry a monotonic `version` used
for rollback protection. A registry that fails any check is discarded
entirely; unreachable is not the same as invalid (unreachable falls back to
the last verified cache, invalid fails closed).

### 4. Domain records

Signed by the TLD operator key. Before continuing, the daemon verifies: the
TLD exists; its record verifies against the Root Key; the domain record's
signature matches the operator key authorized in that TLD record; the domain
is `active`; the domain actually belongs to that TLD. Identical rules for
root-managed and delegated domains; only where the record is stored differs.

### 5. Site manifests

Signed by the domain owner key. The daemon verifies: the domain record is
valid; the fetched manifest bytes hash to the `manifest_hash` in the domain
record; the manifest's signature is valid; the signer equals the domain
record's `owner_public_key`; the manifest's `domain` equals the requested
domain.

### 6. Content blocks

Verified by hash: fetched bytes must match the hash in the manifest, and
cached bytes are re-hashed before every serve. Invalid blocks are rejected
and evicted from cache.

## Canonical signing format

Algorithm: **Ed25519**, signatures hex-encoded. Signed payloads are
canonicalized before signing so signatures never depend on formatting:

- serialize the record to JSON with the `signature` field set to `null`
  removed / `None` (the field is excluded from the signed payload),
- canonical form = **compact JSON** (no whitespace) with **object keys sorted
  lexicographically at every nesting level**; arrays keep their order,
- sign the resulting bytes; store the hex signature in `signature`.

Never sign pretty-printed or insertion-ordered JSON. Implementation:
`federate_core::canonical::canonical_bytes` + each record's
`signable_bytes()`.

Every signed object carries: the payload fields, `signature`,
`signature_algorithm` (`"ed25519"`), the relevant signer public key
(`root_public_key` / `operator_public_key` / `owner_public_key`), and
`created_at` / `updated_at` timestamps plus version fields.

## Replay protection

Enforced, not advisory:

- **Root zone rollback**: daemons remember the `root_version` of the last
  verified zone (memory + disk cache) and reject a correctly signed but
  *older* zone from any node or mirror. Node 1 derives `root_version` from
  the clock at signing time, so it is monotonic across restarts.
- **Delegated registry rollback**: the same rule for live delegated TLD
  registries. The last verified registry is cached per TLD; a correctly
  signed but *older* registry from any provider is rejected, so a host or
  mirror cannot rewind a delegated namespace.
- **Record expiry**: `expires_at` (RFC 3339) on TLD and domain records is
  checked at every resolution (gateway, DNS, registry view, delegated
  fetch). An expired record stops resolving even though its signature is
  still cryptographically valid; an unparseable `expires_at` counts as
  expired (fail closed).

Future mutation APIs (registering/updating TLDs and domains at runtime) MUST
additionally use server-issued nonces or challenge-response so a captured
signed request cannot be replayed.

## When verification fails

The daemon serves a styled **Federate security error page** stating which
layer failed (root / tld / tld-registry / domain / manifest / content), for which domain,
and why, and does not serve the content. `federate doctor`,
`federate root verify`, `federate tld verify <tld>`, `federate domain verify
<domain>`, `federate delegated-registry verify <tld>`, and
`federate manifest verify <domain>` reproduce each check from the command
line.

## Future work

- **Key rotation**: root and operator key rollover via cross-signed
  transition records.
- **Recovery keys**: pre-registered secondary keys for owner account
  recovery.
- **Multisig**: expensive/high-value TLDs guarded by m-of-n signatures.
- **Root mirrors**: multiple mirrors distributing the same root-signed zone
  (signatures make mirrors trustless).
