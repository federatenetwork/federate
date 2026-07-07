# Backing up and restoring the root registry

> [Versão em português (pt-BR)](../pt-BR/backups.md)

The registry lives in an embedded redb database:
`<data_dir>/registry/registry.redb`. It is the authoritative network state
(TLD records, domain records, root zone versions, mutation history, audit
log, nonces, delegated registry pointers). Treat its backup like a
database backup.

## What to back up

| Asset | Where | How |
|---|---|---|
| Registry database | `<data_dir>/registry/registry.redb` | `federate registry backup` |
| Private keys | `<data_dir>/root/`, `official-operator/`, `identity.key` | offline copy, 0600, never in the database |
| Content stores | `<data_dir>/registry/manifests/`, `registry/blocks/` | plain file copy (content-addressed, self-verifying) |
| Blocklists | `blocked_tlds.txt`, `data/blocked/` | plain file copy (external policy data) |

Simplest full backup (server stopped):

```sh
tar czf federate-backup.tgz -C /var/lib/federate data
```

## Registry database backup

```sh
federate registry backup --output /backups/registry-$(date +%Y%m%d).redb \
    --data-dir /var/lib/federate/data
```

Run with `federate-server` stopped: the database is single-writer and the
command refuses (lock error) while the server holds it. The copy is
sanity-opened before the command reports success, and an existing output
file is never overwritten.

## Restore

```sh
federate registry restore --input /backups/registry-20260707.redb \
    --data-dir /var/lib/federate/data [--force]
```

Restore refuses to clobber an existing database without `--force`. After
copying, the restored registry is FULLY re-verified against the root key
(zone signature, delegated registries, audit signatures, content hashes,
table consistency); a backup that fails verification is reported and must
not be served.

Restoring the database without the matching `manifests/` and `blocks/`
directories yields a registry whose domains point at content the node does
not have yet; the node serves records but not the content until the blocks
arrive (restore them from the file backup, or let the network re-supply
cached blocks).

## Rollback protection caveat

Clients remember the highest root zone version they verified. Restoring an
OLD backup means the node serves an older-but-valid zone that clients will
reject until new mutations push the version past what they remember.
Restore the newest backup available, then apply a no-op-ish mutation (or a
snapshot + real mutation) to advance the version when needed.
