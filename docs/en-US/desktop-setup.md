# Desktop Setup: How a Friend Joins

> [Versão em português (pt-BR)](../pt-BR/desktop-setup.md)

Goal: type `http://home.fed` in Chrome/Safari/Firefox/Edge and enter the
Federate Network.

## The one-liner (macOS, Linux, Windows)

macOS or Linux, in a terminal:

```sh
curl -fsSL https://federate.network/install.sh | bash
```

Windows, in PowerShell:

```powershell
iex (irm https://federate.network/install.ps1)
```

The installer downloads the `federate` CLI and runs `federate setup`,
which does four things and then proves they work:

1. **Local certificate authority, mkcert-style.** Public CAs cannot
   issue for `.fed`, so setup generates a CA on YOUR machine (the
   private key is created there and never leaves it; a shared
   network-wide CA could impersonate any HTTPS site and is therefore
   never used) and adds its public certificate to the system trust
   store. Result: `https://home.fed` with a green lock.
2. **Local verifying resolver + gateway as a system service.**
   `federate dns proxy --local-gateway` runs on `127.0.0.1:53` (launchd
   on macOS, systemd on Linux, a SYSTEM boot task on Windows). It
   answers names under every TLD in the **signed root zone**, which it
   refreshes continuously against the pinned root key; there is no TLD
   list on the client, so a TLD created tomorrow resolves on every
   installed machine within a minute. Federate names point at a
   loopback gateway (http 80 + https 443) that fetches content over the
   Federate protocol and verifies the full signature/hash chain on your
   machine before serving a single byte; per-name certificates are
   minted by the local CA on first use. Every non-Federate name is
   forwarded to upstream DNS untouched.
3. **System DNS pointed at the resolver.** Previous DNS settings are
   saved and restored exactly by `federate dns uninstall` (which also
   removes the CA from the trust store).
4. **`fed://` links registered** to open in your browser (see below).

Self-test at the end: `home.fed` must resolve through `127.0.0.1:53`,
fetch through the gateway, and complete a TLS handshake verified against
the local CA, or setup says exactly which step failed.

Manage it later:

```sh
federate dns status          # service + system DNS state
sudo federate dns uninstall  # restore previous DNS, remove the service
sudo federate setup          # do everything again
```

Already running something on port 53 (dnsmasq, a dev DNS)? The installer
detects that and moves to another loopback address automatically
(`127.53.0.1:53` and so on; system DNS settings accept only an IP, so
the escape hatch is an address inside 127.0.0.0/8, never a port). Your
existing service is not touched.

Why this beats a hosts file: nothing is hardcoded, new TLDs and new
domains appear automatically, answers carry multiple healthy gateways
with a 30s TTL, and the root zone signature is verified on your machine.

## Clickable fed:// links

`federate setup` already registers this. To do it alone (per-user, no
admin rights, no code signing; macOS, Linux, and Windows):

```sh
federate handler install     # register (uninstall / status also exist)
open fed://home.fed          # macOS test; Linux: xdg-open, Windows: start
```

On macOS this generates a tiny local AppleScript applet in
`~/Applications` (locally created, so Gatekeeper never quarantines it);
on Linux it writes a `.desktop` entry with `x-scheme-handler/fed`; on
Windows it writes per-user registry keys. All three just rewrite
`fed://name/path` to `http://name/path`, so name resolution still comes
from your Federate DNS setting.

## Building from source instead

```sh
git clone https://github.com/federatenetwork/federate && cd federate
cargo build --release -p federate-cli
sudo ./target/release/federate setup
```

For running a full local daemon (gateway on port 80, local cache), see
[port-80-setup.md](port-80-setup.md) and [nodes.md](nodes.md). The old
hosts-file path ([hosts-setup.md](hosts-setup.md)) still works but is
static; the resolver service replaces it.

## Check-up

```sh
federate doctor     # full diagnostics with fixes
federate dns status # resolver service + system DNS
federate open home.fed
```

Visited sites are cached locally and keep working even when Node 1 is
temporarily offline.
