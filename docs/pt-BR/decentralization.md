# Modelo de descentralização

> [English version](../en-US/decentralization.md)

A Federate Network separa **autoridade** de **infraestrutura**.

## Princípio central

> A Federate Root decide o que é válido.
> Os nós Federate distribuem, resolvem, armazenam em cache, espelham, buscam e servem dados válidos.

`federate.network` (Node 1) continua sendo a autoridade raiz oficial para
TLDs. Todo o resto (DNS, gateways, armazenamento, CDN, busca, bootstrap,
espelhamento da raiz) pode ser executado por qualquer pessoa.

## O que já é descentralizado hoje

| Função | Quem pode executar | Binário / papel |
|---|---|---|
| Resolução DNS | qualquer pessoa | `federate-dnsd` / `dns` |
| Gateways HTTP | qualquer pessoa | `federate-gatewayd` / `gateway` |
| Armazenamento de blocos | qualquer pessoa | `federate-noded` / `storage` |
| CDN / cache | qualquer pessoa | `federate-noded` / `cdn` |
| Busca | qualquer pessoa | `federate-searchd` / `search` |
| Bootstrap | qualquer pessoa | `federate-noded` / `bootstrap` |
| Espelhamento da zona raiz | qualquer pessoa | `federate-noded` / `root-mirror` |
| Hospedagem de origem | donos de domínio | `federate-noded` / `origin` |

## O que ainda NÃO é descentralizado

A autoridade sobre TLDs. Somente a Federate Root Key controla e assina:

- TLDs oficiais
- TLDs delegados (e as chaves de seus operadores)
- TLDs bloqueados
- TLDs reservados
- propriedade de TLDs
- chaves de operadores de TLD

Por quê: um espaço de nomes precisa de uma fonte de verdade única,
consistente e resistente a abusos antes de poder ser federado ainda mais com
segurança. Como todo registro de TLD é *assinado* pela chave raiz, nenhum nó
(espelho, DNS, gateway, nem mesmo o Node 1) pode forjar ou alterar dados de
TLD sem que isso seja detectado. Descentralizar primeiro o transporte, e a
governança depois, mantém a rede honesta em cada etapa.

## Cadeia de confiança

```
Federate Root Key
  └─ signs TLD records (root zone)
       └─ TLD Operator Key signs domain records
            └─ Domain Owner Key signs manifests
                 └─ manifests map paths → BLAKE3 content hashes
                      └─ every block is hash-verified
```

Regras aplicadas em todo o código:

- todos os dados da raiz devem ser assinados pela Federate Root Key
- todos os registros de TLD devem ser assinados pela Federate Root Key
- todos os registros de domínio devem ser assinados pela TLD Operator Key
- todos os manifestos devem ser assinados pela Domain Owner Key
- todos os blocos devem ser verificados por hash
- todos os registros de nós devem ser assinados pela chave do nó
- nós nunca recebem confiança às cegas - a verificação acontece no consumidor

## Como os dados fluem

```
browser ──DNS──▶ federate-dnsd ──▶ node directory (healthy gateways)
browser ──HTTP──▶ gateway node ──▶ resolution engine
                                     ├─ signed root zone (Node 1 or mirror)
                                     ├─ signed manifests
                                     └─ blocks (CDN/storage/origin providers, Node 1 fallback)
```

Os navegadores só falam com gateways. Os gateways entendem manifestos,
assinaturas, blocos e réplicas da Federate.

## Executando seu próprio nó

Consulte:

- [nodes.md](nodes.md) - papéis de nó, registro, configuração
- [dns-nodes.md](dns-nodes.md)
- [gateway-nodes.md](gateway-nodes.md)
- [storage-cdn-nodes.md](storage-cdn-nodes.md)
- [root-mirrors.md](root-mirrors.md)
- [node-directory.md](node-directory.md)
