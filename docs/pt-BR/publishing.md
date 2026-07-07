# Publicando sites

> [English version (en-US)](../en-US/publishing.md)

Publicar não exige mais acesso ao filesystem do Node 1. Um site vira um
pacote endereçado por conteúdo mais uma mutação assinada, e o Node 1 o
ingere em runtime depois de verificar a cadeia inteira. O `sites/` do
Node 1 é só seed de primeiro boot (veja [root-registry.md](root-registry.md)).

## TLDs oficiais: publicação em um passo

```sh
federate publish package ./dist --domain joao.pagina \
    --key-dir .federate-owner --bootstrap https://federate.network
```

Isso empacota `./dist` (precisa conter `index.html`), assina o manifest com
sua chave de dono, pede um nonce de mutação, assina uma mutação
`publish_site` e submete tudo para `POST /v1/ingest/package`. No sucesso o
domínio resolve imediatamente pelo protocolo nativo e pelo gateway HTTP:

```sh
federate fetch fed://joao.pagina/
```

Atualizar é o mesmo comando de novo (a versão do alvo avança), ou, se o
manifest já está no nó:

```sh
federate domain update joao.pagina --manifest <hash-do-novo-manifest> --key-dir .federate-owner
```

## Dois passos: empacotar antes, submeter depois

```sh
federate site package ./dist --domain joao.pagina --key-dir .federate-owner
federate registry submit-package ./dist.federate-package --key-dir .federate-owner
```

`site package` continua funcionando exatamente como antes (empacotamento
offline, `--install` opcional em um nó local). `registry submit-package` lê
o diretório do pacote, assina a mutação de publish com a mesma chave de
dono e submete.

## O que o Node 1 verifica antes de aceitar

A ingestão de pacote falha fechada. Antes de qualquer estado mudar:

- limites do pacote (32 MiB decodificado, 2048 blocos) e decodificação hex;
- o hash de cada bloco bate com seu conteúdo;
- os bytes do manifest batem com o `manifest_hash` da mutação;
- o envelope da mutação verifica ([mutations.md](mutations.md)):
  assinatura, nonce, janela de timestamp, histórico de replay, versão do
  alvo;
- o TLD existe, é root-managed, resolvível e não expirado (TLDs delegados
  publicam pelo próprio operador);
- o domínio ou ainda não existe (first-come sob TLDs oficiais, por
  enquanto) ou pertence à chave que assina e está num status que permite
  atualização;
- o manifest valida, é assinado pelo ator e nomeia exatamente esse domínio.

Só então o registro de domínio é criado/atualizado, contra-assinado pela
chave do operador oficial, a zona re-assinada e persistida, e um evento de
auditoria assinado é anexado. Blocos e manifests são endereçados por
conteúdo, então uma mutação rejeitada não deixa autoridade para trás, só
bytes não referenciados.

## TLDs delegados

Registros de domínio de um TLD delegado vivem no registry assinado do
operador, não na zona raiz, então publicar lá continua com o ferramental de
operador:

```sh
federate site package ./dist --domain eu.femboy --key-dir .federate-owner
federate operator sign-record eu.femboy --owner <chave-do-dono> --manifest-hash <hash>
federate operator build-registry femboy --records .
```

Novo: um operador `delegated_manifest` não precisa mais que a raiz edite
código seed para re-apontar o registry. O operador submete uma mutação
`update_registry_pointer` assinada com a chave de operador; a raiz
re-assina o registro do TLD com o novo hash do registry, e a versão do
registry precisa crescer estritamente (proteção contra rollback nos
clientes).

Criar a própria delegação também é runtime agora (exige a chave raiz):

```sh
federate tld delegate quintal --owner <hex> --operator <hex> --key-dir <dir-da-chave-raiz>
```

## Enforcement

```sh
federate domain suspend joao.pagina --key-dir <dir-da-chave-do-operador-ou-raiz>
federate domain reinstate joao.pagina --key-dir <dir-da-chave-do-operador-ou-raiz>
```

Um domínio suspenso para de resolver em todo lugar imediatamente (gateway,
protocolo nativo, checagens de existência do DNS) e rejeita atualizações do
dono até ser reativado. Revogação é terminal a menos que a chave raiz
reative.

## O que ainda falta

- pagamentos/marketplace: publicar é first-come e grátis nesta fase;
- replicação: o conteúdo vive no Node 1 mais os nós que o cacheiam ou fixam;
- rate limiting no endpoint de ingestão;
- uma UI web; tudo acima é CLI + API HTTP hoje.
