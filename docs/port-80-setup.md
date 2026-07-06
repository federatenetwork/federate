# Port 80 Setup: Portless URLs

> [Versão em português (pt-BR)](pt-BR/port-80-setup.md)

The whole user experience is `http://home.fed` with **no port**. Browsers use
port 80 for plain `http://`, so `federated` must bind `127.0.0.1:80`.
Binding ports below 1024 needs privileges on most systems.

## Linux

Preferred: grant the capability once:

```sh
cargo build --release
sudo setcap 'cap_net_bind_service=+ep' ./target/release/federated
./target/release/federated
```

Or install the systemd user service (`deploy/systemd/federated.service`),
which uses `AmbientCapabilities=CAP_NET_BIND_SERVICE`.

## macOS

Preferred: pf port redirect. `federated` runs **unprivileged** on 8787 and
the kernel forwards loopback port 80 to it. Portless URLs work, no root
process, no root-owned files:

```sh
echo "rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 80 -> 127.0.0.1 port 8787" | sudo pfctl -ef -
./target/release/federated --gateway-addr 127.0.0.1:8787
```

The pf rule resets at reboot. For persistence, add it to `/etc/pf.conf`
(directly after the `rdr-anchor "com.apple/*"` line; pf requires translation
rules in that section), then reload with `sudo pfctl -f /etc/pf.conf`.

Alternative: install the launchd daemon
(`deploy/launchd/network.federate.federated.plist`), which runs as root at
boot and binds port 80 directly:

```sh
sudo cp deploy/launchd/network.federate.federated.plist /Library/LaunchDaemons/
sudo launchctl load /Library/LaunchDaemons/network.federate.federated.plist
```

**Never run `federated` with plain `sudo` from a terminal.** macOS sudo can
preserve `$HOME`, so the daemon writes root-owned files into
`~/Library/Application Support/federate`; after that, running it as your
normal user fails with `Error: Io(PermissionDenied)` before the first log
line. Repair with:

```sh
sudo chown -R "$(whoami)":staff ~/Library/Application\ Support/federate
```

## Windows

MVP: run the terminal **as Administrator**, then `federated.exe`.
A proper Windows service is planned (see `deploy/windows-service/`).

## If binding fails

`federated` prints a clear explanation with per-OS fixes. Check with:

```sh
federate port-check
```

## Development fallback

`federated --gateway-addr 127.0.0.1:8787` works for development, but it is not
the documented user flow; the primary flow is portless `http://home.fed`.
