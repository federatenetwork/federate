# O Federate Root Registry persistente

> [English version (en-US)](../en-US/root-registry.md)

O Node 1 não reconstrói mais o estado da rede a partir de código seed e de
`sites/` a cada reinício. O Federate Root Registry é estado assinado,
durável e mutável em tempo de execução: sobrevive a reinícios e só muda em
runtime através de [mutações assinadas](mutations.md) e do
[caminho de ingestão de pacotes](publishing.md).

## Seed só no primeiro boot

Na primeira inicialização (sem estado de registry no disco), o
`federate-server` roda o seed exatamente uma vez:

1. TLDs oficiais são validados contra as blocklists e assinados pela raiz;
2. `sites/` é escaneado, arquivos viram endereços de conteúdo, manifests
   são assinados pelo dono, registros de domínio pelo operador;
3. TLDs delegados de seed (`.femboy`) ganham suas chaves de operador e
   registries;
4. a zona raiz montada é assinada, auto-verificada e adotada como o
   registry persistente inicial.

Em todo boot seguinte o registry persistente é a fonte da verdade. `sites/`
nunca é escaneado de novo, constantes de seed nunca são consultadas de
novo, e mudar a rede significa enviar uma mutação assinada, não editar
código.

## Layout em disco

Tudo vive em `<data_dir>/registry/` (padrão `.federate-server/registry/`):

| Caminho | Conteúdo |
|---|---|
| `state.json` | zona raiz assinada atual, registries delegados (bytes assinados exatos), versões de mutação por alvo |
| `manifests/<hash>` | bytes de manifests e registries endereçados por conteúdo |
| `blocks/` | blocos de sites endereçados por conteúdo (store BLAKE3) |
| `audit.jsonl` | log de auditoria assinado, append-only, um evento por linha |
| `mutations.jsonl` | histórico append-only de mutações aceitas |
| `snapshots/root-zone-v<N>.json` | um snapshot imutável da zona raiz por versão aceita |

Escritas são atômicas (escreve em `.tmp`, depois renomeia). Chaves privadas
NUNCA são guardadas nesses registros; elas ficam nos seus próprios arquivos
`identity.key`.

## Carga fail-closed

No boot o registry é re-verificado antes de ser servido:

- a zona raiz precisa validar estruturalmente e verificar contra a chave raiz;
- cada registry delegado precisa verificar contra a chave do operador
  nomeada no seu registro de TLD assinado pela raiz;
- cada manifest e bloco é conferido contra seu endereço de conteúdo
  (entradas corrompidas são descartadas, nunca servidas);
- um `state.json` adulterado para o nó em vez de servir dados forjados.

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
