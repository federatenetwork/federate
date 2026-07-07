# Migrating registry storage (JSON files to redb)

> [Versão em português (pt-BR)](../pt-BR/migrations.md)

Older nodes persisted the registry as JSON files (`state.json`,
`mutations.jsonl`, `audit.jsonl`, snapshot files). That layout is retired:
the authoritative registry store is now the embedded redb database
`registry.redb` (see [root-registry.md](root-registry.md)). A node with the
old layout refuses to start and points here.

## One-time migration

```sh
# 1. stop the server
systemctl stop federate-server        # or kill the dev process

# 2. migrate (validates everything first)
federate registry migrate-json-to-redb --data-dir /var/lib/federate/data

# 3. start the server again
systemctl start federate-server
```

What the command does:

1. loads `state.json` + JSONL logs + snapshot files;
2. VALIDATES everything against the node's root key: zone signature,
   every delegated registry against its operator key, every audit event
   signature; any failure aborts the migration with no database written;
3. writes the redb database in one initial transaction (records, current
   zone, older zone versions from snapshots, mutation history, audit log,
   per-target versions, delegated registry pointers);
4. moves the old JSON files into `registry/legacy-json-backup/` (kept, not
   deleted);
5. prints a migration report.

Content stores (`manifests/`, `blocks/`) and snapshot files are
content-addressed and stay exactly where they are; nothing about keys or
blocklists changes.

## After migrating

```sh
federate registry db stats     # table counts
federate registry db verify    # full verification
federate root status --data-dir /var/lib/federate/data
```

Replay protection, per-target versions, and the audit chain survive the
migration unchanged; a mutation that was already applied under the JSON
layout is still rejected as a replay under redb. Once satisfied, the
`legacy-json-backup/` directory can move into cold storage.

## Fresh nodes

Nothing to migrate: `federate root init` + `federate root seed` create the
database directly (see [root-registry.md](root-registry.md)).
