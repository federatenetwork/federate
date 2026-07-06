# Browser compatibility: the bridges

> [Versão em português (pt-BR)](../pt-BR/browser-compatibility.md)

Normal browsers cannot speak the Federate protocol or parse `fed://`. The
compatibility layer exists so anyone can use Federate today with zero
installs; it is a set of **adapters** around the native core, never a
parallel implementation.

## The three bridges

1. **DNS bridge** (`federate-dnsd`): a device points its DNS at a Federate
   DNS node; Federate TLDs answer with healthy gateway IPs, every other name
   forwards upstream. The rest of the internet keeps working.
2. **HTTP gateway** (`federate-gatewayd`, `federated`): reads
   `Host: joao.pagina` + `/about`, translates to `fed://joao.pagina/about`
   via `federate-uri`, and calls the same resolution engine as every native
   consumer. It cannot bypass verification: there is no other code path.
3. **Public gateway mode**: a gateway on a public IP behind the DNS bridge
   serves phones and computers with no Federate software at all.

```
browser  --HTTP-->  gateway  --fed://joao.pagina/about-->  resolution engine
                                                            (signatures/hashes)
native client  ----------fed://joao.pagina/about--------->  same engine
```

## What the bridges guarantee

- `http://home.fed`, `http://joao.pagina`, `http://fotolia.rosa`,
  `http://fed.busca` keep working in any browser
- any valid domain under any valid TLD works; nothing is hardcoded
- content served over the bridge went through the full
  root → TLD → domain → manifest → block verification chain
- content-addressed responses carry strong ETags (the block hash), so
  browsers revalidate to 304s

## What the bridges cannot do

- express native capabilities beyond request/response HTML delivery
- carry the native protocol's future features (peer discovery, subscribe,
  non-HTML documents)
- add trust: a compromised gateway can refuse to serve or serve errors, but
  cannot forge content that verifies against your pinned root key. Users who
  need end-to-end verification run `federated` locally or a native client;
  the public gateway is convenience mode.

## Direction

Bridges stay for as long as normal browsers exist. The native path
([native-protocol.md](native-protocol.md),
[future-federate-browser.md](future-federate-browser.md)) grows beside
them, not instead of them.
