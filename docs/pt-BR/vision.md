# Visão: uma internet alternativa em overlay

> [English version](../en-US/vision.md)

A Federate Network não é um site, não é um truque de DNS e não é um projeto
de gateway HTTP. É uma **internet alternativa em overlay**: uma rede
completa (namespace próprio, protocolo próprio, nós próprios, descoberta,
modelo de conteúdo, busca) que roda sobre o encanamento da internet
existente.

## O que é da Federate

- **Namespace**: raiz própria, TLDs próprios, registros de domínio próprios,
  tudo assinado criptograficamente. Sem ICANN, sem registradores.
- **Endereçamento**: `fed://dominio/caminho` é o jeito nativo de nomear
  qualquer coisa.
- **Protocolo**: nós e clientes nativos falam o protocolo Federate
  ([native-protocol.md](native-protocol.md)), não HTTP.
- **Nós**: autoridade raiz, espelhos, DNS, gateways, storage, CDN, busca,
  bootstrap; qualquer pessoa roda qualquer papel exceto autoridade raiz.
- **Modelo de conteúdo**: tudo é endereçado por conteúdo (BLAKE3), assinado
  em toda camada, e servível por qualquer nó sem confiar nele.
- **Descoberta**: o diretório de nós e o bootstrap respondem "quem existe";
  as assinaturas respondem "o que é válido". Nunca a mesma pergunta.
- **Busca**: sem anúncios, sem rastreamento, sem treinamento de IA, opt-out
  respeitado.

## O que a Federate deliberadamente NÃO é

Infraestrutura física está fora de escopo, permanentemente:

- nenhuma camada de provedor, nenhum last-mile
- nada de BGP, ASN, acordos de peering
- nada de fibra, cabo, rádio, satélites
- nenhum substituto do roteamento IP global

A internet existente move pacotes muito bem. A Federate substitui as camadas
acima dos pacotes: nomes, confiança, publicação, descoberta e a relação
entre pessoas e rede.

## Compatibilidade é ponte, não produto

`http://home.fed` num navegador normal funciona e precisa continuar
funcionando; é assim que alguém experimenta a Federate sem instalar nada.
Mas HTTP, DNS e navegadores são **pontes de compatibilidade**
([browser-compatibility.md](browser-compatibility.md)). O núcleo é o
protocolo nativo e o caminho `fed://`; as pontes traduzem para ele e nunca
conseguem contornar sua verificação.

## Inegociáveis

- sem blockchain
- sem anúncios
- sem rastreamento
- sem treinamento de IA sobre conteúdo
- sem feeds de engajamento
- assinaturas decidem validade; servidores nunca são confiados às cegas

## Para onde isso vai

Um cliente/navegador nativo Federate
([future-federate-browser.md](future-federate-browser.md)) que fala o
protocolo diretamente, renderiza além de HTML
([non-html-runtime-roadmap.md](non-html-runtime-roadmap.md)) e trata o
overlay ([overlay-network.md](overlay-network.md)) como sua rede de origem,
com a web antiga a uma ponte de distância.
