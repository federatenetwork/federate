# Arquitetura

> [English version](../en-US/architecture.md)

## Camadas

A resolução deliberadamente **não** está embutida no gateway HTTP. O motor
central (`federate-resolution`) é reutilizado por todos os consumidores atuais
e futuros:

```
                    ┌─────────────────────────┐
  browser ──────────▶ federate-gateway (HTTP) │──┐
  CLI/desktop ──────▶ federated local API     │──┤
  future DNS ───────▶ federate-dns (boundary) │──┼──▶ federate-resolution
  future publish ───────────────────────────────┤        │
  future peer/CDN ──────────────────────────────┘        ▼
                                              federate-root / naming / manifest
                                                         │
                                              federate-client ──▶ Node 1
                                              federate-storage ──▶ local block cache
```

## Responsabilidades separadas → crates

| Responsabilidade | Crate |
|---|---|
| 1. Carregamento/cache da zona raiz | `federate-root` |
| 2. Validação de TLD | `federate-naming` |
| 3. Resolução de registro de domínio | `federate-resolution` |
| 4. Busca/cache de manifests | `federate-resolution` + `federate-manifest` |
| 5. Busca/cache de blocos de conteúdo | `federate-storage` + `federate-client` |
| 6. Verificação de hash | `federate-storage` (BLAKE3, verificado no download E na leitura do cache) |
| 7. Serviço do gateway HTTP | `federate-gateway` |
| 8. Futuro resolvedor DNS | `federate-dns` (crate de fronteira, ver dns-resolver.md) |
| 9. Futura descoberta de peers/CDN | Stubs `/v1/nodes`, `/v1/peers` do Node 1 + campo `nodes` no `DomainRecord` |

Além disso: `federate-core` (tipos/erros/configuração), `federate-identity` (chaves Ed25519),
`federate-client` (cliente da API do Node 1), `federate-cli`, `federated` (daemon),
`federate-server` (Node 1).

## Fluxo de resolução

```
Host: home.fed, Path: /
  → FederateDomain::parse       (naming: TLD validation)
  → Resolver.root()             (memory → disk cache → Node 1)
  → RootZone.lookup("home.fed") (domain record: manifest hash, NOT an IP)
  → Resolver.manifest(hash)     (cache → Node 1, hash-verified)
  → Manifest.resolve_path("/")  ("/" → entry file → content hash)
  → Resolver.block(hash)        (block cache → Node 1, hash-verified)
  → gateway serves bytes with guessed MIME
```

Registros de domínio resolvem para **identidades** (hoje, o hash do manifest;
identidades de dono, serviço e nó são campos reservados para fases
posteriores), nunca diretamente para IPs públicos.

## Resiliência offline

A zona raiz, os manifests e os blocos ficam todos em cache no disco. Quando o
Node 1 está inacessível, os sites visitados anteriormente continuam
funcionando a partir do cache.

## Por que DNS sozinho não basta

Um resolvedor DNS só responde *para onde um nome deve ir* (no caso do
Federate: 127.0.0.1). O daemon/runtime ainda cuida da validação da zona raiz,
da resolução de registros de domínio, dos manifests, dos hashes de conteúdo,
do cache, da descoberta de peers, do CDN, da replicação, da identidade dos
nós, da publicação e de servir o conteúdo ao navegador. Ver
[dns-resolver.md](dns-resolver.md).
