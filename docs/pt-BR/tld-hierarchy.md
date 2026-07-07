# A Hierarquia de TLDs do Federate

> [English version](../en-US/tld-hierarchy.md)

A Federate Network opera um namespace web paralelo com raiz própria. A camada
raiz não vende todos os domínios diretamente para sempre; ela controla **quais
TLDs existem**, quais estão reservados ou bloqueados e **quem opera** cada TLD
delegado.

```
Federate Root Registry      controls TLDs (exists? official? delegated? blocked?)
  → TLD Operator            controls domains inside that TLD
    → Domain Registrant     owns a specific domain
      → Site/Manifest Owner controls the site's signed manifest and content
```

Essas camadas nunca são achatadas em um único registro plano.

## Registro Raiz do Federate

Controlado pela Federate Network, inicialmente servido pelo Node 1 em
`federate.network`. Ele mantém: TLDs ativos/oficiais/delegados/reservados/
bloqueados/desabilitados/pendentes/expirados/revogados, a lista de bloqueio da
IANA, os nomes reservados do Federate, listas de bloqueio de política e de
proteção de marca, registros de propriedade de TLD, chaves públicas de
operadores, endpoints/manifests de registro, políticas, metadados de
expiração/renovação, campos reservados de metadados de preço e eventos de
auditoria.

Ele responde: este TLD existe? qual o status? quem é o dono? quem o opera?
onde fica seu registro? ele tem permissão para existir? ele colide com o DNS
público ICANN/IANA? está bloqueado por razões de segurança/legais/de
governança?

## Status de TLD

| Status | Significado |
|---|---|
| `official` | Operado pela própria Federate Network (23 TLDs hoje; conjunto completo em `federate-naming`) |
| `delegated` | Operado por um usuário/operador (ex.: `.femboy`) |
| `reserved` | Não pode ser comprado - infraestrutura/governança/segurança/uso futuro (`root`, `admin`, `registry`, …) |
| `blocked` | Não pode ser criado - colisão com DNS público, marca, phishing, política (`com`, `net`, `dev`, `app`, …) |
| `disabled` | Existe, mas está temporariamente sem resolução |
| `pending` | Solicitação existe, ainda não aprovada |
| `expired` | Propriedade/concessão expirada |
| `revoked` | Removido pela governança da raiz (abuso, inadimplência, questões legais, emergência) |

## Papéis e chaves

- **Dono do TLD** (`owner_public_key` no registro de TLD): o dono
  econômico/legal do TLD.
- **Operador do TLD** (`operator_public_key`): a chave autorizada a rodar o
  registro do TLD e assinar registros de domínio sob ele. Frequentemente é a
  mesma chave do dono, mas pode ser separada (um dono pode contratar um
  operador).
- **Registrante/dono do domínio** (`owner_public_key` no registro de
  domínio): a chave autorizada a publicar/atualizar o manifest daquele
  domínio.
- **Dono do site/manifest**: assina o manifest; deve ser a chave do dono do
  domínio.

Exemplo: o Federate delega `.femboy` a um usuário. Esse usuário se torna o
Operador do TLD. Outros usuários então registram `eu.femboy`, `joao.femboy`,
`wiki.femboy` (emitidos pelo operador de `.femboy`, e **não** pelo Federate).
A raiz do Federate registra apenas que `.femboy` existe, quem é o dono/quem o
opera, qual registro ele usa e se está ativo.

## Por que isso evita colisões com a internet normal

Toda criação de TLD é validada contra `blocked_tlds.txt`: a lista pública
completa de TLDs da IANA/ICANN. `.com`, `.net`, `.org`, `.br`, `.dev`,
`.app`, `.live`, `.page`, `.google`, `.bank`, `.gov`, … nunca poderão existir
dentro do Federate. Um nome do Federate, portanto, nunca sombreia um nome real
da internet, e o futuro resolvedor DNS local pode encaminhar com segurança
tudo o que não for Federate para o DNS normal. Ver
[blocked-tlds.md](blocked-tlds.md).

## Como registros delegados resolvem

TLDs delegados resolvem de verdade. Um TLD delegado publica um **registro de
TLD assinado**: um documento listando todos os registros de domínio que o
operador emitiu, assinado pela chave de operador nomeada no registro de TLD
assinado pela raiz. O `registry_type` do registro de TLD diz como esse
registro é distribuído:

| Modo | Onde o registro vive | Modelo de atualização |
|---|---|---|
| `root_managed` | os registros de domínio ficam na própria zona raiz assinada | a raiz re-assina a zona (TLDs oficiais) |
| `delegated_manifest` | manifest de registro endereçado por conteúdo, fixado por `registry_manifest_hash` | o operador publica novos bytes, a raiz re-assina o registro de TLD com o novo hash |
| `delegated_native` | providers nativos de registro em `registry_providers` (`GetTldRegistry`) | o operador atualiza livremente; clientes impõem proteção contra rollback de versão |
| `delegated_http` | endpoint HTTP do operador em `registry_endpoint` (`/v1/tld-registry/:tld`) | gêmeo de compatibilidade do `delegated_native`, mesmo documento assinado |

Resolvendo `fed://eu.femboy` (`.femboy` é a delegação seed neutra de dev):

1. interpretar a URI; extrair `eu.femboy` e `.femboy`,
2. carregar e verificar a zona raiz assinada contra a chave raiz fixada,
3. verificar o registro de TLD `.femboy` (assinado pela raiz) e seu
   status/expiração,
4. buscar o registro de `.femboy` pelo modo configurado e **verificar a
   assinatura do operador** contra a chave de operador do registro de TLD,
5. encontrar `eu.femboy` no registro e verificar a assinatura do operador,
   o status e a expiração do registro de domínio,
6. seguir pelo caminho inalterado de manifest/conteúdo (manifest assinado
   pelo dono, blocos verificados por hash).

O comportamento de falha é assimétrico de propósito:

- **assinatura inválida em qualquer ponto** (registro do TLD, registro de
  domínio): fail closed, erro de segurança, nada é servido;
- **registro inacessível**: cai para o último registro verificado em cache;
  sem nada em cache, uma resposta clara de "registro delegado indisponível";
- **TLD expirado/revogado/desabilitado/suspenso**: fail closed antes mesmo
  de buscar o registro;
- **domínio simplesmente ausente**: a resposta normal de domínio não
  encontrado.

Registros vivos (`delegated_native`/`delegated_http`) carregam um `version`
monotônico; clientes rejeitam um registro corretamente assinado porém mais
antigo do que um já verificado, então nem o host do operador nem um mirror
conseguem rebobinar o namespace (mesma regra de rollback da zona raiz).

É por isso que a raiz controla **a existência dos TLDs, mas não todos os
domínios para sempre**: uma vez que `.femboy` é delegado, a chave do
operador emite e assina domínios sob ele sem pedir nada à raiz; as únicas
alavancas da raiz são o próprio registro de delegação (status, expiração,
revogação). Inspecione e verifique a cadeia inteira com
`federate delegated-registry inspect femboy` e
`federate delegated-registry verify femboy`. Ver
[signatures.md](signatures.md) e
[tld-marketplace-roadmap.md](tld-marketplace-roadmap.md).
