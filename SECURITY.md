# Security Policy

> [Versão em português (pt-BR)](SECURITY.pt-BR.md)

Federate Network is security-critical infrastructure: a naming and content
system where every layer (root zone, TLD records, domain records, manifests,
content blocks) is cryptographically verified. Bugs that break that chain
matter a lot.

## Reporting a vulnerability

Please do **not** open a public issue for security problems.

Use GitHub's private vulnerability reporting: **Security tab → Report a
vulnerability** on this repository. You will get a response as soon as
possible, and credit in the fix notes unless you prefer otherwise.

Especially interesting reports:

- signature or hash verification bypass at any layer
- root zone rollback/replay acceptance
- path traversal in the block/manifest stores
- DNS answer spoofing or cache poisoning in `federate-dns`
- SSRF via node registrations or provider fetching
- directory poisoning (fake nodes, provider-map stuffing)

## Scope notes

- Private keys never leave the node that generated them; any API that would
  expose one is a critical bug.
- Nodes are untrusted by design: a report must show a *client* accepting bad
  data, not just a node serving it.
- Deployment hardening guidance lives in
  [docs/en-US/deployment-vps.md](docs/en-US/deployment-vps.md).
