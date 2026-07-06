# Nós gateway

> [English version](../en-US/gateway-nodes.md)

O `federate-gatewayd` serve sites Federate para navegadores comuns. Qualquer pessoa pode rodar
um; o DNS do Federate anuncia todos os gateways saudáveis.

## Comportamento

Em um `GET /` com `Host: home.fed` o gateway:

1. verifica a **zona raiz assinada** (verificada contra a chave raiz fixada)
2. verifica o registro do TLD (assinado pela raiz)
3. verifica o registro do domínio (assinado pela chave do operador do TLD)
4. busca o manifest assinado (endereçado por conteúdo + assinado pelo dono)
5. busca os blocos de conteúdo primeiro nos provedores CDN/storage/origem encontrados no
   diretório de nós (classificados por online → mesma região → menor latência),
   recorrendo ao Node 1 como fallback
6. verifica o hash de cada bloco
7. serve o HTML/CSS/JS/imagens

Qualquer falha de assinatura ou de hash significa que o conteúdo **não é servido**; uma página
de erro de segurança estilizada é retornada em seu lugar.

Navegadores falam com gateways; gateways falam com nós de storage/CDN/origem.
Navegadores entendem páginas HTTP; gateways entendem manifests, assinaturas,
blocos e réplicas do Federate.

## Rode um

```sh
federate-gatewayd \
  --listen 0.0.0.0:80 \
  --bootstrap https://federate.network \
  --root-key <FEDERATE_ROOT_PUBLIC_KEY_HEX> \
  --public-ip <YOUR_PUBLIC_IP> \
  --region br-sp
```

`--public-ip` registra o nó (papel `gateway`) e inicia a API de saúde
(`--health-listen`, padrão `0.0.0.0:8081`). Assim que o diretório marca o
nó como online, os nós DNS passam a incluir seu IP nas respostas automaticamente.

Teste:

```sh
federate gateway test home.fed --gateway http://<ip>:80
```
