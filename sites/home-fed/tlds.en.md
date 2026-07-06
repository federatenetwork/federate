# Namespaces

Every Federate name is one label plus one TLD, like `home.fed` or
`you.pagina`. TLDs live in the signed root registry: no server, node, or
gateway can invent one.

| TLD | What it is for |
|---|---|
| `.fed` | Official Federate namespace: specs, registry, status, governance |
| `.pagina` | Personal sites, blogs, portfolios, essays |
| `.rosa` | Creative, visual, poetic, art-oriented spaces |
| `.cara` | Identity, profiles, people pages, public cards |
| `.mosca` | Weird internet: experiments, memes, small games, underground pages |
| `.busca` | Federate search and discovery (`fed.busca`): no ads, no tracking |
| `.types` | Typography, type design, lettering, fonts |

All are official and root-operated for now. Delegated, community-operated
TLDs arrive in later phases.

## How a name becomes a page

```
domain → root zone → TLD → domain record → manifest → content blocks → your browser
```

Every arrow is a verification; if any one fails, nothing is served:

1. **Root zone** - the signed map of all TLDs. Your machine checks its
   signature against the pinned root key; a tampering server is rejected.
2. **TLD record** - signed by the root key; says who operates the TLD.
3. **Domain record** - signed by the TLD operator key; says who owns the
   name and points at the site's manifest.
4. **Manifest** - signed by the domain owner key; lists every file of the
   site and the content hash of each one.
5. **Content blocks** - every file is checked byte for byte against its
   hash before it reaches your browser.

No server is blindly trusted: nodes distribute signed data, the signatures
decide what is valid.

## Check a name

```
federate tld check yourname
federate domain check you.pagina
```

Domain registration and new-TLD applications open in later phases; no
payments for now.

[← back to home.fed](/)
