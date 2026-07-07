# Publishing sites

> [Versão em português (pt-BR)](../pt-BR/publishing.md)

Publishing no longer requires filesystem access to Node 1. A site becomes a
content-addressed package plus a signed mutation, and Node 1 ingests it at
runtime after verifying the whole chain. `sites/` on Node 1 is only a
first-boot seed (see [root-registry.md](root-registry.md)).

## Official TLDs: one-step publish

```sh
federate publish package ./dist --domain joao.pagina \
    --key-dir .federate-owner --bootstrap https://federate.network
```

This packages `./dist` (must contain `index.html`), signs the manifest with
your owner key, requests a mutation nonce, signs a `publish_site` mutation,
and submits everything to `POST /v1/ingest/package`. On success the domain
resolves immediately over the native protocol and the HTTP gateway:

```sh
federate fetch fed://joao.pagina/
```

Updating is the same command again (the target version advances), or, if
the manifest is already on the node:

```sh
federate domain update joao.pagina --manifest <new-manifest-hash> --key-dir .federate-owner
```

## Two-step: package first, submit later

```sh
federate site package ./dist --domain joao.pagina --key-dir .federate-owner
federate registry submit-package ./dist.federate-package --key-dir .federate-owner
```

`site package` still works exactly as before (offline packaging, optional
`--install` into a local node). `registry submit-package` reads the package
directory, signs the publish mutation with the same owner key, and submits
it.

## What Node 1 verifies before accepting

Package ingest is fail closed. Before any state changes:

- package caps (32 MiB decoded, 2048 blocks) and hex decoding;
- every block hash matches its content;
- the manifest bytes match the mutation's `manifest_hash`;
- the mutation envelope verifies ([mutations.md](mutations.md)): signature,
  nonce, timestamp window, replay history, target version;
- the TLD exists, is root-managed, resolvable, and not expired (delegated
  TLDs publish through their own operator instead);
- the domain either does not exist yet (first-come under official TLDs, for
  now) or is owned by the signing key and in a status that allows updates;
- the manifest validates, is signed by the actor, and names exactly this
  domain.

Only then is the domain record created/updated, countersigned by the
official operator key, the zone re-signed and persisted, and a signed audit
event appended. Blocks and manifests are content-addressed, so a rejected
mutation leaves no authority behind, only unreferenced bytes.

## Delegated TLDs

Domain records of a delegated TLD live in the operator's signed registry,
not in the root zone, so publishing there stays with the operator tooling:

```sh
federate site package ./dist --domain eu.femboy --key-dir .federate-owner
federate operator sign-record eu.femboy --owner <owner-key> --manifest-hash <hash>
federate operator build-registry femboy --records .
```

New: a `delegated_manifest` operator no longer needs the root to edit seed
code to re-pin the registry. The operator submits an
`update_registry_pointer` mutation signed with the operator key; the root
re-signs the TLD record with the new registry hash, and the registry
version must strictly increase (client rollback protection).

Creating the delegation itself is also runtime now (root key required):

```sh
federate tld delegate quintal --owner <hex> --operator <hex> --key-dir <root-key-dir>
```

## Enforcement

```sh
federate domain suspend joao.pagina --key-dir <operator-or-root-key-dir>
federate domain reinstate joao.pagina --key-dir <operator-or-root-key-dir>
```

A suspended domain stops resolving everywhere immediately (gateway, native
protocol, DNS existence checks) and rejects owner updates until reinstated.
Revocation is terminal unless the root key reinstates.

## What still remains

- payments/marketplace: publishing is first-come and free in this phase;
- replication: content lives on Node 1 plus whatever nodes cache or pin it;
- rate limiting on the ingest endpoint;
- a web UI; everything above is CLI + HTTP API today.
