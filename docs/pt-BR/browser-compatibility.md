# Compatibilidade com navegadores: as pontes

> [English version](../en-US/browser-compatibility.md)

Navegadores normais não falam o protocolo Federate nem entendem `fed://`. A
camada de compatibilidade existe para qualquer pessoa usar a Federate hoje
sem instalar nada; ela é um conjunto de **adaptadores** ao redor do núcleo
nativo, nunca uma implementação paralela.

## As três pontes

1. **Ponte DNS** (`federate-dnsd`): o dispositivo aponta seu DNS para um nó
   DNS Federate; TLDs Federate respondem com IPs de gateways saudáveis, todo
   outro nome é encaminhado ao upstream. O resto da internet continua
   funcionando.
2. **Gateway HTTP** (`federate-gatewayd`, `federated`): lê
   `Host: joao.pagina` + `/about`, traduz para `fed://joao.pagina/about` via
   `federate-uri` e chama a mesma engine de resolução de todo consumidor
   nativo. Não consegue contornar a verificação: não existe outro caminho de
   código.
3. **Modo gateway público**: um gateway em IP público atrás da ponte DNS
   serve celulares e computadores sem nenhum software Federate.

```
navegador --HTTP--> gateway --fed://joao.pagina/about--> engine de resolução
                                                          (assinaturas/hashes)
cliente nativo ---------fed://joao.pagina/about--------> mesma engine
```

## O que as pontes garantem

- `http://home.fed`, `http://joao.pagina`, `http://fotolia.rosa`,
  `http://fed.busca` continuam funcionando em qualquer navegador
- qualquer domínio válido sob qualquer TLD válido funciona; nada é
  hardcoded
- conteúdo servido pela ponte passou pela cadeia completa
  raiz → TLD → domínio → manifest → bloco
- respostas endereçadas por conteúdo carregam ETags fortes (o hash do
  bloco), então navegadores revalidam para 304

## O que as pontes não conseguem

- expressar capacidades nativas além de entrega HTML requisição/resposta
- carregar recursos futuros do protocolo nativo (descoberta de pares,
  subscribe, documentos não-HTML)
- adicionar confiança: um gateway comprometido pode se recusar a servir ou
  servir erros, mas não consegue forjar conteúdo que verifique contra sua
  chave raiz fixada. Quem precisa de verificação ponta a ponta roda o
  `federated` localmente ou um cliente nativo; o gateway público é o modo
  conveniência.

## Direção

As pontes ficam enquanto navegadores normais existirem. O caminho nativo
([native-protocol.md](native-protocol.md),
[future-federate-browser.md](future-federate-browser.md)) cresce ao lado
delas, não no lugar delas.
