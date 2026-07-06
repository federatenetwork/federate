# Roadmap de runtime além do HTML

> [English version](../en-US/non-html-runtime-roadmap.md)

O conteúdo Federate hoje é site estático: HTML, CSS, imagens, fontes, tudo
endereçado por conteúdo e assinado. Isso é o chão, não o teto. Este roadmap
esboça o que um runtime nativo Federate pode carregar quando o cliente
nativo existir, e o que continua verdadeiro em todo estágio.

## Invariantes (todo estágio)

- tudo é endereçado por conteúdo e assinado; conteúdo sem assinatura nunca
  roda
- capacidades são explícitas: um documento/app não ganha nada que não
  declarou e que o usuário não concedeu
- sem anúncios, sem rastreamento, sem treinamento de IA, sem feeds de
  engajamento
- o manifest continua sendo a unidade de publicação: um mapa assinado de
  nomes para hashes

## Estágios

1. **Conteúdo web estático** (hoje): HTML/CSS/JS servidos por gateways; JS
   roda na sandbox normal do navegador.
2. **Documentos nativos Federate**: documentos assinados markdown-first (o
   renderizador `fed-md` é a semente), renderizados pelo cliente nativo sem
   engine web. Layout determinístico, sem scripts, seguro por construção.
3. **Pacotes assinados**: um manifest que declara documento de entrada,
   assets e lista de permissões. Instalável, atualizável publicando uma nova
   versão assinada do manifest, verificável offline.
4. **Apps em sandbox**: pacotes com código (provavelmente WASM) rodando sob
   permissões explícitas concedidas pelo usuário (cota de storage, rede só
   para nomes Federate declarados, nenhuma autoridade ambiente).
5. **Tempo real e streaming**: extensões do protocolo para sessões e
   streams (jogos, mídia, mundos compartilhados) quando o transporte nativo
   carregar QUIC.

## Racional da sequência

Documentos antes de pacotes antes de apps: cada estágio reaproveita a
história de verificação do anterior e adiciona exatamente uma pergunta nova
de confiança. A resposta tem sempre o mesmo formato: assinado por quem,
concedido o quê, endereçado por hash.

Nada deste roadmap está implementado hoje, de propósito. O código mantém as
fronteiras (URI / protocolo / resolução / stores de conteúdo) limpas para
que cada estágio chegue como um novo consumidor das camadas existentes, não
como uma reescrita.
