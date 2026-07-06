# O futuro navegador Federate

> [English version](../en-US/future-federate-browser.md)

Um cliente nativo Federate que fala o protocolo diretamente, trata `fed://`
como o scheme de casa da barra de endereços e não precisa das pontes DNS ou
HTTP. Ainda não implementado; esta página define as fronteiras que o código
já respeita, para que construí-lo seja montagem, não cirurgia.

## O que ele é

- a barra de endereços aceita `fed://qualquer.tldvalido/caminho`
- resolução, verificação e fetch pela mesma engine `federate-resolution`
  que o gateway e a CLI usam hoje
- transporte via `federate-transport` (protocolo nativo), com o cliente HTTP
  de compatibilidade como fallback, não como fundação
- âncora de confiança local: a chave raiz fixada vive no dispositivo do
  usuário; o navegador verifica tudo sozinho, sem confiar em gateway

## Fronteiras já no lugar

| Necessidade do navegador futuro | Onde já vive |
|---|---|
| parsear/normalizar endereços | `federate-uri` |
| resolver + verificar qualquer domínio | `federate-resolution::resolve_uri` |
| falar com nós nativamente | `federate-protocol` + `federate-transport` |
| descobrir nós/providers | cliente do `federate-directory` |
| cache local, leituras verificadas | block store do `federate-storage` |
| identidade/chaves | `federate-identity` |

A casca do navegador (renderização, abas, UI) é a única parte genuinamente
nova.

## Além do HTML

O primeiro renderizador é sem graça de propósito: HTML/CSS (o que os sites
publicam hoje). O modelo de documentos fica deliberadamente aberto para
mais; veja [non-html-runtime-roadmap.md](non-html-runtime-roadmap.md):

- documentos nativos Federate (assinados, endereçados por conteúdo,
  markdown primeiro)
- pacotes de aplicação assinados com permissões explícitas
- jogos e experiências em tempo real
- streaming de mídia e de mundos

## Não-objetivos

Os mesmos da rede: sem blockchain, sem anúncios, sem rastreamento, sem
treinamento de IA, sem feeds de engajamento. Um navegador Federate que
espiona seu usuário não é um navegador Federate.
