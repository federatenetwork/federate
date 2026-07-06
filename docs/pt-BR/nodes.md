# Executando um nó Federate

> [English version](../nodes.md)

Qualquer pessoa pode executar infraestrutura Federate. Um nó é identificado
por um par de chaves Ed25519 gerado na primeira execução; sua chave pública
é seu `node_id`.

## Papéis

| Papel | O que faz |
|---|---|
| `root-authority` | Assina registros de TLD e a zona raiz. **Somente a raiz oficial da Federate pode executar este papel** - o diretório rejeita o papel vindo de qualquer outra chave. |
| `root-mirror` | Serve cópias assinadas da zona raiz. Não pode criar nem modificar TLDs. |
| `dns` | Responde consultas DNS da Federate com IPs de gateways saudáveis; encaminha todo o resto para o upstream. |
| `gateway` | Recebe requisições HTTP dos navegadores e serve sites Federate verificados. |
| `storage` | Armazena blocos endereçados por conteúdo. |
| `cdn` | Faz cache de blocos populares (LRU, com limite de tamanho) e os serve com rapidez. |
| `search` | Indexa páginas públicas (sem anúncios, sem rastreamento, sem treinamento de IA, opt-out respeitado). |
| `bootstrap` | Ajuda novos nós a descobrirem a rede. |
| `origin` | Hospeda o conteúdo original de um domínio/site. |

## Arquivo de configuração

Todo nó lê uma configuração TOML (padrão `federate.toml`):

```toml
[node]
roles = ["gateway", "cdn"]
region = "br-sp"
public_ip = "x.x.x.x"
# listen = "0.0.0.0:8080"       # serviço HTTP (saúde, blocos, gateway…)
# dns_listen = "0.0.0.0:5353"   # DNS UDP (apenas para o papel dns)

[network]
bootstrap = "https://federate.network"
root_key = "..."                 # fixe a chave pública da Federate Root (recomendado)
# directory = "https://federate.network"  # diretório de nós (padrão: o bootstrap)
# upstream_dns = "1.1.1.1:53"

[capacity]
storage_gb = 100
bandwidth_mbps = 500
```

## Execute

```sh
federate-noded --config federate.toml
# ou sobrescreva os papéis:
federate-noded --config federate.toml --roles gateway,dns,cdn
# ou via CLI:
federate node run --roles gateway,dns,cdn
```

Também existem daemons dedicados de papel único: `federate-dnsd`,
`federate-gatewayd`, `federate-searchd`.

## Registro

Na inicialização, o nó assina um registro com sua chave privada e o envia ao
diretório de nós, depois se registra novamente a cada 60s (heartbeat). O
registro contém: `node_id`, `public_key`, `roles`, IPs públicos, `region`,
`version`, `capacity`, `health_endpoint` e a assinatura. Registros não
assinados ou adulterados são rejeitados.

```sh
federate node register --config federate.toml
```

## API de saúde

Todo nó expõe:

- `GET /health` → `ok`
- `GET /status` → node_id, roles, region, version, capacity, uptime
- `GET /roles` → lista de papéis

O verificador de saúde do diretório consulta `/health` e marca os nós como
**online**, **degraded** (1-2 falhas consecutivas) ou **offline** (3 ou
mais).

## CLI

```sh
federate node status  --node http://45.1.1.1:8080
federate node roles   --node http://45.1.1.1:8080
federate node health  --node http://45.1.1.1:8080
federate node list --role gateway
federate directory list --role dns --healthy
federate dns test home.fed --server 45.1.1.1:53
federate gateway test home.fed --gateway http://45.1.1.1:8080
```
