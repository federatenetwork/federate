# Implantando o Node 1 na Hetzner

> [English version](../deployment-hetzner.md)

Node 1 = `federate-server` atrás do Caddy em `https://federate.network`.

## 1. Compilar os binários de release

No servidor (ou faça cross-compile localmente para `x86_64-unknown-linux-gnu`):

```sh
cargo build --release -p federate-server
```

## 2. Copiar para o servidor

```sh
scp target/release/federate-server root@<hetzner-ip>:/usr/local/bin/
rsync -a sites/ root@<hetzner-ip>:/var/lib/federate/sites/
scp blocked_tlds.txt root@<hetzner-ip>:/var/lib/federate/blocked_tlds.txt
rsync -a data/blocked/ root@<hetzner-ip>:/var/lib/federate/blocked/
```

O `blocked_tlds.txt` (a lista de bloqueio de colisões da IANA) é **obrigatório**: o servidor
se recusa a iniciar sem ele, de modo que nenhum restart jamais roda sem a
proteção contra colisões com o DNS público.

## 3. Criar um usuário e o serviço systemd

```sh
ssh root@<hetzner-ip>
useradd -r -m -d /var/lib/federate federate
chown -R federate:federate /var/lib/federate
cp deploy/systemd/federate-server.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now federate-server
systemctl status federate-server
```

A unit executa `federate-server --listen 127.0.0.1:9000 --sites-dir /var/lib/federate/sites`.

## 4. Apontar o DNS para o servidor

No seu registrador de domínio, adicione para `federate.network`:

```
A     federate.network   <hetzner-ipv4>
AAAA  federate.network   <hetzner-ipv6>   (opcional)
```

## 5. Reverse proxy + Let's Encrypt com Caddy

`federate.network` é um domínio público real, então o Let's Encrypt normal funciona.
O Caddy cuida dos certificados automaticamente:

```sh
apt install caddy
cp deploy/caddy/Caddyfile /etc/caddy/Caddyfile
systemctl reload caddy
```

(Nginx + certbot funciona igualmente bem; o Caddy é a opção sem configuração.)

## 6. Verificar

```sh
curl https://federate.network/health          # -> ok
curl https://federate.network/v1/status
curl https://federate.network/v1/root | head
```

## 7. Apontar os daemons locais para o Node 1

Os amigos executam o `federated` com o bootstrap padrão, que já é
`https://federate.network`. Pronto; veja [desktop-setup.md](desktop-setup.md).

## Atualizando os sites

Refaça o rsync de `sites/` e execute `systemctl restart federate-server`; ele reconstrói a
zona raiz, os manifests e os blocos na inicialização.

## 8. Executando um nó DNS (porta 53)

Um nó DNS responde os TLDs Federate com os IPs de nós gateway saudáveis e
encaminha todo o resto para o upstream. DNS em produção precisa de UDP **e** TCP 53.

```sh
scp target/release/federate-dnsd root@<ip>:/usr/local/bin/
# Vincular a porta baixa 53 sem root:
setcap 'cap_net_bind_service=+ep' /usr/local/bin/federate-dnsd
federate-dnsd \
  --listen 0.0.0.0:53 \
  --bootstrap https://federate.network \
  --root-key <federate-root-public-key-hex> \
  --upstream 1.1.1.1:53 \
  --public-ip <this-node-ipv4> --region <region>
```

- `--root-key` **deve** ser a chave pública real do Federate Root. Sem ela o
  nó faz trust-on-first-use e fixa o que quer que a primeira zona anuncie; aceitável para uma
  demo, inseguro para produção.
- `--public-ip` deve ser um IP real *desta* máquina: o diretório rejeita um
  registro cujo host do `health_endpoint` não seja um dos IPs declarados do nó
  (anti-SSRF), então uma divergência significa que o nó nunca aparece como saudável.
- O encaminhamento para o upstream conecta o socket ao resolvedor e confere o
  ID de transação DNS, de modo que respostas forjadas fora do caminho são rejeitadas.

## 9. Executando um nó gateway / storage / CDN

```sh
federate-gatewayd --listen 0.0.0.0:80 --bootstrap https://federate.network \
  --root-key <hex> --public-ip <ip> --region <region>
# ou um nó multi-função a partir de um arquivo de configuração:
federate-noded --config /etc/federate/federate.toml
```

O `federate-noded` recusa o papel `root-authority`, e o diretório rejeita
esse papel vindo de qualquer chave que não seja a Federate Root Key fixada, então nenhum nó pode forjar
autoridade de raiz.

## 10. Firewall (exemplo com ufw)

```sh
ufw allow 22/tcp                 # ssh
ufw allow 80,443/tcp             # gateway + HTTPS do Caddy
ufw allow 53/udp                 # DNS (somente nós DNS)
ufw allow 53/tcp                 # fallback DNS via TCP (somente nós DNS)
# O Node 1 escuta em 127.0.0.1:9000 (atrás do Caddy); NÃO o exponha.
ufw enable
```

## Armazenamento de chaves e backups

As chaves privadas ficam sob o `--data-dir` do servidor (padrão `.federate-server`,
ou `/var/lib/federate/data` na unit): `root/`, `official-operator/`,
`dev-owner/`, chaves de operador por TLD. Elas são gravadas com `0600` e **nunca**
são servidas por nenhuma API.

- Faça backup offline de `root/identity.key`. Perdê-la significa que você nunca mais poderá assinar uma nova
  zona raiz; vazá-la compromete o namespace inteiro.
- A unit `.service` roda como o usuário sem privilégios `federate` com
  `ProtectSystem=strict`, `ReadWritePaths=/var/lib/federate`, `PrivateTmp`
  e `UMask=0077`.
- Verifique as permissões após a primeira inicialização:
  `find /var/lib/federate/data -name identity.key -exec ls -l {} +`
  - toda chave deve estar como `-rw-------` e pertencer a `federate`.
- Backup sugerido (chaves + snapshot do diretório de nós, NÃO o cache de blocos):
  `tar czf federate-backup.tgz -C /var/lib/federate data` armazenado fora da máquina.
  A zona raiz, os manifests e os blocos são reconstruídos a partir de `sites/` na inicialização.

## Comportamento de restart

`Restart=on-failure` + `RestartSec=3` em todas as units. O servidor reconstrói e
reassina a zona raiz na inicialização; `root_version` é derivado do relógio,
então os daemons (que rejeitam zonas mais antigas do que uma já verificada) sempre
aceitam a nova zona após um restart. Os nós registrados são persistidos em
`data/directory-nodes.json` e reverificados ao carregar; os nós também se registram novamente
a cada ~60 s.

## Logs

`journalctl -u federate-server -f` (e `-u federate-dnsd`, `-u federated`).
Ajuste a verbosidade com `Environment=RUST_LOG=debug` na unit.
