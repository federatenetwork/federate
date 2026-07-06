# Federate URI: `fed://`

> [English version](../en-US/federate-uri.md)

O formato de endereçamento nativo da Federate Network, implementado uma vez
na crate `federate-uri` e usado por todo consumidor (engine de resolução,
gateway, CLI, navegador futuro).

```
fed://<rotulo>.<tld>[/caminho][?query]
```

Exemplos:

```
fed://home.fed
fed://joao.pagina/about
fed://fed.busca/?q=manifesto
fed://arcade.mosca/play?level=2
fed://fotolia.rosa/galeria/2026
```

## Regras

- o scheme é exatamente `fed`
- a autoridade é exatamente um rótulo + um TLD; as regras de sintaxe vêm de
  `federate-naming` (rótulo: a-z 0-9 hífen, 1-63; TLD: a-z, 2-32)
- **sem portas, sem userinfo, sem IPs literais**: um nome Federate nunca
  carrega endereço de transporte; de onde vêm os bytes é trabalho do
  resolvedor
- o caminho é absoluto e opcional (padrão `/`), limitado a 2048 caracteres
- a query é mantida como veio; o significado pertence ao site/app
- fragmentos (`#...`) são aceitos e descartados (assunto do cliente)
- a forma canônica omite o caminho raiz: `fed://home.fed`, não
  `fed://home.fed/`

O parsing é puramente sintático. Se `joao.pagina` existe é decidido pela
zona raiz assinada na hora da resolução, nunca pelo parser. Nenhum domínio é
especial: `fed://qualquer.tldvalido` parseia igual a `fed://home.fed`.

## Mapeamento de compatibilidade HTTP

O gateway traduz requisições de navegador para URIs nativas:

```
Host: joao.pagina        GET /about?x=1
            ↓
fed://joao.pagina/about?x=1
```

`FederateUri::from_http(host, caminho_e_query)` produz uma URI igual à do
parsing da forma nativa, então depois da tradução uma requisição HTTP e uma
nativa são indistinguíveis.

## CLI

```
federate inspect-uri fed://joao.pagina/about
federate resolve fed://home.fed
federate fetch fed://fotolia.rosa/ --output index.html
federate open fed://fed.busca
```

Todo comando aceita domínios puros (`home.fed`) e formas `http://` como
conveniência; internamente tudo vira uma `FederateUri`.
