# Hosts-file Setup (MVP)

Until the local DNS resolver ships (phase 3), map Federate domains to the
local daemon by hand.

## Where

- Linux / macOS: `/etc/hosts` (edit with `sudo`)
- Windows: `C:\Windows\System32\drivers\etc\hosts` (edit as Administrator)

## Add these lines

```txt
127.0.0.1 home.fed
```

A ready-to-append copy lives at [`deploy/hosts-federate.txt`](../deploy/hosts-federate.txt).

## Verify

```sh
ping -c1 home.fed        # should hit 127.0.0.1
federate doctor          # checks hosts file among other things
```

Then, with `federated` running on port 80, open **http://home.fed** — no port.
