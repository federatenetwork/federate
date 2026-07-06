# Non-HTML runtime roadmap

> [Versão em português (pt-BR)](../pt-BR/non-html-runtime-roadmap.md)

Federate content today is static sites: HTML, CSS, images, fonts, all
content-addressed and signed. That is the floor, not the ceiling. This
roadmap sketches what a Federate-native runtime can carry once the native
client exists, and what stays true at every stage.

## Invariants (every stage)

- everything is content-addressed and signed; unsigned content never runs
- capabilities are explicit: a document/app gets nothing it did not declare
  and the user did not grant
- no ads, no tracking, no AI training, no engagement feeds
- the manifest stays the unit of publishing: a signed map of names to hashes

## Stages

1. **Static web content** (today): HTML/CSS/JS served through gateways;
   JS runs in the normal browser sandbox.
2. **Native Federate documents**: markdown-first signed documents (the
   `fed-md` renderer is the seed), rendered by the native client without a
   web engine. Deterministic layout, no scripts, safe by construction.
3. **Signed packages**: a manifest that declares an entry document plus
   assets plus a permission list. Installable, updatable by publishing a new
   signed manifest version, verifiable offline.
4. **Sandboxed apps**: packages with code (likely WASM) running under
   explicit, user-granted permissions (storage quota, network to declared
   Federate names only, no ambient authority).
5. **Realtime and streaming**: protocol extensions for sessions and streams
   (games, media, shared worlds) once the native transport carries QUIC.

## Sequencing rationale

Documents before packages before apps: each stage reuses the previous
stage's verification story and adds exactly one new trust question. The
answer is always the same shape: signed by whom, granted what, addressed by
hash.

Nothing in this roadmap is implemented today, deliberately. The codebase
keeps the boundaries (URI / protocol / resolution / content stores) clean so
each stage lands as a new consumer of existing layers, not a rewrite.
