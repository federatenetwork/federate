# Internal HTTPS & Local Federate Root CA (Planned)

> [Versão em português (pt-BR)](../pt-BR/https-local.md)

## MVP position

The MVP flow is `http://home.fed`. Public `https://federate.network` uses
normal Let's Encrypt (real public domain, handled by Caddy; see
[deployment-vps.md](deployment-vps.md)).

Internal domains (`home.fed`, `joao.pagina`, `fotolia.rosa`, `arcade.mosca`, …)
**cannot** use Let's Encrypt: they are internal Federate TLDs with no public
DNS, so no public CA will ever issue for them.

## Optional today: mkcert

For developers who want `https://home.fed` locally now:

```sh
mkcert -install
mkcert home.fed docs.fed "*.pagina" "*.rosa" "*.cara" "*.mosca"
```

Then run a local TLS terminator (or a future `federated --tls` flag) with the
generated cert. This is optional and undocumented in the friend flow.

## Planned: local Federate Root CA (phase 7)

The desktop installer will:

1. Generate a per-machine Federate Root CA key (never leaves the device).
2. Install it into the OS/browser trust stores (like mkcert does).
3. Have `federated` mint short-lived leaf certificates per Federate domain
   on the fly and terminate TLS on `127.0.0.1:443`.
4. Browsers then load `https://home.fed` with a valid padlock.

Design notes:

- Per-machine CA (not a shared network CA) - compromise of one machine never
  affects others, and no CA private key is ever distributed.
- Certificate minting lives beside the gateway, reusing `federate-identity`
  for key handling; resolution stays untouched in `federate-resolution`.
- HTTP on port 80 remains as fallback/redirect.
