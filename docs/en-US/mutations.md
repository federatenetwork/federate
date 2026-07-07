# Signed mutations

> [Versão em português (pt-BR)](../pt-BR/mutations.md)

A mutation is the only way the [persistent root registry](root-registry.md)
changes at runtime. Every mutation is a signed envelope; the server verifies
the signature, the challenge nonce, the timestamp, the per-target version,
and the actor's authority against CURRENT registry state before anything is
touched. Everything fails closed.

## The envelope

```json
{
  "mutation_id": "<blake3 of the canonical envelope with id+signature blanked>",
  "nonce": "<server-issued single-use nonce>",
  "issued_at": "2026-07-06T12:00:00Z",
  "actor_public_key": "<hex ed25519 key of the signer>",
  "target_version": 3,
  "action": { "type": "publish_site", "domain": "joao.pagina", "manifest_hash": "..." },
  "signature_algorithm": "ed25519",
  "signature": "<hex signature over the canonical envelope>"
}
```

Signing uses the same canonical JSON rules as every other signed object
(see [signatures.md](signatures.md)). The `mutation_id` is self-certifying:
it is the BLAKE3 hash of the envelope content, so a replayed or altered
request is detectable forever.

## Challenge-response (anti-replay)

1. `POST /v1/mutations/nonce` returns a random single-use nonce with a
   5 minute TTL (`federate mutation nonce`).
2. The client embeds the nonce in the envelope and signs it.
3. The server consumes the nonce on submission: reuse, expiry, or an
   unknown nonce rejects the mutation with `409`.

A mutation is ALSO rejected when:

- the signature is missing or not from `actor_public_key`;
- `mutation_id` does not match the envelope content;
- `issued_at` is outside the 5 minute acceptance window (fail closed on
  unparseable timestamps);
- the `mutation_id` was already applied (persistent history, survives
  restarts);
- `target_version` does not strictly advance the target's last accepted
  version (rollback attempt);
- the actor is not authorized for the action (see below);
- the target's status does not allow the transition.

## Actions and authorization

Authorization is checked against the current signed state, never against
anything the request claims.

| Action | Authorized signer | Effect |
|---|---|---|
| `publish_site` | domain owner key | create/update an official-TLD domain from an ingested package |
| `update_domain_manifest` | domain owner key | point an existing domain at a new owner-signed manifest |
| `set_domain_status` | TLD operator key or root key | suspend / reinstate / revoke a root-managed domain |
| `issue_domain` | TLD operator key | insert a full operator-signed record inside the operator's own TLD |
| `delegate_tld` | Federate Root Key | create a delegated TLD record |
| `update_tld` | Federate Root Key | update mutable TLD metadata (endpoint, expiry, notes) |
| `set_tld_status` | Federate Root Key | change a TLD status |
| `update_registry_pointer` | delegated TLD operator key | pin a new signed registry (version must increase) |

Status transitions for domains: `active -> suspended`,
`suspended -> active`, anything `-> revoked`; `revoked -> active` needs the
root key (operator revocation is terminal for the operator). Same-status
writes are rejected.

## What an accepted mutation produces

1. the zone (or TLD/domain record) is updated and re-signed;
2. `root_version` bumps to `max(previous + 1, now)`: client rollback
   protection keeps working;
3. a signed [audit event](root-registry.md) is appended:
   `event_id`, `mutation_id`, `actor_public_key`, `actor_role`, `action`,
   `target_type`, `target_id`, `previous_state_hash`, `new_state_hash`,
   `timestamp`, `signature` (root key);
4. the mutation is recorded in `mutations.jsonl` (replay protection across
   restarts);
5. a new root zone snapshot is written.

## HTTP surface

| Endpoint | Purpose |
|---|---|
| `POST /v1/mutations/nonce` | issue a single-use challenge nonce |
| `POST /v1/mutations` | submit one signed mutation |
| `POST /v1/ingest/package` | submit a site package (blocks + manifest + publish mutation) |
| `GET /v1/mutations/:id` | inspect an accepted mutation |
| `GET /v1/mutations/target/:kind/:id` | current/next version for a target (`domain` or `tld`) |

Rejections: `403` unauthorized or bad signature, `409` replay / nonce /
version rollback, `404` unknown target, `400` malformed or disallowed
transition. The body always carries `{"accepted": false, "error": "..."}`.

## CLI

```sh
federate mutation nonce
federate mutation inspect <mutation_id>
federate domain update <domain> --manifest <hash> --key-dir <owner-key-dir>
federate domain suspend <domain> --key-dir <operator-or-root-key-dir>
federate domain reinstate <domain> --key-dir <operator-or-root-key-dir>
federate tld delegate <tld> --owner <hex> --operator <hex> --key-dir <root-key-dir>
```

Each command fetches a nonce and the next target version, signs the
envelope locally with the key in `--key-dir`, and submits it. Private keys
never leave the machine.
