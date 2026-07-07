*A human web, built by people.*

**This is the door. Three minutes and you are in.**

Federate names like `home.fed` do not exist in the old internet's phone
book. One setting fixes that, and the normal web keeps working.

## 1. One command and you are in (Mac, Linux, Windows)

**Mac or Linux** - paste in a terminal:

```
curl -fsSL https://federate.network/install.sh | bash
```

**Windows** - paste in PowerShell:

```
iex (irm https://federate.network/install.ps1)
```

This installs the `federate` CLI, starts a local verifying resolver
(every Federate TLD, present and future, answered from the signed root
zone), points your system DNS at it, makes `fed://` links clickable,
and self-tests. Undo anytime: `sudo federate dns uninstall`.

**iPhone or iPad instead?**

1. [Download the DNS profile](/federate-dns.mobileconfig)
2. Open the downloaded file
3. Settings → General → Device Management → **Install**

**Just a browser, no install (30 seconds)**

Chrome, Edge, or Firefox → Settings → **Secure DNS** → custom provider →
paste:

`https://federate.network/dns-query`

Works on any network: home, office, 4G. No router or ISP can block it.

## 2. Open your first Federate page

Go to [http://home.fed](http://home.fed)

If it opened: you are on the network. That page arrived signed by the root
key and hash-verified, block by block. If it did not open, step 1 has not
applied yet (restart the browser).

## 3. Go deeper

- **Read the manifesto** - [http://home.fed](http://home.fed) explains why
  this exists: no feeds, no scraping, no AI training.
- **Publish your site** - package a folder with an `index.html` and
  publish it under your own name:

```
federate publish package ./my-site --domain you.pagina
```

- **Use the command line** - the installer from step 1 already gave you
  the native way to browse:

```
federate fetch fed://home.fed/
```

- **Run a node** - serve DNS, gateway, or content for the network. Start
  at `docs/en-US/nodes.md` in the repository.

## What this is, in one sentence

A namespace of our own (`.fed`, `.pagina`, `.rosa`, `.mosca`, and 19
more), where every name and every byte arrives signed end to end, operated
by people, with no ads, no surveillance, and no AI scraping what is yours.
