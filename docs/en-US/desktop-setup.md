# Desktop Setup: How a Friend Joins

> [Versão em português (pt-BR)](../pt-BR/desktop-setup.md)

Goal: type `http://home.fed` in Chrome/Safari/Firefox/Edge and enter the
Federate Network.

## 1. Install or build `federated`

```sh
git clone <this repo> && cd federatenetwork
cargo build --release
```

Binaries: `target/release/federated` (daemon) and `target/release/federate` (CLI).

## 2. Configure the bootstrap server

The default is already `https://federate.network` (Node 1). To override:

```sh
federated --bootstrap https://federate.network
```

## 3. Add hosts-file mappings

Follow [hosts-setup.md](hosts-setup.md); append `deploy/hosts-federate.txt`
to your hosts file.

## 4. Allow the daemon to bind port 80

Follow [port-80-setup.md](port-80-setup.md) for your OS
(Linux: `setcap` or systemd; macOS: `sudo` or launchd; Windows: run as Administrator).

## 5. Run the daemon

```sh
federated
```

You should see: identity loaded, root zone fetched from Node 1, gateway on
`http://127.0.0.1:80`, API on `127.0.0.1:7777`.

## 6. Open the Federate Network

Open **http://home.fed** in any normal browser, portless. More sites will
appear on the network as publishing opens up.

## Check-up

```sh
federate doctor     # full diagnostics with fixes
federate status     # daemon status
federate open home.fed
```

Visited sites are cached locally and keep working even when Node 1 is
temporarily offline.
