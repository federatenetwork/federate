# Implantando o Node 1 em um VPS

> [English version](../en-US/deployment-vps.md)

Este é o runbook de produção para a primeira implantação real: um VPS de
qualquer provedor (Hetzner, DigitalOcean, Vultr, OVH, AWS Lightsail, ...)
rodando Ubuntu ou Debian com toda a stack atrás do domínio público
`federate.network`. Os únicos requisitos do provedor: um IPv4 público, SSH
root e nenhum firewall do provedor bloqueando as portas 53/80/443.

O que roda na máquina:

| Serviço | Escuta em | Função |
|---|---|---|
| `federate-server` | 127.0.0.1:9000 | Node 1: zona raiz assinada, registry, diretório de nós, bootstrap |
| Caddy | 0.0.0.0:80 + 443 | TLS para `https://federate.network`, roteia qualquer outro Host na porta 80 para o gateway |
| `federate-gatewayd` | 127.0.0.1:8080 (+ health 0.0.0.0:8081) | Serve sites Federate após verificação completa de assinaturas |
| `federate-dnsd` | 0.0.0.0:53 UDP + TCP (+ health 0.0.0.0:8053) | Responde TLDs Federate com IPs de gateways saudáveis, encaminha o resto ao upstream |

O servidor DNS fala **UDP e TCP na porta 53** (TCP usa o framing com prefixo
de tamanho do RFC 7766). As respostas são limitadas a 8 registros, então
respostas UDP simples sempre cabem em 512 bytes; o TCP existe para stub
resolvers que insistem nele e para ferramentas como `dig +tcp`.

Fluxo ponta a ponta que isso habilita a partir de qualquer dispositivo externo:

1. O dispositivo configura seu servidor DNS para o IP do VPS.
2. O navegador abre `http://home.fed`.
3. O `federate-dnsd` responde `home.fed` com o IP do gateway (este VPS).
4. O navegador envia `Host: home.fed` para a porta 80; o Caddy entrega ao
   `federate-gatewayd`.
5. O gateway verifica assinatura da zona raiz, registro do TLD, registro do
   domínio, assinatura do manifest e hashes dos blocos, e então serve a página.
6. `google.com` etc. continuam funcionando: nomes não Federate são
   encaminhados ao DNS upstream com proteções anti-spoofing.

---

## 0. Checklist de implantação

- [ ] Compilar binários de release (§1)
- [ ] Criar usuário Linux + diretórios (§2)
- [ ] Copiar binários, sites, blocklists (§3)
- [ ] Instalar units do systemd + arquivo env do nó (§4)
- [ ] Instalar Caddy com o Caddyfile de roteamento por Host (§5)
- [ ] Configurar firewall / abrir portas (§6)
- [ ] Configurar registros DNS de `federate.network` (§7)
- [ ] Iniciar tudo, fixar a chave raiz (§8)
- [ ] Rodar health checks na máquina (§9)
- [ ] Testar DNS + gateway de fora (§10)
- [ ] Primeiro teste externo com amigos, pelo celular (§11)

Os comandos abaixo assumem Ubuntu 22.04/24.04 ou Debian 12 como root.
Substitua `<VPS_IP>` pelo IPv4 público do servidor em todo lugar.

## 1. Compilar os binários de release

No servidor (ou cross-compile local para `x86_64-unknown-linux-gnu`):

```sh
apt update && apt install -y build-essential pkg-config curl git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
git clone https://github.com/c3b/federatenetwork /opt/federatenetwork
cd /opt/federatenetwork
cargo build --release -p federate-server -p federate-dnsd -p federate-gatewayd -p federate-cli
```

## 2. Criar usuário e diretórios

```sh
useradd -r -m -d /var/lib/federate federate
mkdir -p /etc/federate
```

`federate-dnsd` e `federate-gatewayd` usam `DynamicUser` + `StateDirectory`
do systemd, então não precisam de usuário próprio.

## 3. Copiar binários e dados

A partir do checkout do repositório (local ou `/opt/federatenetwork`):

```sh
install -m 755 target/release/federate-server  /usr/local/bin/
install -m 755 target/release/federate-dnsd    /usr/local/bin/
install -m 755 target/release/federate-gatewayd /usr/local/bin/
install -m 755 target/release/federate         /usr/local/bin/
rsync -a sites/ /var/lib/federate/sites/
install -m 644 blocked_tlds.txt /var/lib/federate/blocked_tlds.txt
rsync -a data/blocked/ /var/lib/federate/blocked/
chown -R federate:federate /var/lib/federate
```

O `blocked_tlds.txt` (blocklist de colisões da IANA) é **obrigatório**: o
servidor se recusa a iniciar sem ele, então nenhum restart roda sem a
proteção contra colisões com o DNS público.

## 4. Units do systemd + ambiente do nó

```sh
cp deploy/systemd/federate-server.service   /etc/systemd/system/
cp deploy/systemd/federate-dnsd.service     /etc/systemd/system/
cp deploy/systemd/federate-gatewayd.service /etc/systemd/system/
cp deploy/federate-node.env.example         /etc/federate/node.env
chmod 600 /etc/federate/node.env
```

Edite `/etc/federate/node.env`:

- `PUBLIC_IP=<VPS_IP>` (precisa ser o IP público real: o diretório rejeita
  registro cujo host do health endpoint não seja um dos IPs declarados do
  nó, proteção anti-SSRF)
- `ROOT_KEY=` deixe para o §8 (o servidor imprime no primeiro start)
- `REGION=` p.ex. `de-fsn`

### Liberar a porta 53 (o Ubuntu vem com systemd-resolved nela)

O `systemd-resolved` segura `127.0.0.53:53`, o que conflita com o bind em
`0.0.0.0:53`. Tire a resolução da própria máquina do stub listener:

```sh
mkdir -p /etc/systemd/resolved.conf.d
printf '[Resolve]\nDNS=1.1.1.1 9.9.9.9\nDNSStubListener=no\n' \
  > /etc/systemd/resolved.conf.d/federate.conf
ln -sf /run/systemd/resolve/resolv.conf /etc/resolv.conf
systemctl restart systemd-resolved
```

## 5. Caddy (TLS + roteamento por Host na porta 80)

```sh
apt install -y caddy
cp deploy/caddy/Caddyfile /etc/caddy/Caddyfile
systemctl reload caddy
```

O Caddyfile roteia pelo header Host: `federate.network` vai para o
`federate-server` (com Let's Encrypt automático) e **qualquer outro Host na
porta 80 vai para o gateway**. Esse catch-all é o que serve
`http://home.fed`.

Sem Caddy? Rode o gateway direto na porta 80: edite
`federate-gatewayd.service` para `--listen 0.0.0.0:80` (a unit já concede
`CAP_NET_BIND_SERVICE`) e use nginx/certbot, ou nada, para a API de
`federate.network`.

## 6. Firewall

```sh
ufw allow 22/tcp        # ssh
ufw allow 80/tcp        # Caddy -> gateway + ACME
ufw allow 443/tcp       # Caddy -> API do Node 1 sobre TLS
ufw allow 53/udp        # DNS Federate
ufw allow 53/tcp        # DNS Federate (fallback TCP)
ufw allow 8081/tcp      # health endpoint do gateway (health checks do diretório)
ufw allow 8053/tcp      # health endpoint do nó DNS
ufw allow 4077/tcp      # protocolo Federate nativo (listener do Node 1)
ufw enable
```

O Node 1 em si fica em 127.0.0.1:9000 atrás do Caddy; nunca o exponha.

## 7. Registros DNS de federate.network

No registrador:

```
A     federate.network   <VPS_IP>
AAAA  federate.network   <VPS_IPv6>    (opcional)
```

Espere a propagação (`dig federate.network` do seu notebook mostra
`<VPS_IP>`), ou a emissão do Let's Encrypt no §5 falha até lá.

## 8. Iniciar os serviços e fixar a chave raiz

A ordem só importa por conveniência; tudo tenta de novo sozinho.

```sh
systemctl daemon-reload
systemctl enable --now federate-server
journalctl -u federate-server -n 20 --no-pager
```

O log de startup imprime a chave raiz:

```
root zone signed: T TLDs, N domains, M blocks (root key <64-hex>)
```

Coloque esse hex em `/etc/federate/node.env` como `ROOT_KEY=...`, e então:

```sh
systemctl enable --now federate-gatewayd federate-dnsd
```

Fixar a chave importa: com `ROOT_KEY` definido, o nó rejeita qualquer zona
que não seja assinada exatamente por essa chave. Sem ele, o nó fixa por
trust-on-first-use a chave que a primeira zona buscada anunciar (ok para
demo, inseguro em produção).

## 9. Health checks na máquina

```sh
curl -s https://federate.network/health            # -> ok
curl -s https://federate.network/v1/status | head  # root_version, tlds, ...
curl -s http://127.0.0.1:8081/health               # gateway -> ok
curl -s http://127.0.0.1:8053/health               # nó dns -> ok
curl -s -H "Host: home.fed" http://127.0.0.1:8080/ | head -3   # HTML do site
/usr/local/bin/federate directory list --bootstrap https://federate.network
# esperado: nós gateway + dns listados, status online (dê ~30s após o start)
```

O nó DNS responde SERVFAIL para nomes Federate nos primeiros ~10s (até o
primeiro refresh da lista de gateways). Espere, e então:

```sh
dig @127.0.0.1 home.fed +short        # -> <VPS_IP>
dig @127.0.0.1 home.fed +tcp +short   # mesma resposta via TCP
dig @127.0.0.1 google.com +short      # -> IPs reais do Google (encaminhado)
```

## 10. Validação externa (rode do seu notebook, NÃO do VPS)

```sh
dig @<VPS_IP> home.fed            # -> <VPS_IP>, TTL 30, flags incluem aa
dig @<VPS_IP> home.fed +tcp       # o mesmo via TCP 53
dig @<VPS_IP> google.com          # -> resposta encaminhada do upstream
curl -H "Host: home.fed" http://<VPS_IP>/          # -> HTML do site, 200
curl -sI -H "Host: home.fed" http://<VPS_IP>/ | grep -i etag   # hash do conteúdo
curl https://federate.network/v1/root | head      # zona assinada via TLS
```

Depois, o teste real no navegador:

1. Em um notebook ou celular, configure o servidor DNS para `<VPS_IP>`
   (configurações do Wi-Fi; no Android: Private DNS desligado + DNS manual
   nas configurações da rede; no iOS: Wi-Fi > Configurar DNS > Manual).
2. Abra `http://home.fed`.
3. A página carrega pelo gateway; falhas de verificação renderizam uma
   página de erro, nunca conteúdo não verificado.
4. Abra um site normal (google.com) para confirmar que o encaminhamento não
   quebra o resto da internet.

Acompanhe nos logs:

```sh
journalctl -u federate-dnsd -f       # consultas + refresh de gateways
journalctl -u federate-gatewayd -f   # serving HTTP
journalctl -u federate-server -f     # registry + diretório + health checks
```

## 11. Primeiro teste externo (só com amigos)

Mande para um amigo duas coisas: `<VPS_IP>` e o hex da chave raiz.

1. O amigo configura o DNS do dispositivo para `<VPS_IP>` (ou o DNS do
   roteador para a casa toda).
2. O amigo abre `http://home.fed` e, p.ex., `http://joao.pagina`.
3. O amigo restaura o DNS depois (é um teste, não um compromisso).

Amigos rodando o daemon desktop em vez de DNS puro:

```sh
federated --bootstrap https://federate.network --root-key <hex>
```

fixa a chave raiz explicitamente e verifica cada camada localmente; veja
[desktop-setup.md](desktop-setup.md).

O que coletar dos testadores: `home.fed` carrega, a navegação normal
continua funcionando, quão lento parece, horário exato de qualquer falha
(para casar com a saída do `journalctl`).

## 12. Rollback

Os binários não têm estado; o estado vive em `/var/lib/federate*` e é ou
re-derivável (zona raiz, manifests e blocos são reconstruídos de `sites/` a
cada start) ou auto-recuperável (nós se re-registram em ~60s).

Reverter um binário ruim:

```sh
systemctl stop federate-server federate-gatewayd federate-dnsd
# mantenha o binário anterior por perto na hora do deploy:
#   cp /usr/local/bin/federate-server /usr/local/bin/federate-server.prev
cp /usr/local/bin/federate-server.prev /usr/local/bin/federate-server
systemctl start federate-server federate-gatewayd federate-dnsd
```

Reverter uma publicação ruim de site:

```sh
rsync -a --delete <checkout-bom>/sites/ /var/lib/federate/sites/
systemctl restart federate-server
```

A zona re-assinada ganha um `root_version` novo (derivado do relógio), então
os daemons a aceitam; eles só rejeitam zonas **mais antigas** que uma já
verificada.

Tirar o DNS do ar sem mexer no resto:

```sh
systemctl stop federate-dnsd    # os dispositivos dos testadores caem para o
                                # DNS secundário para a internet normal
```

Desmontar tudo: `systemctl disable --now federate-server federate-dnsd
federate-gatewayd`, remova as regras do ufw, apague `/var/lib/federate*`.
Os testadores só removem o DNS customizado dos dispositivos.

**Nunca perca `/var/lib/federate/data/root/identity.key`** (veja backups
abaixo): binários e sites são substituíveis, a chave raiz não.

## Armazenamento de chaves & backups

As chaves privadas vivem no data dir do servidor (`/var/lib/federate/data`):
`root/`, `official-operator/`, `dev-owner/`, chaves de operador por TLD. São
gravadas com `0600` e nunca servidas por nenhuma API.

- Faça backup offline de `root/identity.key`. Perdê-la significa nunca mais
  assinar uma zona raiz nova; vazá-la compromete o namespace inteiro.
- Verifique as permissões após o primeiro start:
  `find /var/lib/federate/data -name identity.key -exec ls -l {} +`
  (toda chave `-rw-------`, dona `federate`).
- Backup sugerido (chaves + registry persistente + snapshot do diretório,
  NÃO os caches de blocos):
  `tar czf federate-backup.tgz -C /var/lib/federate data`, guardado fora da
  máquina. `data/registry/registry.redb` É o estado autoritativo da rede
  (um banco embutido redb: registros, versões da zona, histórico de
  mutações, log de auditoria, nonces); trate o backup como o de um banco de
  dados, de preferência com `federate registry backup` / `restore` (veja
  [backups.md](backups.md)). Nós atualizados do layout JSON antigo rodam
  `federate registry migrate-json-to-redb` uma vez (veja
  [migrations.md](migrations.md)).

## Comportamento de restart

`Restart=on-failure` + `RestartSec=3` em toda unit. O servidor NUNCA cria
TLDs a partir de código: inicialize e faça o seed do registry
explicitamente antes do primeiro start (`federate root init` + `federate
root seed --file seeds/official-tlds.toml --data-dir
/var/lib/federate/data`), e então todo boot carrega `data/registry/` como
fonte da verdade, re-verificado contra a chave raiz (veja
[root-registry.md](root-registry.md)). Versões da zona raiz crescem
estritamente entre mutações e reinícios, então os daemons (que rejeitam
zonas mais antigas que uma já verificada) sempre aceitam a zona atual. Nós
registrados persistem em `data/directory-nodes.json` e são re-verificados
no load; nós também se re-registram a cada ~60s.

## Logs

`journalctl -u federate-server -f` (também `-u federate-dnsd`,
`-u federate-gatewayd`). Verbosidade: `Environment=RUST_LOG=debug` na unit.

## Deploy em VPS compartilhada (Docker + reverse proxy existente)

Foi assim que o PRIMEIRO deploy real do Node 1 foi executado (2026-07-07,
VPS Hetzner em 195.201.171.223): uma máquina compartilhada onde as portas
80/443 pertencem a um Traefik existente, o ufw está ativo, não havia
root/sudo, e uma dúzia de outros serviços precisava continuar funcionando.
Tudo roda como containers Docker sob um usuário normal no grupo `docker`;
portas publicadas pelo Docker passam por fora do ufw, então nenhuma regra
de firewall foi necessária para 53/4077.

As peças vivem em `deploy/docker/`: `Dockerfile` (os quatro binários +
seeds + blocklists), `docker-compose.yml`, `traefik-federate-catchall.yml`,
`entrypoint.sh`.

Layout na máquina: `~/federate/src` (fonte),
`~/federate/data/{node1,gatewayd,dnsd}` (estado; chaves e registry.redb
ficam em node1), `~/federate/backups`.

```sh
# 1. build da imagem (na máquina)
cd ~/federate/src
docker build -f deploy/docker/Dockerfile -t federate:latest .

# 2. bootstrap explícito do registry (containers one-shot, servidor parado)
docker run --rm --user 1001:1001 -e HOME=/tmp \
  -v $HOME/federate/data/node1:/var/lib/federate/data \
  federate:latest federate root init --data-dir /var/lib/federate/data
docker run --rm --user 1001:1001 -e HOME=/tmp \
  -v $HOME/federate/data/node1:/var/lib/federate/data \
  federate:latest federate root seed \
  --file /var/lib/federate/seeds/official-tlds.toml --data-dir /var/lib/federate/data

# 3. configurar e subir o stack
cp ~/federate/src/deploy/docker/docker-compose.yml ~/federate/
cat > ~/federate/.env <<ENV
PUBLIC_IP=195.201.171.223
REGION=de-fsn
FEDERATE_UID=1001
FEDERATE_GID=1001
FEDERATE_DATA=/home/c3b/federate/data
DNS_UPSTREAM=1.1.1.1:53
ROOT_KEY=<hex impresso pelo root init>
ENV
chmod 600 ~/federate/.env
cd ~/federate && docker compose up -d

# 4. publicar o site demo pela API de ingestão
docker run --rm --user 1001:1001 -e HOME=/tmp --network federate_federate \
  -v $HOME/federate/cli:/keys federate:latest \
  federate publish package /var/lib/federate/sites/home-fed \
  --domain home.fed --key-dir /keys/owner --bootstrap http://federate-server:9600

# 5. porta do navegador: catch-all de prioridade mínima no Traefik existente
docker run --rm -v /opt/traefik/dynamic:/dyn \
  -v $HOME/federate/src/deploy/docker:/srcd federate:latest \
  cp /srcd/traefik-federate-catchall.yml /dyn/90-federate-catchall.yml
```

Mapa de portas: HTTP do Node 1 em 127.0.0.1:9600 (só loopback), protocolo
nativo 0.0.0.0:4077 (público), gateway 127.0.0.1:8095 (o catch-all do
Traefik roteia todo Host não reivindicado para lá, prioridade 1), DNS em
PUBLIC_IP:53 udp+tcp (bind no IP público específico deixa o
systemd-resolved em 127.0.0.53 intocado), health endpoints em
PUBLIC_IP:8081/8053.

Uma lacuna real que esse deploy revelou: o Docker exclui tráfego da mesma
bridge do DNAT de portas publicadas, então as sondas de health do Node 1
para os endpoints públicos dos nós irmãos (o único host que o guarda SSRF
do registro aceita) davam timeout e os nós decaíam para offline, o que
esvazia as respostas DNS. Correção em `entrypoint.sh` + compose: o
container do servidor ganha NET_ADMIN, instala duas regras DNAT
redirecionando exatamente esses destinos de sonda para os IPs estáticos
dos containers irmãos (172.30.77.11/12) e então derruba privilégios para
RUN_AS. Nada muda no host.

Backups: `~/federate/backup.sh` (instalado no crontab do usuário, diário
às 04:20) roda `federate registry backup` em `~/federate/backups/` mais um
tarball de chaves+conteúdo, mantendo os últimos 14 de cada. Chaves privadas
são arquivos 0600 no volume de dados; nunca ficam na imagem nem no banco.

Verificação executada de uma máquina externa:

```sh
dig @195.201.171.223 home.fed          # -> 195.201.171.223 (veja ressalva)
dig @195.201.171.223 google.com        # -> resposta encaminhada ao upstream
curl -H "Host: home.fed" http://195.201.171.223/   # -> a página home.fed via Traefik
federate node ping --addr 195.201.171.223:4077     # -> handshake nativo, v1, root-authority
federate fetch fed://home.fed/ --provider 195.201.171.223:4077 \
  --root-key <hex da chave raiz>       # -> cadeia inteira verificada, 2557 bytes
```

Ressalva encontrada na verificação: algumas redes de acesso interceptam
TODO o tráfego da porta 53 (o sinal: respostas trazem a flag `ad` e EDNS,
que este servidor DNS não emite) e respondem NXDOMAIN para nomes Federate a
partir das raízes públicas enquanto google.com continua resolvendo. Nessas
redes teste o DNS de outro ponto de vista; o protocolo nativo (4077) e a
porta HTTP (80) não são afetados.

Onboarding de celular/desktop: configure o DNS do aparelho para
195.201.171.223 e abra `http://home.fed`. Redes com interceptação de DNS
precisam da rota via arquivo hosts ([hosts-setup.md](hosts-setup.md)).

## Escalando depois

Qualquer pessoa pode adicionar capacidade sem tocar no Node 1: mais nós
gateway (`federate-gatewayd` em outros VPSes, registrados via
`--public-ip`), mais nós DNS, storage/CDN/busca via `federate-noded`; veja
[nodes.md](nodes.md). O diretório faz health check deles e o DNS passa a
responder com todo gateway saudável automaticamente.
