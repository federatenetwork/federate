# Configuração no Desktop: Como um Amigo Entra na Rede

> [English version](../en-US/desktop-setup.md)

Objetivo: digitar `http://home.fed` no Chrome/Safari/Firefox/Edge e entrar na
Federate Network.

## O comando único (macOS, Linux, Windows)

macOS ou Linux, no terminal:

```sh
curl -fsSL https://federate.network/install.sh | bash
```

Windows, no PowerShell:

```powershell
iex (irm https://federate.network/install.ps1)
```

O instalador baixa a CLI `federate` e roda `federate setup`, que faz
quatro coisas e depois prova que funcionam:

1. **Autoridade certificadora local, estilo mkcert.** CAs públicas não
   emitem para `.fed`, então o setup gera uma CA na SUA máquina (a
   chave privada nasce ali e nunca sai; uma CA compartilhada da rede
   poderia se passar por qualquer site HTTPS e por isso jamais é usada)
   e adiciona o certificado público dela ao repositório de confiança do
   sistema. Resultado: `https://home.fed` com cadeado verde.
2. **Resolvedor local verificador + gateway como serviço do sistema.**
   O `federate dns proxy --local-gateway` roda em `127.0.0.1:53`
   (launchd no macOS, systemd no Linux, tarefa de boot SYSTEM no
   Windows). Ele responde nomes sob todo TLD da **zona raiz assinada**,
   que ele atualiza continuamente contra a chave raiz fixada; não existe
   lista de TLDs no cliente, então um TLD criado amanhã resolve em toda
   máquina instalada em um minuto. Nomes Federate apontam para um
   gateway em loopback (http 80 + https 443) que busca o conteúdo pelo
   protocolo Federate e verifica toda a cadeia de assinaturas/hashes na
   sua máquina antes de servir um único byte; certificados por nome são
   emitidos pela CA local no primeiro uso. Todo nome não-Federate é
   encaminhado ao DNS upstream sem nenhuma alteração.
3. **DNS do sistema apontado para o resolvedor.** As configurações
   anteriores são salvas e restauradas exatamente por
   `federate dns uninstall` (que também remove a CA da confiança do
   sistema).
4. **Links `fed://` registrados** para abrir no navegador (abaixo).

Teste ao vivo no final: `home.fed` precisa resolver via `127.0.0.1:53`,
ser servido pelo gateway e completar um handshake TLS verificado contra
a CA local, senão o setup diz exatamente qual passo falhou.

Para gerenciar depois:

```sh
federate dns status          # estado do serviço + DNS do sistema
sudo federate dns uninstall  # restaura o DNS anterior, remove o serviço
sudo federate setup          # faz tudo de novo
```

Já roda algo na porta 53 (dnsmasq, um DNS de desenvolvimento)? O
instalador detecta e muda para outro endereço de loopback sozinho
(`127.53.0.1:53` e assim por diante; configurações de DNS do sistema
aceitam apenas um IP, então a saída é um endereço dentro de 127.0.0.0/8,
nunca uma porta). Seu serviço existente não é tocado.

Por que isso é melhor que arquivo hosts: nada é fixo no código, TLDs e
domínios novos aparecem sozinhos, as respostas trazem vários gateways
saudáveis com TTL de 30s e a assinatura da zona raiz é verificada na sua
máquina.

## Links fed:// clicáveis

O `federate setup` já registra isso. Para fazer sozinho (por usuário,
sem admin, sem assinatura de código; macOS, Linux e Windows):

```sh
federate handler install     # registrar (uninstall / status também existem)
open fed://home.fed          # teste no macOS; Linux: xdg-open, Windows: start
```

No macOS isso gera um applet AppleScript minúsculo em `~/Applications`
(criado localmente, então o Gatekeeper nunca o coloca em quarentena); no
Linux escreve uma entrada `.desktop` com `x-scheme-handler/fed`; no
Windows escreve chaves de registro por usuário. Os três apenas reescrevem
`fed://nome/caminho` para `http://nome/caminho`, então a resolução de
nomes continua vindo da sua configuração de DNS Federate.

## Compilando do código-fonte

```sh
git clone https://github.com/federatenetwork/federate && cd federate
cargo build --release -p federate-cli
sudo ./target/release/federate setup
```

Para rodar um daemon local completo (gateway na porta 80, cache local),
veja [port-80-setup.md](port-80-setup.md) e [nodes.md](nodes.md). O
caminho antigo por arquivo hosts ([hosts-setup.md](hosts-setup.md)) ainda
funciona, mas é estático; o serviço de resolvedor o substitui.

## Verificação

```sh
federate doctor     # diagnóstico completo com correções
federate dns status # serviço de resolvedor + DNS do sistema
federate open home.fed
```

Os sites visitados são armazenados em cache localmente e continuam
funcionando mesmo quando o Node 1 está temporariamente offline.
