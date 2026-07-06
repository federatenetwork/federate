# The future Federate browser

> [Versão em português (pt-BR)](../pt-BR/future-federate-browser.md)

A Federate-native client that speaks the Federate protocol directly, treats
`fed://` as its address bar's home scheme, and does not need the DNS or HTTP
bridges at all. Not implemented yet; this page defines the boundaries the
codebase already respects so that building it is assembly, not surgery.

## What it is

- address bar takes `fed://anything.validtld/path`
- resolution, verification, and fetching through the same
  `federate-resolution` engine the gateway and CLI use today
- transport through `federate-transport` (native protocol), with the HTTP
  compatibility client as a fallback, not a foundation
- local trust anchor: the pinned root key lives on the user's device;
  the browser verifies everything itself, trusting no gateway

## Boundaries already in place

| Future browser need | Where it already lives |
|---|---|
| parse/normalize addresses | `federate-uri` |
| resolve + verify any domain | `federate-resolution::resolve_uri` |
| speak to nodes natively | `federate-protocol` + `federate-transport` |
| discover nodes/providers | `federate-directory` client |
| local cache, verified reads | `federate-storage` block store |
| identity/keys | `federate-identity` |

The browser shell (rendering, tabs, UI) is the only genuinely new part.

## Beyond HTML

The first renderer is boring on purpose: HTML/CSS (what sites publish
today). The document model is deliberately open for more; see
[non-html-runtime-roadmap.md](non-html-runtime-roadmap.md):

- native Federate documents (signed, content-addressed, markdown-first)
- signed application packages with explicit permissions
- games and realtime experiences
- media and world streaming

## Non-goals

Same as the network's: no blockchain, no ads, no tracking, no AI training,
no engagement feeds. A Federate browser that spies on its user is not a
Federate browser.
