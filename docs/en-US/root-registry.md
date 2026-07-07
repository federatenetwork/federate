# The persistent Federate Root Registry

> [Versão em português (pt-BR)](../pt-BR/root-registry.md)

Node 1 no longer rebuilds the network state from seed code and `sites/` on
every restart. The Federate Root Registry is durable, runtime-mutable signed
state: it survives restarts, and it changes at runtime only through
[signed mutations](mutations.md) and the [package ingest path](publishing.md).

## The database is the only source of truth

No TLD exists in compiled code. There is no hardcoded TLD list anywhere in
runtime logic: official TLDs, delegated TLDs, reserved names, and blocked
names are all ordinary TldRecords in the persistent registry (plus the
external blocklist data files). Adding, updating, suspending, or removing a
TLD never requires editing source code or recompiling anything.

## Explicit initialization and seeding

A brand new node bootstraps in explicit steps; the server never seeds TLDs
on its own:

```sh
federate root init --data-dir .federate-server         # empty signed registry, ZERO TLDs
federate root seed --file seeds/official-tlds.toml --data-dir .federate-server
federate-server                                        # serves whatever the database holds
federate publish package ./site --domain home.fed      # content arrives via ingest
```

`seeds/official-tlds.toml` is plain TOML data (`[[tlds]]` entries with
`name`, `mode`, `purpose`). The seed command validates every name (naming
rules + blocklists), creates root-signed TldRecords through the normal
audited mutation path, and signs a new root zone version. Editing the seed
file changes NOTHING until the command runs again, and the command refuses
an already-populated registry; `--force` only adds missing entries, never
overwrites existing records.

If `federate-server` starts with no registry on disk, it initializes an
EMPTY one (zero TLDs) and logs how to seed it. It never creates TLDs from
code, on first boot or any other boot. TLDs can also be created on a
running node with signed mutations:

```sh
federate tld create quintal --purpose "..." --key-dir <root-key-dir>
federate tld reserve tesouro --reason "..." --key-dir <root-key-dir>
federate tld block scam --reason "..." --key-dir <root-key-dir>
federate tld delegate outra --owner <hex> --operator <hex> --key-dir <root-key-dir>
```

## On-disk layout

Everything lives under `<data_dir>/registry/` (default
`.federate-server/registry/`):

| Path | Contents |
|---|---|
| `registry.redb` | the authoritative embedded database (redb): tables `tld_records`, `domain_records`, `root_zone_versions`, `mutations`, `audit_events`, `snapshots`, `nonces`, `registry_metadata`, `delegated_registries`, `target_versions` |
| `manifests/<hash>` | content-addressed manifest and registry bytes |
| `blocks/` | content-addressed site blocks (BLAKE3-sharded store) |
| `snapshots/root-zone-v<N>.json` | human-inspectable root zone copies (the signed bytes are also in the database) |

Every accepted mutation commits in ONE database transaction: it either
fully applies or not at all, and a crash mid-mutation leaves the previous
state intact. Nonces are persistent too, so a consumed challenge can never
be replayed, not even across restarts. Private keys are NEVER stored in
the database or any record; they stay in their own 0600 `identity.key`
files. Blocklists remain external policy data files (`blocked_tlds.txt`,
`data/blocked/*`). The old JSON layout (`state.json` + JSONL logs) is
retired; see [migrations.md](migrations.md) to convert an existing node
and [backups.md](backups.md) for backup/restore.

## Fail-closed loading

On boot the registry is re-verified before it is served:

- the root zone must validate structurally and verify against the root key;
- every delegated registry must verify against the operator key named in
  its root-signed TLD record;
- every manifest and block is checked against its content address
  (corrupted content entries are dropped, never served);
- a tampered database record (e.g. a forged zone) stops the node instead
  of serving forged data; `federate registry db verify` additionally
  cross-checks the record tables against the signed zone.

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
federate registry db stats               # table counts + database size (offline)
federate registry db verify              # full offline verification incl. table consistency
federate registry backup --output <file> # copy the database (offline; see backups.md)
federate registry restore --input <file> # restore + full re-verification
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
