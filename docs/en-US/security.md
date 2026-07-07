# Security model of the runtime registry

> [Versão em português (pt-BR)](../pt-BR/security.md)

This page covers the security properties of the persistent, runtime-mutable
root registry and its mutation/ingest APIs. The signature chain itself
(canonical signing, chain of trust, key pinning) is in
[signatures.md](signatures.md).

## Roles and keys

| Key | Authority |
|---|---|
| Federate Root Key | signs the zone, TLD records, audit events; delegates TLDs; emergency enforcement |
| TLD operator key | issues/updates/suspends domains inside its own TLD; moves its delegated registry pointer |
| Domain owner key | signs manifests; publishes/updates its own domain |
| Node identity key | transport identity only; no registry authority |

Private keys never appear in any persisted record, any API response, or any
mutation. The server holds the root and official-operator keys to
countersign accepted mutations; owners and delegated operators sign on
their own machines.

## Anti-replay, in layers

1. **Nonce challenge-response**: every mutation embeds a server-issued
   single-use nonce (5 minute TTL). Reuse is rejected before anything else
   runs.
2. **Timestamp window**: envelopes older than 5 minutes are rejected;
   unparseable timestamps count as expired (fail closed).
3. **Self-certifying mutation ids**: `mutation_id` is the BLAKE3 of the
   envelope; accepted ids are persisted in `mutations.jsonl`, so a replay
   is rejected even after a restart.
4. **Per-target monotonic versions**: each mutation must strictly advance
   its target's version; captured-and-resent or reordered mutations cannot
   roll a domain or TLD back.
5. **Root zone monotonicity**: every accepted mutation re-signs the zone
   with `max(previous + 1, now)`; clients keep rejecting older zones.
6. **Delegated registry versions**: a re-pinned registry must carry a
   strictly higher operator version.

## Fail-closed authorization

Authority is derived from the CURRENT signed state, never from the request:
the owner key on the existing domain record, the operator key on the
root-signed TLD record, the root key on the zone. Unknown signers, wrong
signers, cross-TLD operators, and disallowed status transitions are all
rejected with an explicit error, and nothing is partially applied: the
mutation path works on a clone and commits only after the new zone
self-verifies.

## Tamper evidence

- `state.json` is re-verified against the pinned root key on every boot; a
  tampered file stops the node.
- Delegated registries are re-verified against their operator keys on load.
- Manifests and blocks are re-checked against their content addresses on
  load and on read; corrupted entries are dropped, never served.
- Every accepted mutation appends a root-signed audit event carrying the
  BLAKE3 of the zone before and after, so the audit log chains the state
  history; `federate registry verify` re-checks all of it on demand.

## Known limitations (deliberate, documented)

- **Single root authority**: one Node 1 holds the root key. Mirrors are
  trustless for reads, but mutations have one accepting node.
- **No key rotation yet**: a leaked root/operator/owner key has no
  cross-signed rollover path (see future work in
  [signatures.md](signatures.md)). Keep offline backups; 0600 files.
- **No rate limiting** on nonce/mutation/ingest endpoints yet; a public
  deployment should front them with a reverse-proxy limit.
- **First-come publishing** under official TLDs in this phase; no payment
  or identity binding.
- **Nonces are in-memory**: a restart clears unissued challenges (clients
  just request a new one); accepted-mutation history is durable.
