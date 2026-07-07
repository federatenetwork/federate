# Mutações assinadas

> [English version (en-US)](../en-US/mutations.md)

Uma mutação é o único jeito de o [root registry persistente](root-registry.md)
mudar em tempo de execução. Toda mutação é um envelope assinado; o servidor
verifica a assinatura, o nonce de desafio, o timestamp, a versão por alvo e
a autoridade do ator contra o estado ATUAL do registry antes de tocar em
qualquer coisa. Tudo falha fechado.

## O envelope

```json
{
  "mutation_id": "<blake3 do envelope canônico com id+assinatura em branco>",
  "nonce": "<nonce de uso único emitido pelo servidor>",
  "issued_at": "2026-07-06T12:00:00Z",
  "actor_public_key": "<chave ed25519 hex de quem assina>",
  "target_version": 3,
  "action": { "type": "publish_site", "domain": "joao.pagina", "manifest_hash": "..." },
  "signature_algorithm": "ed25519",
  "signature": "<assinatura hex sobre o envelope canônico>"
}
```

A assinatura usa as mesmas regras de JSON canônico de todos os objetos
assinados (veja [signatures.md](signatures.md)). O `mutation_id` é
auto-certificante: é o hash BLAKE3 do conteúdo do envelope, então um
request repetido ou alterado é detectável para sempre.

## Desafio-resposta (anti-replay)

1. `POST /v1/mutations/nonce` devolve um nonce aleatório de uso único com
   TTL de 5 minutos (`federate mutation nonce`).
2. O cliente embute o nonce no envelope e assina.
3. O servidor consome o nonce na submissão: reuso, expiração ou nonce
   desconhecido rejeitam a mutação com `409`.

Uma mutação TAMBÉM é rejeitada quando:

- a assinatura falta ou não é da `actor_public_key`;
- o `mutation_id` não bate com o conteúdo do envelope;
- `issued_at` está fora da janela de 5 minutos (timestamps ilegíveis
  contam como expirados, fail closed);
- o `mutation_id` já foi aplicado (histórico persistente, sobrevive a
  reinícios);
- `target_version` não avança estritamente a última versão aceita do alvo
  (tentativa de rollback);
- o ator não está autorizado para a ação (veja abaixo);
- o status do alvo não permite a transição.

## Ações e autorização

Autorização é conferida contra o estado assinado atual, nunca contra o que
o request alega.

| Ação | Assinante autorizado | Efeito |
|---|---|---|
| `publish_site` | chave do dono do domínio | criar/atualizar um domínio de TLD oficial a partir de um pacote ingerido |
| `update_domain_manifest` | chave do dono do domínio | apontar um domínio existente para um novo manifest assinado pelo dono |
| `set_domain_status` | chave do operador do TLD ou chave raiz | suspender / reativar / revogar um domínio root-managed |
| `issue_domain` | chave do operador do TLD | inserir um registro completo assinado pelo operador dentro do próprio TLD |
| `create_tld` | Federate Root Key | criar um TLD oficial root-managed (é assim que o conjunto de TLDs é definido; arquivos de seed alimentam esta ação) |
| `reserve_tld` | Federate Root Key | reservar um nome de TLD (registro não resolvível) |
| `block_tld` | Federate Root Key | bloquear um nome de TLD (registro não resolvível) |
| `delegate_tld` | Federate Root Key | criar um registro de TLD delegado |
| `update_tld` | Federate Root Key | atualizar metadados mutáveis do TLD (endpoint, expiração, notas) |
| `set_tld_status` | Federate Root Key | mudar o status de um TLD |
| `update_registry_pointer` | chave do operador do TLD delegado | fixar um novo registry assinado (versão precisa crescer) |

Transições de status de domínio: `active -> suspended`,
`suspended -> active`, qualquer coisa `-> revoked`; `revoked -> active`
exige a chave raiz (revogação pelo operador é terminal para o operador).
Escrever o mesmo status é rejeitado.

## O que uma mutação aceita produz

1. a zona (ou registro de TLD/domínio) é atualizada e re-assinada;
2. `root_version` sobe para `max(anterior + 1, agora)`: a proteção contra
   rollback dos clientes continua funcionando;
3. um [evento de auditoria](root-registry.md) assinado é anexado:
   `event_id`, `mutation_id`, `actor_public_key`, `actor_role`, `action`,
   `target_type`, `target_id`, `previous_state_hash`, `new_state_hash`,
   `timestamp`, `signature` (chave raiz);
4. a mutação é gravada em `mutations.jsonl` (proteção contra replay entre
   reinícios);
5. um novo snapshot da zona raiz é escrito.

## Superfície HTTP

| Endpoint | Propósito |
|---|---|
| `POST /v1/mutations/nonce` | emitir um nonce de desafio de uso único |
| `POST /v1/mutations` | submeter uma mutação assinada |
| `POST /v1/ingest/package` | submeter um pacote de site (blocos + manifest + mutação de publish) |
| `GET /v1/mutations/:id` | inspecionar uma mutação aceita |
| `GET /v1/mutations/target/:kind/:id` | versão atual/próxima de um alvo (`domain` ou `tld`) |

Rejeições: `403` não autorizado ou assinatura inválida, `409` replay /
nonce / rollback de versão, `404` alvo desconhecido, `400` malformado ou
transição proibida. O corpo sempre traz `{"accepted": false, "error": "..."}`.

## CLI

```sh
federate mutation nonce
federate mutation inspect <mutation_id>
federate domain update <domínio> --manifest <hash> --key-dir <dir-da-chave-do-dono>
federate domain suspend <domínio> --key-dir <dir-da-chave-do-operador-ou-raiz>
federate domain reinstate <domínio> --key-dir <dir-da-chave-do-operador-ou-raiz>
federate tld create <tld> --purpose "..." --key-dir <dir-da-chave-raiz>
federate tld reserve <tld> --reason "..." --key-dir <dir-da-chave-raiz>
federate tld block <tld> --reason "..." --key-dir <dir-da-chave-raiz>
federate tld delegate <tld> --owner <hex> --operator <hex> --key-dir <dir-da-chave-raiz>
```

Cada comando busca um nonce e a próxima versão do alvo, assina o envelope
localmente com a chave em `--key-dir` e submete. Chaves privadas nunca
saem da máquina.
