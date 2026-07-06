# Root mirrors

A root mirror distributes signed root zone copies so the network does not
depend on one VPS. Mirrors **cannot create or modify TLDs** — and the crypto
makes cheating pointless.

## How it works

1. The mirror fetches the root zone from Node 1 (or another mirror).
2. It verifies the zone signature against the pinned Federate Root Key.
   An unverifiable zone is never stored or served.
3. It serves the verified zone at `GET /v1/root`, refreshing every minute.

## Why mirrors can't cheat

Every consumer (daemon, DNS node, gateway, CLI) verifies the root zone
signature against its own pinned root key **before trusting any data** —
regardless of where the bytes came from. A mirror that alters a TLD record,
adds a domain, or unblocks a blocked TLD produces a zone that fails
verification and is rejected by every client.

Mirrors distribute; the Federate Root Key decides.

## Run one

```toml
# federate.toml
[node]
roles = ["root-mirror"]
region = "eu-de"
public_ip = "x.x.x.x"

[network]
bootstrap = "https://federate.network"
root_key = "<FEDERATE_ROOT_PUBLIC_KEY_HEX>"   # required in spirit: pin it
```

```sh
federate-noded --config federate.toml
```

Other nodes can then use the mirror as their root source:

```sh
federate-dnsd --bootstrap http://<mirror-ip>:8080 --root-key <ROOT_KEY>
```
