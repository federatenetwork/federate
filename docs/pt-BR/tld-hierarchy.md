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

## Como registros delegados vão resolver (futuro)

Hoje apenas registros `root_managed` resolvem (os domínios vivem na zona raiz
assinada). Para um TLD delegado, o resolvedor já:

1. encontra o registro de TLD na zona raiz,
2. confirma que ele é delegado e está ativo,
3. lê seus `registry_type` / `registry_endpoint` / `registry_manifest_hash`,
4. …e para com um erro estruturado `DelegatedRegistryNotImplemented` e uma
   página de erro clara.

Na fase 6, o resolvedor buscará registros de domínio no registro do operador
(endpoint `delegated_http`, ou um `delegated_manifest` assinado), verificará
cada registro contra a chave de operador autorizada no registro de TLD
assinado pela raiz e continuará pelo caminho normal de manifest/conteúdo. A
cadeia de confiança nunca muda: a raiz assina o registro de TLD; a chave de
operador nomeada nele assina os registros de domínio. Ver
[signatures.md](signatures.md) e
[tld-marketplace-roadmap.md](tld-marketplace-roadmap.md).
