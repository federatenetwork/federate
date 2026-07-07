*A human web, built by people.*

**This is the door. Three minutes and you are in.**

Federate names like `home.fed` do not exist in the old internet's phone
book. One setting fixes that, and the normal web keeps working.

## 1. Turn on Federate DNS (once)

Pick ONE path:

**iPhone or Mac (recommended, covers the whole device)**

1. [Download the DNS profile](/federate-dns.mobileconfig)
2. Open the downloaded file
3. Settings → General → Device Management → **Install**

**Browser only (30 seconds)**

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

- **Make fed:// links clickable** - after building the CLI (below), run
  `federate handler install` once and addresses like `fed://home.fed`
  open in your browser.
- **Use the command line** - the native way to browse:

```
git clone https://github.com/c3b/federatenetwork
cargo build --release -p federate-cli
federate fetch fed://home.fed/
```

- **Run a node** - serve DNS, gateway, or content for the network. Start
  at `docs/en-US/nodes.md` in the repository.

## What this is, in one sentence

A namespace of our own (`.fed`, `.pagina`, `.rosa`, `.mosca`, and 19
more), where every name and every byte arrives signed end to end, operated
by people, with no ads, no surveillance, and no AI scraping what is yours.
