# Nós de storage e CDN

> [English version](../storage-cdn-nodes.md)

Nós de storage e CDN guardam blocos endereçados por conteúdo e os servem aos
gateways. Eles **nunca são retornados aos navegadores pelo DNS**; navegadores falam com
gateways; gateways buscam blocos nos provedores.

## Papéis

- `storage`: armazena de forma durável os blocos que recebeu e os serve em
  `GET /v1/block/:hash`.
- `cdn`: mesma API, mas com fetch-on-miss: um bloco solicitado que ele não tem é
  puxado (com hash verificado) de outros provedores ou do Node 1, armazenado em cache e servido.
  O cache tem remoção LRU e é limitado por `[capacity] storage_gb`.

Ambos anunciam os hashes dos blocos em cache ao diretório de nós a cada minuto,
para que os gateways possam descobri-los como provedores (`GET /v1/providers/:hash`).

## Modelo de confiança

Provedores nunca são confiáveis:

- todo bloco que um gateway busca é reverificado contra seu hash BLAKE3
- um provedor que retorna bytes errados é detectado imediatamente e ignorado
- o próprio cache da CDN reverifica os blocos na leitura (corrupção de disco é detectada)

## Seleção de provedores

Os gateways classificam os provedores: online antes de degradado, mesma região primeiro, depois
menor latência, e fazem failover descendo a lista, terminando no Node 1 (origem do
conteúdo oficial).

## Rode um

```toml
# federate.toml
[node]
roles = ["cdn"]          # ou ["storage"], ou ["storage", "cdn"]
region = "br-sp"
public_ip = "x.x.x.x"

[network]
bootstrap = "https://federate.network"
root_key = "..."

[capacity]
storage_gb = 100
bandwidth_mbps = 500
```

```sh
federate-noded --config federate.toml
```
