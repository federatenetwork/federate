# TLDs bloqueados

> [English version](../blocked-tlds.md)

## `blocked_tlds.txt`

O arquivo `blocked_tlds.txt` na raiz do repositório é a **lista pública
oficial de bloqueio de TLDs IANA/ICANN**: a lista completa de TLDs que
existem na zona raiz da internet convencional (cerca de 1.400 nomes). O
`federate-server` a carrega na inicialização (flag `--blocked-tlds`); ela é
dado, nunca fixada no código-fonte. Um nome por linha, sem distinção entre
maiúsculas e minúsculas, comentários com `#` são permitidos.

Qualquer tentativa de criar, solicitar, aprovar ou ativar um TLD Federate
que apareça nesse arquivo é rejeitada.

## Por que TLDs públicos são bloqueados

Se `.com` existisse dentro da Federate, `google.com` em um navegador
configurado para a Federate poderia resolver para conteúdo Federate em vez
do Google real; uma máquina perfeita de phishing/falsificação de identidade,
e isso quebraria o DNS da internet convencional para os usuários do daemon.
Por isso `.com`, `.net`, `.org`, `.br`, `.dev`, `.app`, `.live`, `.page`,
`.games`, `.network`, `.google`, `.apple`, `.bank`, `.gov` e todos os outros
TLDs da IANA jamais podem ser criados dentro da Federate:

```
$ federate tld check com
[blocked] .com - .com cannot be created because it is a public IANA/ICANN TLD
(blocked_tlds.txt); Federate never collides with the normal internet
```

Essa garantia é o que permitirá ao futuro resolvedor DNS local responder
TLDs Federate localmente com segurança e encaminhar todo o resto para o
upstream; os dois espaços de nomes são disjuntos por construção.

## As listas de bloqueio adicionais (`data/blocked/`)

| Arquivo | Finalidade |
|---|---|
| `reserved-tlds.txt` | Nomes reservados da Federate: infraestrutura, governança, segurança, uso futuro (`fed`, `root`, `admin`, `registry`, `status`, `nodes`, `protocol`, `system`). Nomes reservados não podem ser solicitados por usuários; a própria raiz pode registrá-los como TLDs oficiais (é assim que `.fed` existe). |
| `policy-tlds.txt` | Lista de bloqueio de política (padrões de phishing, questões legais, decisões de governança). Espaço reservado - será preenchido pela governança futura. |
| `brand-safety-tlds.txt` | Lista de bloqueio de marca/segurança. Espaço reservado - será preenchido pela governança futura. |

Os arquivos são criados com valores padrão na primeira inicialização do
servidor, caso não existam. Ordem de verificação: IANA → reservados →
política → marca/segurança; a primeira correspondência vence e seu motivo é
informado por `federate tld check <tld>` e pelo endpoint
`/v1/tld-check/:tld`.

## Mantendo o `blocked_tlds.txt` atualizado

A IANA adiciona/remove TLDs ocasionalmente. Atualize a partir da fonte
oficial:

```sh
curl -s https://data.iana.org/TLD/tlds-alpha-by-domain.txt | grep -v '^#' > blocked_tlds.txt
```

e depois reinicie o `federate-server`.
