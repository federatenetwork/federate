# The persistent Federate Root Registry

> [Versão em português (pt-BR)](../pt-BR/root-registry.md)

Node 1 no longer rebuilds the network state from seed code and `sites/` on
every restart. The Federate Root Registry is durable, runtime-mutable signed
state: it survives restarts, and it changes at runtime only through
[signed mutations](mutations.md) and the [package ingest path](publishing.md).

## Seed is first-boot only

On the very first start (no registry state on disk), `federate-server` runs
the seed exactly once:

1. official TLDs are validated against the blocklists and root-signed;
2. `sites/` is scanned, files are content-addressed, manifests are
   owner-signed, domain records are operator-signed;
3. seed delegated TLDs (`.femboy`) get their operator keys and registries;
4. the assembled root zone is signed, self-verified, and adopted as the
   initial persistent registry.

On every later boot the persistent registry is the source of truth. `sites/`
is never scanned again, seed constants are never consulted again, and
changing the network means sending a signed mutation, not editing code.

## On-disk layout

Everything lives under `<data_dir>/registry/` (default
`.federate-server/registry/`):

| Path | Contents |
|---|---|
| `state.json` | current signed root zone, delegated registries (exact signed bytes), per-target mutation versions |
| `manifests/<hash>` | content-addressed manifest and registry bytes |
| `blocks/` | content-addressed site blocks (BLAKE3-sharded store) |
| `audit.jsonl` | append-only signed audit log, one event per line |
| `mutations.jsonl` | append-only history of accepted mutations |
| `snapshots/root-zone-v<N>.json` | one immutable root zone snapshot per accepted version |

Writes are atomic (write to `.tmp`, then rename). Private keys are NEVER
stored in any of these records; they stay in their own `identity.key` files.

## Fail-closed loading

On boot the registry is re-verified before it is served:

- the root zone must validate structurally and verify against the root key;
- every delegated registry must verify against the operator key named in
  its root-signed TLD record;
- every manifest and block is checked against its content address
  (corrupted content entries are dropped, never served);
- a tampered `state.json` stops the node instead of serving forged data.

## Root zone versions and rollback protection

The seed derives `root_version` from the wall clock. Every accepted mutation
afterwards re-signs the zone with `max(previous + 1, now)`, so the version
is strictly monotonic across mutations AND restarts. Clients keep their
existing rollback protection: a correctly signed but older zone is rejected.
Old snapshot files exist for audit and recovery, but the server only ever
serves the current zone.

## Inspecting the registry

```sh
federate registry status                 # version, counts, mutation history size
federate registry audit --limit 50      # the signed audit log
federate registry verify                 # ask the node to self-verify everything
federate registry snapshot               # force a root zone snapshot
federate mutation inspect <mutation_id> # one accepted mutation + its audit event
```

HTTP equivalents: `GET /v1/registry/status`, `GET /v1/registry/audit`,
`GET /v1/registry/verify`, `POST /v1/registry/snapshot`,
`GET /v1/mutations/:id`.

## What this unblocks and what still remains

Now possible at runtime, with signatures and audit:

- publishing and updating official-TLD domains ([publishing.md](publishing.md));
- delegating TLDs (`federate tld delegate`);
- suspending / reinstating / revoking domains;
- delegated operators re-pinning their registry hash through the root.

Still ahead before a marketplace and payments:

- application/approval and payment flows (mutations exist; commerce does not);
- key rotation and recovery ([signatures.md](signatures.md), future work);
- rate limiting and abuse reporting on the mutation endpoints;
- multi-node root authority (today one Node 1 holds the root key).
