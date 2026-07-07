# Backup e restore do root registry

> [English version (en-US)](../en-US/backups.md)

O registry vive num banco embutido redb:
`<data_dir>/registry/registry.redb`. Ele é o estado autoritativo da rede
(registros de TLD, registros de domínio, versões da zona raiz, histórico de
mutações, log de auditoria, nonces, ponteiros de registries delegados).
Trate o backup dele como backup de banco de dados.

## O que salvar

| Ativo | Onde | Como |
|---|---|---|
| Banco do registry | `<data_dir>/registry/registry.redb` | `federate registry backup` |
| Chaves privadas | `<data_dir>/root/`, `official-operator/`, `identity.key` | cópia offline, 0600, nunca no banco |
| Stores de conteúdo | `<data_dir>/registry/manifests/`, `registry/blocks/` | cópia simples de arquivos (endereçados por conteúdo, auto-verificáveis) |
| Blocklists | `blocked_tlds.txt`, `data/blocked/` | cópia simples (dados externos de política) |

Backup completo mais simples (servidor parado):

```sh
tar czf federate-backup.tgz -C /var/lib/federate data
```

## Backup do banco do registry

```sh
federate registry backup --output /backups/registry-$(date +%Y%m%d).redb \
    --data-dir /var/lib/federate/data
```

Rode com o `federate-server` parado: o banco tem escritor único e o comando
recusa (erro de lock) enquanto o servidor o segura. A cópia é aberta para
sanidade antes do comando reportar sucesso, e um arquivo de saída existente
nunca é sobrescrito.

## Restore

```sh
federate registry restore --input /backups/registry-20260707.redb \
    --data-dir /var/lib/federate/data [--force]
```

O restore recusa sobrescrever um banco existente sem `--force`. Depois de
copiar, o registry restaurado é COMPLETAMENTE re-verificado contra a chave
raiz (assinatura da zona, registries delegados, assinaturas de auditoria,
hashes de conteúdo, consistência de tabelas); um backup que falha na
verificação é reportado e não deve ser servido.

Restaurar o banco sem os diretórios `manifests/` e `blocks/`
correspondentes gera um registry cujos domínios apontam para conteúdo que o
nó ainda não tem; o nó serve os registros mas não o conteúdo até os blocos
chegarem (restaure-os do backup de arquivos, ou deixe a rede re-suprir
blocos cacheados).

## Ressalva sobre proteção de rollback

Clientes lembram a maior versão de zona raiz que já verificaram. Restaurar
um backup ANTIGO significa servir uma zona válida porém mais velha, que os
clientes rejeitam até novas mutações empurrarem a versão além do que eles
lembram. Restaure o backup mais novo disponível e, se preciso, avance a
versão com uma mutação real.
