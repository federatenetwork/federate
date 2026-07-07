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
which does three things and then proves they work:

1. **Local verifying resolver as a system service.** `federate dns proxy`
   runs on `127.0.0.1:53` (launchd on macOS, systemd on Linux, a SYSTEM
   boot task on Windows). It answers names under every TLD in the
   **signed root zone**, which it refreshes continuously against the
   pinned root key. There is no TLD list on the client: a TLD created
   tomorrow resolves on every installed machine within a minute. Every
   non-Federate name is forwarded to upstream DNS untouched.
2. **System DNS pointed at the resolver.** Previous DNS settings are
   saved and restored exactly by `federate dns uninstall`.
3. **`fed://` links registered** to open in your browser (see below).

Self-test at the end: `home.fed` must resolve through `127.0.0.1:53` and
fetch through a live gateway, or setup fails loudly.

Manage it later:

```sh
federate dns status          # service + system DNS state
sudo federate dns uninstall  # restore previous DNS, remove the service
sudo federate setup          # do everything again
```

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
