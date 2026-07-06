# Blocked TLDs

> [Versão em português (pt-BR)](../pt-BR/blocked-tlds.md)

## `blocked_tlds.txt`

The repository root file `blocked_tlds.txt` is the **authoritative public
IANA/ICANN TLD blocklist**: the full list of TLDs that exist in the normal
internet's root zone (~1,400 names). `federate-server` loads it at startup
(`--blocked-tlds` flag); it is data, never hardcoded in source. One name per
line, case-insensitive, `#` comments allowed.

Any attempt to create, apply for, approve, or activate a Federate TLD that
appears in this file is rejected.

## Why public TLDs are blocked

If `.com` existed inside Federate, `google.com` in a Federate-configured
browser could resolve to Federate content instead of the real Google; a
perfect phishing/impersonation machine, and it would break normal internet
DNS for daemon users. So `.com`, `.net`, `.org`, `.br`, `.dev`, `.app`,
`.live`, `.page`, `.games`, `.network`, `.google`, `.apple`, `.bank`, `.gov`,
and every other IANA TLD can never be created inside Federate:

```
$ federate tld check com
[blocked] .com - .com cannot be created because it is a public IANA/ICANN TLD
(blocked_tlds.txt); Federate never collides with the normal internet
```

This guarantee is what lets the future local DNS resolver safely answer
Federate TLDs locally and forward everything else upstream; the two
namespaces are disjoint by construction.

## The additional blocklists (`data/blocked/`)

| File | Purpose |
|---|---|
| `reserved-tlds.txt` | Federate reserved names: infrastructure, governance, safety, future use (`fed`, `root`, `admin`, `registry`, `status`, `nodes`, `protocol`, `system`). Reserved names cannot be applied for by users; the root itself may register them as official TLDs (that's how `.fed` exists). |
| `policy-tlds.txt` | Policy blocklist (phishing patterns, legal, governance decisions). Placeholder - populated by future governance. |
| `brand-safety-tlds.txt` | Brand/safety blocklist. Placeholder - populated by future governance. |

Files are created with defaults on first server start if missing. Check
order: IANA → reserved → policy → brand-safety; the first match wins and its
reason is reported by `federate tld check <tld>` and the `/v1/tld-check/:tld`
endpoint.

## Keeping `blocked_tlds.txt` current

IANA adds/removes TLDs occasionally. Refresh from the official source:

```sh
curl -s https://data.iana.org/TLD/tlds-alpha-by-domain.txt | grep -v '^#' > blocked_tlds.txt
```

then restart `federate-server`.
