# Assinaturas e Cadeia de Confiança

> [English version](../signatures.md)

## Chaves em um parágrafo

Um par de chaves tem uma **chave privada** (secreta, prova a posse ao produzir
assinaturas) e uma **chave pública** (um identificador público, seguro de
publicar; conhecê-la permite que qualquer pessoa *verifique* assinaturas, mas
nunca as *crie*). No Federate, chaves públicas SÃO a camada de identidade:
donos de TLD, operadores de TLD e donos de domínio são todos chaves públicas.
A posse de qualquer coisa é provada por uma assinatura da chave privada
correspondente.

## A cadeia de confiança

```
Federate Root Key
  → TLD Record        (signed by the Root Key)
    → Domain Record   (signed by the TLD operator key named in the TLD record)
      → Site Manifest (signed by the domain owner key named in the domain record)
        → Content Blocks (verified by BLAKE3 hash listed in the manifest)
```

O Node 1 é um **distribuidor de dados assinados, não uma autoridade
confiável**. O daemon confia em assinaturas válidas e hashes de conteúdo,
nunca nas respostas do servidor. Um Node 1 comprometido ou falsificado não
consegue forjar nenhum registro sem as chaves privadas correspondentes; o
daemon rejeita os dados e continua servindo a última zona verificada em
cache.

### 1. Chave Raiz do Federate

Autoridade máxima. Assina a zona raiz e todos os registros de TLD (oficiais e
delegados). A chave raiz **pública** é configurada no `federated` via
`--root-key <hex>`, ou fixada no primeiro uso (TOFU) e persistida em
`<data-dir>/trusted-root-key`. A chave raiz **privada** vive apenas no host do
registro raiz (`.federate-server/root/identity.key` em dev) e nunca é
embutida no daemon nem exposta por nenhuma API.

### 2. Registros de TLD

Assinados pela Chave Raiz. O daemon rejeita registros de TLD sem assinatura
ou com assinatura inválida.

### 3. Registros de domínio

Assinados pela chave do operador do TLD. Antes de prosseguir, o daemon
verifica: o TLD existe; seu registro é validado contra a Chave Raiz; a
assinatura do registro de domínio corresponde à chave de operador autorizada
naquele registro de TLD; o domínio está `active`; o domínio realmente
pertence àquele TLD.

### 4. Manifests de site

Assinados pela chave do dono do domínio. O daemon verifica: o registro de
domínio é válido; os bytes do manifest baixado produzem o hash igual ao
`manifest_hash` do registro de domínio; a assinatura do manifest é válida; o
assinante é igual ao `owner_public_key` do registro de domínio; o `domain` do
manifest é igual ao domínio solicitado.

### 5. Blocos de conteúdo

Verificados por hash: os bytes baixados devem corresponder ao hash no
manifest, e os bytes em cache são re-hasheados antes de cada serviço. Blocos
inválidos são rejeitados e removidos do cache.

## Formato canônico de assinatura

Algoritmo: **Ed25519**, assinaturas codificadas em hex. Os payloads assinados
são canonicalizados antes da assinatura, para que as assinaturas nunca
dependam da formatação:

- serialize o registro para JSON com o campo `signature` definido como `null`
  removido / `None` (o campo é excluído do payload assinado),
- a forma canônica é **JSON compacto** (sem espaços em branco) com **chaves de
  objeto ordenadas lexicograficamente em todos os níveis de aninhamento**;
  arrays mantêm sua ordem,
- assine os bytes resultantes; armazene a assinatura em hex no campo
  `signature`.

Nunca assine JSON formatado (pretty-printed) ou com chaves em ordem de
inserção. Implementação: `federate_core::canonical::canonical_bytes` + o
`signable_bytes()` de cada registro.

Todo objeto assinado carrega: os campos do payload, `signature`,
`signature_algorithm` (`"ed25519"`), a chave pública do assinante relevante
(`root_public_key` / `operator_public_key` / `owner_public_key`) e os
timestamps `created_at` / `updated_at`, além de campos de versão.

## Proteção contra replay

Imposta, não apenas recomendada:

- **Rollback da zona raiz**: os daemons lembram o `root_version` da última
  zona verificada (memória + cache em disco) e rejeitam uma zona corretamente
  assinada porém *mais antiga* vinda de qualquer nó ou mirror. O Node 1 deriva
  o `root_version` do relógio no momento da assinatura, então ele é monotônico
  entre reinicializações.
- **Expiração de registros**: o `expires_at` (RFC 3339) nos registros de TLD e
  de domínio é checado em toda resolução (gateway, DNS, visão do registro,
  busca delegada). Um registro expirado para de resolver mesmo que sua
  assinatura ainda seja criptograficamente válida; um `expires_at` que não
  pode ser interpretado conta como expirado (fail closed).

Futuras APIs de mutação (registrar/atualizar TLDs e domínios em tempo de
execução) DEVEM usar adicionalmente nonces emitidos pelo servidor ou
desafio-resposta, para que uma requisição assinada capturada não possa ser
reexecutada.

## Quando a verificação falha

O daemon serve uma **página de erro de segurança do Federate** estilizada,
indicando qual camada falhou (root / tld / domain / manifest / content), para
qual domínio e por quê, e não serve o conteúdo. `federate doctor`,
`federate root verify`, `federate tld verify <tld>`, `federate domain verify
<domain>` e `federate manifest verify <domain>` reproduzem cada checagem pela
linha de comando.

## Trabalho futuro

- **Rotação de chaves**: substituição das chaves raiz e de operador via
  registros de transição com assinatura cruzada.
- **Chaves de recuperação**: chaves secundárias pré-registradas para
  recuperação de conta do dono.
- **Multisig**: TLDs caros/de alto valor protegidos por assinaturas m-de-n.
- **Mirrors da raiz**: múltiplos mirrors distribuindo a mesma zona assinada
  pela raiz (as assinaturas tornam os mirrors trustless).
