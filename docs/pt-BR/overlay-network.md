# Federate como rede overlay

> [English version](../en-US/overlay-network.md)

A Federate é um **overlay**: uma rede completa construída sobre o transporte
de pacotes da internet existente. O underlay (IP, roteamento, enlaces
físicos) fica como está; toda camada acima dele é da Federate.

## Mapa de camadas

| Camada | Internet normal | Federate |
|---|---|---|
| Endereçamento | URLs + nomes DNS | URIs `fed://` ([federate-uri.md](federate-uri.md)) |
| Autoridade de nomes | ICANN/registradores | zona raiz assinada + operadores de TLD ([tld-hierarchy.md](tld-hierarchy.md)) |
| Resolução de nomes | DNS | engine de resolução por zona assinada (`federate-resolution`) |
| Protocolo de aplicação | HTTP(S) | protocolo Federate ([native-protocol.md](native-protocol.md)) |
| Confiança | CAs + confiança de canal TLS | assinaturas por objeto + endereçamento por conteúdo ([signatures.md](signatures.md)) |
| Localização de conteúdo | servidores de origem | qualquer provider; o hash decide a validade ([storage-cdn-nodes.md](storage-cdn-nodes.md)) |
| Descoberta | buscadores + anúncios | diretório de nós + `.busca` (sem anúncios/rastreamento/IA) |
| Transporte | TCP/QUIC | mesmo TCP/QUIC (underlay, reaproveitado) |
| Pacotes, roteamento, física | IP/BGP/fibra | **fora de escopo, reaproveitado como está** |

As linhas de baixo são o ponto: a Federate reaproveita a entrega de pacotes
e substitui tudo que as pessoas de fato tocam.

## Papéis no overlay

Todo nó é participante de primeira classe do overlay ([nodes.md](nodes.md)):
root-authority (só a chave raiz), root-mirror, dns, gateway, storage, cdn,
search, bootstrap, origin. Descobrir nós é trabalho do diretório; validade
dos dados nunca é. Essa separação (disponibilidade vem dos nós, autoridade
vem das assinaturas) é o que permite estranhos servirem conteúdo uns aos
outros com segurança.

## Modelo de conteúdo

Conteúdo é endereçado por hash e servido por quem o tiver. Hoje: origem +
CDN fetch-on-miss + anúncios de provider assinados + caches LRU. O modelo se
estende a replicação, pinning e seleção do provider mais próximo sem novas
decisões de confiança, porque a identidade de um bloco É seu hash:
replicação é pura engenharia de disponibilidade.

## Duas portas, uma rede

- **Porta nativa**: `fed://` + protocolo Federate; o que clientes nativos e
  o navegador futuro usam.
- **Porta de compatibilidade**: ponte DNS + gateway HTTP
  ([browser-compatibility.md](browser-compatibility.md)); o que deixa
  qualquer celular ou navegador alcançar o mesmo conteúdo hoje.

As duas portas terminam na mesma engine de resolução e na mesma cadeia de
verificação. Remover a porta de compatibilidade um dia não mudaria a rede;
remover o núcleo nativo não deixaria nada.
