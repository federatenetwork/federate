# Troubleshooting

> [Versão em português (pt-BR)](../pt-BR/troubleshooting.md)

First step, always:

```sh
federate doctor
```

It checks daemon status, Node 1 reachability, port 80, hosts file, root zone
cache, domain resolution, gateway health, and cached content.

## Browser says "site can't be reached" for home.fed

1. Hosts file missing entries → [hosts-setup.md](hosts-setup.md). Verify:
   `ping -c1 home.fed` must hit `127.0.0.1`.
2. Daemon not running → run `federated`.
3. Daemon running on a fallback port, not 80 → check startup logs;
   see [port-80-setup.md](port-80-setup.md). `federate port-check` helps.

## "federated could not bind to 127.0.0.1:80"

Something else owns port 80 (`sudo lsof -i :80`), or missing privileges.
Fixes per OS in [port-80-setup.md](port-80-setup.md).

## "Domain not found in Federate Network" page

The name is a valid Federate TLD but has no record in the root zone. Check
`federate root show` for registered domains. If Node 1 recently added it,
restart `federated` (root refresh on demand is on the roadmap).

## "Federate resolution error" page

Node 1 unreachable and content not cached yet. Check
`curl https://federate.network/health`, your internet connection, and
`federate status` (`node1_reachable`). Cached sites keep working offline;
uncached ones need Node 1.

## Hash mismatch errors in logs

Downloaded content didn't match the manifest hash (corruption or tampering).
The daemon refuses to serve it. Clear cache and retry:
`federate cache clear`.

## Stale content after publishing an update

Manifests and blocks are content-addressed, so updates arrive via a new root
zone (new manifest hash). Restart `federated` to force a root refresh, or
`federate cache clear` for a full reset.

## Normal websites broken?

The MVP touches nothing global; only the hosts-file lines you added and
`127.0.0.1:80`. Remove the hosts lines to fully undo.
