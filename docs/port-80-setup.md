# Port 80 Setup — Portless URLs

The whole user experience is `http://home.fed` with **no port**. Browsers use
port 80 for plain `http://`, so `federated` must bind `127.0.0.1:80`.
Binding ports below 1024 needs privileges on most systems.

## Linux

Preferred — grant the capability once:

```sh
cargo build --release
sudo setcap 'cap_net_bind_service=+ep' ./target/release/federated
./target/release/federated
```

Or install the systemd user service (`deploy/systemd/federated.service`),
which uses `AmbientCapabilities=CAP_NET_BIND_SERVICE`.

## macOS

MVP: run with admin rights:

```sh
sudo ./target/release/federated
```

Or install the launchd daemon (`deploy/launchd/network.federate.federated.plist`),
which runs as root at boot:

```sh
sudo cp deploy/launchd/network.federate.federated.plist /Library/LaunchDaemons/
sudo launchctl load /Library/LaunchDaemons/network.federate.federated.plist
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
the documented user flow — the primary flow is portless `http://home.fed`.
