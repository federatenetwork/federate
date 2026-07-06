# Diretório de nós

> [English version](../node-directory.md)

O diretório de nós rastreia os nós de infraestrutura Federate ativos. O
Node 1 hospeda o diretório oficial; `federate-directory` é uma biblioteca,
então outras implantações de diretório serão possíveis mais adiante.

O diretório é **apenas descoberta de infraestrutura**. Ele nunca decide
quais nomes ou conteúdos são válidos; essa autoridade permanece com a zona
raiz assinada.

## O que ele rastreia por nó

- `node_id` (= chave pública)
- `public_key`
- IPs públicos
- `region`
- `roles`
- status de saúde (`online` / `degraded` / `offline`)
- `latency_ms`
- `capacity` (storage_gb, bandwidth_mbps)
- `last_seen`

## Registro

`POST /v1/nodes/register` com um `NodeRegistration` assinado. O diretório:

- verifica a assinatura Ed25519 sobre o JSON canônico
- exige `node_id == public_key`
- rejeita o papel `root-authority` de qualquer chave que não seja a Federate
  Root Key

Os nós se registram novamente a cada 60 segundos.

## Verificação de saúde

O diretório consulta o `{health_endpoint}/health` de cada nó a cada 15
segundos:

- 200 → **online** (latência registrada)
- 1-2 falhas consecutivas → **degraded**
- 3 ou mais → **offline** (excluído das listagens de nós saudáveis e das
  respostas DNS)

## Nós obsoletos, limites, persistência

- Nós não vistos por **24 horas** (nenhuma verificação de saúde
  bem-sucedida, nenhum novo registro) são removidos por completo, incluindo
  suas entradas de provedor de blocos. Eles sempre podem se registrar de
  novo.
- O diretório rastreia no máximo **5000 nós**; novos registros além disso
  são rejeitados (atualizações de nós já conhecidos sempre têm sucesso). Os
  registros são autoassinados, então o limite protege a memória contra
  registros falsos em massa.
- O Node 1 persiste a tabela de nós em `data/directory-nodes.json`
  (gravação seguida de renomeação). Ao reiniciar, cada entrada do snapshot é
  verificada novamente - um snapshot adulterado não consegue injetar
  registros não verificáveis.

## API

| Endpoint | Finalidade |
|---|---|
| `POST /v1/nodes/register` | registro de nó assinado |
| `GET /v1/nodes?role=gateway&healthy=true` | lista nós (gateways saudáveis, nós DNS, espelhos, bootstrap…) |
| `GET /v1/nodes/:id` | um nó específico |
| `POST /v1/nodes/announce-blocks` | anúncios **assinados** de blocos de storage/CDN |
| `GET /v1/providers/:hash?role=cdn` | provedores de um bloco |

As listagens de nós saudáveis são ordenadas do melhor para o pior: online
antes de degraded, depois menor latência. É exatamente isso que os nós DNS
consomem para responder `home.fed` com múltiplos IPs de gateway.

## Regras antiabuso aplicadas no registro

O diretório é apenas descoberta, mas ainda assim recusa entradas que
poderiam ser usadas para atacá-lo ou para atacar outros nós:

- **Registros assinados.** `node_id` deve ser igual à chave pública, e o
  registro deve ser assinado por essa chave. Registros adulterados são
  rejeitados.
- **`root-authority` é fixado.** Somente a Federate Root Key pode registrar
  o papel `root-authority`; qualquer outra chave é recusada.
- **Sem SSRF via `health_endpoint`.** O endpoint deve ser uma URL `http(s)`
  cujo host seja um dos `public_ips` declarados pelo próprio nó, e cada
  `public_ip` deve ser interpretável como um IP real. Isso impede que um nó
  aponte o verificador de saúde do diretório (ou as buscas de blocos dos
  gateways) para um terceiro ou para um IP de metadados de nuvem.
- **Anúncios de blocos assinados.** `POST /v1/nodes/announce-blocks` carrega
  uma assinatura Ed25519 do nó anunciante; o nó já deve estar registrado, e
  hashes de bloco malformados são descartados. Ninguém consegue inflar a
  lista de provedores de outro nó. Os bytes dos blocos ainda passam por novo
  hash a cada busca, então um provedor mentiroso só desperdiça uma
  requisição antes de ser ignorado.

## CLI

```sh
federate directory list --role gateway
federate directory list --role dns --healthy
federate directory list --role storage
federate directory list --role cdn
```
