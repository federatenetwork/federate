# Migrando o armazenamento do registry (arquivos JSON para redb)

> [English version (en-US)](../en-US/migrations.md)

Nós antigos persistiam o registry como arquivos JSON (`state.json`,
`mutations.jsonl`, `audit.jsonl`, arquivos de snapshot). Esse layout foi
aposentado: o store autoritativo agora é o banco embutido redb
`registry.redb` (veja [root-registry.md](root-registry.md)). Um nó com o
layout antigo recusa iniciar e aponta para cá.

## Migração única

```sh
# 1. pare o servidor
systemctl stop federate-server        # ou mate o processo de dev

# 2. migre (valida tudo antes)
federate registry migrate-json-to-redb --data-dir /var/lib/federate/data

# 3. suba o servidor de novo
systemctl start federate-server
```

O que o comando faz:

1. carrega `state.json` + logs JSONL + arquivos de snapshot;
2. VALIDA tudo contra a chave raiz do nó: assinatura da zona, cada registry
   delegado contra a chave do seu operador, cada assinatura de evento de
   auditoria; qualquer falha aborta a migração sem escrever banco algum;
3. escreve o banco redb numa transação inicial (registros, zona atual,
   versões antigas da zona vindas dos snapshots, histórico de mutações, log
   de auditoria, versões por alvo, ponteiros de registries delegados);
4. move os arquivos JSON antigos para `registry/legacy-json-backup/`
   (guardados, não apagados);
5. imprime um relatório da migração.

Stores de conteúdo (`manifests/`, `blocks/`) e arquivos de snapshot são
endereçados por conteúdo e ficam exatamente onde estão; nada muda em chaves
ou blocklists.

## Depois de migrar

```sh
federate registry db stats     # contagens de tabelas
federate registry db verify    # verificação completa
federate root status --data-dir /var/lib/federate/data
```

Proteção contra replay, versões por alvo e a cadeia de auditoria
sobrevivem à migração sem mudanças; uma mutação já aplicada no layout JSON
continua rejeitada como replay no redb. Quando estiver satisfeito, o
diretório `legacy-json-backup/` pode ir para armazenamento frio.

## Nós novos

Nada a migrar: `federate root init` + `federate root seed` criam o banco
diretamente (veja [root-registry.md](root-registry.md)).
