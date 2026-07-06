# HTTPS interno e CA raiz local do Federate (Planejado)

> [English version](../https-local.md)

## Posição no MVP

O fluxo do MVP é `http://home.fed`. O `https://federate.network` público usa
Let's Encrypt normal (domínio público real, tratado pelo Caddy; veja
[deployment-vps.md](deployment-vps.md)).

Domínios internos (`home.fed`, `joao.pagina`, `fotolia.rosa`, `arcade.mosca`, …)
**não podem** usar Let's Encrypt: são TLDs internos do Federate sem DNS
público, então nenhuma CA pública jamais emitirá certificados para eles.

## Opcional hoje: mkcert

Para desenvolvedores que querem `https://home.fed` localmente agora:

```sh
mkcert -install
mkcert home.fed docs.fed "*.pagina" "*.rosa" "*.cara" "*.mosca"
```

Depois, rode um terminador TLS local (ou uma futura flag `federated --tls`) com o
certificado gerado. Isso é opcional e não faz parte da documentação do fluxo para amigos.

## Planejado: CA raiz local do Federate (fase 7)

O instalador desktop vai:

1. Gerar uma chave de CA raiz do Federate por máquina (que nunca sai do dispositivo).
2. Instalá-la nos repositórios de confiança do SO e dos navegadores (como o mkcert faz).
3. Fazer o `federated` emitir certificados folha de curta duração por domínio Federate
   sob demanda e terminar o TLS em `127.0.0.1:443`.
4. Os navegadores então carregam `https://home.fed` com o cadeado válido.

Notas de design:

- CA por máquina (não uma CA compartilhada da rede) - o comprometimento de uma
  máquina nunca afeta as outras, e nenhuma chave privada de CA é distribuída.
- A emissão de certificados fica ao lado do gateway, reutilizando `federate-identity`
  para o manuseio de chaves; a resolução permanece intocada em `federate-resolution`.
- HTTP na porta 80 continua como fallback/redirecionamento.
