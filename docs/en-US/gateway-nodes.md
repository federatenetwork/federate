# Gateway nodes

> [Versão em português (pt-BR)](../pt-BR/gateway-nodes.md)

`federate-gatewayd` serves Federate sites to normal browsers. Anyone can run
one; Federate DNS advertises all healthy gateways.

## Behavior

On `GET /` with `Host: home.fed` the gateway:

1. checks the **signed root zone** (verified against the pinned root key)
2. checks the TLD record (root-signed)
3. checks the domain record (signed by the TLD operator key)
4. fetches the signed manifest (content-addressed + owner-signed)
5. fetches content blocks from CDN/storage/origin providers found in the
   node directory first (ranked online → same region → lowest latency),
   falling back to Node 1
6. verifies every block hash
7. serves the HTML/CSS/JS/images

Any signature or hash failure means the content is **not served**; a styled
security error page is returned instead.

Browsers talk to gateways; gateways talk to storage/CDN/origin nodes.
Browsers understand HTTP pages; gateways understand Federate manifests,
signatures, blocks, and replicas.

## Run one

```sh
federate-gatewayd \
  --listen 0.0.0.0:80 \
  --bootstrap https://federate.network \
  --root-key <FEDERATE_ROOT_PUBLIC_KEY_HEX> \
  --public-ip <YOUR_PUBLIC_IP> \
  --region br-sp
```

`--public-ip` registers the node (role `gateway`) and starts the health API
(`--health-listen`, default `0.0.0.0:8081`). Once the directory marks the
node online, DNS nodes start including its IP in answers automatically.

Test it:

```sh
federate gateway test home.fed --gateway http://<ip>:80
```
