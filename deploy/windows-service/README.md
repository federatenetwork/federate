# Windows (MVP)

For the MVP, run `federated.exe` from a terminal opened **as Administrator**
(needed to bind port 80).

A proper Windows service (via `windows-service` crate or `sc.exe`/NSSM
wrapping) is planned for a later phase; this directory reserves the spot.

Interim NSSM example:

```powershell
nssm install federated C:\federate\federated.exe --bootstrap https://federate.network
nssm start federated
```
