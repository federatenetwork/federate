# Vision: an alternative internet overlay

> [Versão em português (pt-BR)](../pt-BR/vision.md)

Federate Network is not a website, not a DNS trick, and not an HTTP gateway
project. It is an **alternative internet overlay**: a complete network (its
own namespace, protocol, nodes, discovery, content model, search) that runs
on top of the existing internet's plumbing.

## What Federate owns

- **Namespace**: its own root, its own TLDs, its own domain records, all
  cryptographically signed. No ICANN, no registrars.
- **Addressing**: `fed://domain/path` is the native way to name anything.
- **Protocol**: nodes and native clients speak the Federate protocol
  ([native-protocol.md](native-protocol.md)), not HTTP.
- **Nodes**: root authority, mirrors, DNS, gateways, storage, CDN, search,
  bootstrap; anyone can run any role except root authority.
- **Content model**: everything is content-addressed (BLAKE3), signed at
  every layer, and servable by any node without trusting it.
- **Discovery**: the node directory and bootstrap answer "who is out there";
  signatures answer "what is valid". Those are never the same question.
- **Search**: no ads, no tracking, no AI training, opt-out honored.

## What Federate deliberately does NOT own

Physical infrastructure is out of scope, permanently:

- no ISP layer, no last-mile anything
- no BGP, no ASN, no peering agreements
- no fiber, cable, radio, satellites
- no replacement for global IP routing

The existing internet moves packets fine. Federate replaces the layers above
packets: naming, trust, publishing, discovery, and the relationship between
people and the network.

## Compatibility is a bridge, not the product

`http://home.fed` in a normal browser works and must keep working; that is
how people try Federate with zero installs. But HTTP, DNS, and browsers are
**compatibility bridges** ([browser-compatibility.md](browser-compatibility.md)).
The core is the native protocol and the native `fed://` path; the bridges
translate into it and can never bypass its verification.

## Non-negotiables

- no blockchain
- no ads
- no tracking
- no AI training on content
- no engagement feeds
- signatures decide validity; servers are never trusted blindly

## Where this goes

A Federate-native client/browser ([future-federate-browser.md](future-federate-browser.md))
that speaks the protocol directly, renders more than HTML
([non-html-runtime-roadmap.md](non-html-runtime-roadmap.md)), and treats the
overlay ([overlay-network.md](overlay-network.md)) as its home network, with
the old web one bridge away.
