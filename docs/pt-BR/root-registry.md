# O Federate Root Registry persistente

> [English version (en-US)](../en-US/root-registry.md)

O Node 1 não reconstrói mais o estado da rede a partir de código seed e de
`sites/` a cada reinício. O Federate Root Registry é estado assinado,
durável e mutável em tempo de execução: sobrevive a reinícios e só muda em
runtime através de [mutações assinadas](mutations.md) e do
[caminho de ingestão de pacotes](publishing.md).

## O banco de dados é a única fonte da verdade

Nenhum TLD existe em código compilado. Não há lista de TLDs hardcoded em
nenhuma lógica de runtime: TLDs oficiais, delegados, nomes reservados e
bloqueados são todos TldRecords comuns no registry persistente (mais os
arquivos de dados das blocklists). Adicionar, atualizar, suspender ou
remover um TLD nunca exige editar código-fonte nem recompilar nada.

## Inicialização e seed explícitos

Um nó novo faz bootstrap em passos explícitos; o servidor nunca cria TLDs
sozinho:

```sh
federate root init --data-dir .federate-server         # registry vazio assinado, ZERO TLDs
federate root seed --file seeds/official-tlds.toml --data-dir .federate-server
federate-server                                        # serve o que o banco contém
federate publish package ./site --domain home.fed      # conteúdo chega pela ingestão
```

`seeds/official-tlds.toml` é dado em TOML puro (entradas `[[tlds]]` com
`name`, `mode`, `purpose`). O comando de seed valida cada nome (regras de
nomes + blocklists), cria TldRecords assinados pela raiz pelo caminho
normal de mutações auditadas e assina uma nova versão da zona raiz. Editar
o arquivo de seed não muda NADA até o comando rodar de novo, e o comando
recusa um registry já populado; `--force` só adiciona entradas faltantes,
nunca sobrescreve registros existentes.

Se o `federate-server` inicia sem registry no disco, ele inicializa um
VAZIO (zero TLDs) e loga como fazer o seed. Ele nunca cria TLDs a partir de
código, nem no primeiro boot nem em nenhum outro. TLDs também podem ser
criados num nó em execução com mutações assinadas:

```sh
federate tld create quintal --purpose "..." --key-dir <dir-da-chave-raiz>
federate tld reserve tesouro --reason "..." --key-dir <dir-da-chave-raiz>
federate tld block golpe --reason "..." --key-dir <dir-da-chave-raiz>
federate tld delegate outra --owner <hex> --operator <hex> --key-dir <dir-da-chave-raiz>
```

## Layout em disco

Tudo vive em `<data_dir>/registry/` (padrão `.federate-server/registry/`):

| Caminho | Conteúdo |
|---|---|
| `registry.redb` | o banco embutido autoritativo (redb): tabelas `tld_records`, `domain_records`, `root_zone_versions`, `mutations`, `audit_events`, `snapshots`, `nonces`, `registry_metadata`, `delegated_registries`, `target_versions` |
| `manifests/<hash>` | bytes de manifests e registries endereçados por conteúdo |
| `blocks/` | blocos de sites endereçados por conteúdo (store BLAKE3) |
| `snapshots/root-zone-v<N>.json` | cópias legíveis da zona raiz (os bytes assinados também estão no banco) |

Cada mutação aceita comita em UMA transação de banco: ou aplica inteira ou
não aplica nada, e um crash no meio de uma mutação deixa o estado anterior
intacto. Nonces também são persistentes, então um desafio consumido nunca
pode ser reusado, nem depois de reiniciar. Chaves privadas NUNCA são
guardadas no banco nem em registro algum; elas ficam nos seus próprios
arquivos `identity.key` 0600. Blocklists continuam arquivos externos de
política (`blocked_tlds.txt`, `data/blocked/*`). O layout JSON antigo
(`state.json` + logs JSONL) foi aposentado; veja
[migrations.md](migrations.md) para converter um nó existente e
[backups.md](backups.md) para backup/restore.

## Carga fail-closed

No boot o registry é re-verificado antes de ser servido:

- a zona raiz precisa validar estruturalmente e verificar contra a chave raiz;
- cada registry delegado precisa verificar contra a chave do operador
  nomeada no seu registro de TLD assinado pela raiz;
- cada manifest e bloco é conferido contra seu endereço de conteúdo
  (entradas corrompidas são descartadas, nunca servidas);
- um registro adulterado no banco (ex.: zona forjada) para o nó em vez de
  servir dados forjados; `federate registry db verify` ainda confere as
  tabelas de registros contra a zona assinada.

## Versões da zona raiz e proteção contra rollback

O seed deriva `root_version` do relógio. Cada mutação aceita depois
re-assina a zona com `max(anterior + 1, agora)`, então a versão é
estritamente monotônica entre mutações E reinícios. Clientes mantêm a
proteção contra rollback existente: uma zona corretamente assinada porém
mais antiga é rejeitada. Snapshots antigos existem para auditoria e
recuperação, mas o servidor só serve a zona atual.

## Inspecionando o registry

```sh
federate registry status                 # versão, contagens, tamanho do histórico
federate registry audit --limit 50      # o log de auditoria assinado
federate registry verify                 # pede ao nó para auto-verificar tudo
federate registry snapshot               # força um snapshot da zona raiz
federate mutation inspect <mutation_id> # uma mutação aceita + seu evento de auditoria
federate registry db stats               # contagens de tabelas + tamanho do banco (offline)
federate registry db verify              # verificação offline completa incl. consistência de tabelas
federate registry backup --output <arq>  # copia o banco (offline; veja backups.md)
federate registry restore --input <arq>  # restaura + re-verificação completa
```

Equivalentes HTTP: `GET /v1/registry/status`, `GET /v1/registry/audit`,
`GET /v1/registry/verify`, `POST /v1/registry/snapshot`,
`GET /v1/mutations/:id`.

## O que isso destrava e o que ainda falta

Agora possível em runtime, com assinaturas e auditoria:

- publicar e atualizar domínios de TLDs oficiais ([publishing.md](publishing.md));
- delegar TLDs (`federate tld delegate`);
- suspender / reativar / revogar domínios;
- operadores delegados re-apontando o hash do registry através da raiz.

Ainda pela frente antes de marketplace e pagamentos:

- fluxos de aplicação/aprovação e pagamento (mutações existem; comércio não);
- rotação e recuperação de chaves ([signatures.md](signatures.md), trabalho futuro);
- rate limiting e canal de denúncia nos endpoints de mutação;
- autoridade raiz multi-nó (hoje um Node 1 guarda a chave raiz).
