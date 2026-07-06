# Federate URI: `fed://`

> [Versão em português (pt-BR)](../pt-BR/federate-uri.md)

The native addressing format of the Federate Network, implemented once in
the `federate-uri` crate and used by every consumer (resolution engine,
gateway, CLI, future browser).

```
fed://<label>.<tld>[/path][?query]
```

Examples:

```
fed://home.fed
fed://joao.pagina/about
fed://fed.busca/?q=manifesto
fed://arcade.mosca/play?level=2
fed://fotolia.rosa/galeria/2026
```

## Rules

- scheme is exactly `fed`
- authority is exactly one label + one TLD; syntax rules come from
  `federate-naming` (label: a-z 0-9 hyphen, 1-63; TLD: a-z, 2-32)
- **no ports, no userinfo, no IP literals**: a Federate name never carries a
  transport address; where bytes come from is the resolver's job
- path is absolute and optional (defaults to `/`), capped at 2048 chars
- query is kept verbatim; its meaning belongs to the site/app
- fragments (`#...`) are accepted and discarded (client-side concern)
- canonical form omits the root path: `fed://home.fed`, not `fed://home.fed/`

Parsing is purely syntactic. Whether `joao.pagina` exists is decided by the
signed root zone at resolution time, never by the parser. No domain is
special: `fed://anything.validtld` parses the same as `fed://home.fed`.

## HTTP compatibility mapping

The gateway translates browser requests into native URIs:

```
Host: joao.pagina        GET /about?x=1
            ↓
fed://joao.pagina/about?x=1
```

`FederateUri::from_http(host, path_and_query)` produces a URI equal to
parsing the native spelling, so after translation an HTTP request and a
native request are indistinguishable.

## CLI

```
federate inspect-uri fed://joao.pagina/about
federate resolve fed://home.fed
federate fetch fed://fotolia.rosa/ --output index.html
federate open fed://fed.busca
```

Every command accepts bare domains (`home.fed`) and `http://` spellings as
convenience input; internally everything becomes a `FederateUri`.
