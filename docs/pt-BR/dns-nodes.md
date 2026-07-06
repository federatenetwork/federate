# Nós DNS

> [English version](../dns-nodes.md)

O `federate-dnsd` é um servidor DNS autoritativo para os TLDs Federate. Qualquer pessoa pode
rodar um.

## Comportamento

Para uma consulta como `home.fed`:

1. Confirma que `.fed` existe na **zona raiz assinada** (assinatura verificada
   contra a Federate Root Key fixada; uma zona não verificável nunca é usada).
2. Pergunta ao diretório de nós por **nós gateway saudáveis**.
3. Retorna **múltiplos** registros A/AAAA, nunca um único IP fixo:

   ```
   home.fed  A  45.1.1.1
   home.fed  A  45.2.2.2
   home.fed  A  45.3.3.3
   TTL: 30 seconds
   ```

   O diretório classifica os gateways por saúde e depois por latência; o TTL baixo faz com que
   gateways com falha saiam das respostas em segundos.

4. Qualquer nome não Federate (`google.com`, …) é encaminhado sem alterações para o DNS
   upstream (`1.1.1.1:53` por padrão, `--upstream 8.8.8.8:53` para mudar), então
   a resolução normal da internet nunca é quebrada.

Se nenhum gateway saudável existir, o servidor responde SERVFAIL em vez de um IP
obsoleto ou inventado.

Limites operacionais (implementação atual):

- As respostas são limitadas a **8 registros**, de modo que toda resposta cabe em uma
  resposta UDP simples de 512 bytes.
- **Somente UDP** por enquanto, sem listener TCP e sem EDNS. Truncamento nunca acontece
  por causa do limite de respostas, mas resolvedores que insistem em repetir via TCP não
  receberão resposta ainda.
- TLDs cujo registro na zona raiz está expirado (`expires_at` no passado) são
  tratados como não Federate e encaminhados ao upstream como qualquer outro nome.
- O encaminhamento ao upstream usa um socket conectado novo por consulta (porta de origem
  aleatória) e exige um ID de transação DNS correspondente, então respostas forjadas
  fora do caminho são descartadas.

O DNS só responde *para onde um nome deve ir*. Os gateways ainda verificam toda a
cadeia root → TLD → domínio → manifest → bloco antes de servir qualquer coisa.

## Rode um

```sh
federate-dnsd \
  --listen 0.0.0.0:53 \
  --bootstrap https://federate.network \
  --root-key <FEDERATE_ROOT_PUBLIC_KEY_HEX> \
  --public-ip <YOUR_PUBLIC_IP> \
  --region br-sp
```

- `--root-key` fixa a âncora de confiança (fortemente recomendado; caso contrário a chave
  é fixada no primeiro uso).
- `--public-ip` registra o nó no diretório (papel `dns`) e inicia
  a API de saúde (`--health-listen`, padrão `0.0.0.0:8053`).
- A porta 53 exige privilégios; use `setcap` no Linux ou execute a unit do systemd.

Teste:

```sh
federate dns test home.fed --server <ip>:53
federate dns test example.com --server <ip>:53   # encaminhado ao upstream
```
