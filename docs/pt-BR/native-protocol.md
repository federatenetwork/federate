# O protocolo nativo Federate

> [English version](../en-US/native-protocol.md)

O protocolo que nós Federate e clientes nativos falam entre si. Os endpoints
HTTP (`/v1/root`, `/v1/block/:hash`, ...) são a superfície de
compatibilidade para navegadores e ferramentas comuns; **este protocolo é a
superfície nativa em torno da qual a rede é construída**.

Crates:

- `federate-protocol`: mensagens, framing, negociação de versão
- `federate-transport`: como os frames viajam (TCP com frames hoje, QUIC
  planejado)

## Formato da sessão (v0)

Toda sessão começa com um handshake:

```
cliente                          nó
  | Hello {versions, node_id,     |
  |        agent}                 |
  |------------------------------>|
  |            Welcome {version,  |
  |             node_id, agent,   |
  |             capabilities}     |
  |<------------------------------|
  |  ... loop requisição/resposta |
```

- a negociação escolhe a maior versão em comum; sem versão em comum a
  resposta é `Error { code: unsupported }` e a conexão fecha
- `node_id` é a chave pública do par (hex): identidade, não autoridade. O
  que um nó pode alegar continua limitado pelas assinaturas dos dados.
- capabilities dizem ao cliente quais requisições valem a pena
  (`root`, `manifests`, `blocks`, `providers`)

## Requisições e respostas (v0)

| Requisição | Resposta | Notas |
|---|---|---|
| `GetRoot` | `Root { zone_json }` | o receptor DEVE verificar a assinatura da zona contra sua chave raiz fixada |
| `GetManifest { hash }` | `Manifest { hash, bytes }` | o receptor DEVE verificar que os bytes têm o hash do endereço |
| `GetBlock { hash }` | `Block { hash, bytes }` | o receptor DEVE verificar que os bytes têm o hash do endereço |
| `GetProviders { hash }` | `Providers { hash, nodes_json }` | consultivo; blocos buscados são verificados por hash de qualquer forma |
| `GetStatus` | `Status { roles, region, root_version, ... }` | diagnóstico |
| qualquer coisa | `Error { code, detail }` | `unsupported`, `not-found`, `bad-request`, `unavailable` |

Planejado para versões futuras: troca de descoberta de pares, handshakes
assinados (prova de posse da chave), rate limits por capability,
push/subscribe para atualizações de zona.

## Framing e encoding (v0)

- uma mensagem = prefixo de tamanho de 4 bytes big-endian + corpo JSON
- teto de frame: 68 MiB (blocos são limitados a 64 MiB; o envelope precisa
  de folga)
- JSON agora, deliberadamente: trivial de depurar, portável em todo lugar.
  Um encoding binário pode chegar como versão 1 pela mesma negociação, então
  escolher JSON hoje não custa nada amanhã.

## Transporte

`federate-transport` é orientado a mensagens de propósito: quem chama só vê
`send(Message)` / `recv() -> Message`. Hoje isso roda sobre TCP (porta
padrão **4077**, que é 0xFED) com:

- timeout por operação (15s)
- validação do tamanho do frame antes de alocar
- conexões simultâneas limitadas (256) e requisições por conexão (10k)

QUIC/UDP é o segundo transporte planejado; como a API é orientada a
mensagens, trocar o socket não toca na lógica do protocolo nem em quem
chama. IPv6 funciona onde o endereço de bind for IPv6 (dual-stack do tokio).

## Modelo de confiança

O transporte carrega confiança **zero**. Zonas raiz, registros de TLD,
registros de domínio, manifests e blocos são verificados pelo receptor
contra a chave raiz fixada e os endereços de conteúdo, exatamente como no
caminho HTTP. O protocolo move bytes; assinaturas decidem o que é válido.
Um nó malicioso pode se recusar a responder; não consegue forjar uma
resposta que verifique.

## Servindo o protocolo

O `federate-noded` escuta nativamente em `native_listen` (padrão
`0.0.0.0:4077`) e responde `GetRoot`, `GetBlock` e `GetStatus` a partir dos
mesmos stores verificados que suas rotas HTTP usam. Existe uma engine de
resolução e um conjunto de stores; as superfícies nativa e de
compatibilidade são duas portas para a mesma sala.
