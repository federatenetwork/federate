# Configuração no Desktop: Como um Amigo Entra na Rede

> [English version](../desktop-setup.md)

Objetivo: digitar `http://home.fed` no Chrome/Safari/Firefox/Edge e entrar na
Federate Network.

## 1. Instalar ou compilar o `federated`

```sh
git clone <this repo> && cd federatenetwork
cargo build --release
```

Binários: `target/release/federated` (daemon) e `target/release/federate` (CLI).

## 2. Configurar o servidor de bootstrap

O padrão já é `https://federate.network` (Node 1). Para sobrescrever:

```sh
federated --bootstrap https://federate.network
```

## 3. Adicionar os mapeamentos no arquivo hosts

Siga [hosts-setup.md](hosts-setup.md); acrescente `deploy/hosts-federate.txt`
ao seu arquivo hosts.

## 4. Permitir que o daemon vincule a porta 80

Siga [port-80-setup.md](port-80-setup.md) para o seu sistema operacional
(Linux: `setcap` ou systemd; macOS: `sudo` ou launchd; Windows: executar como Administrador).

## 5. Executar o daemon

```sh
federated
```

Você deve ver: identidade carregada, zona raiz obtida do Node 1, gateway em
`http://127.0.0.1:80`, API em `127.0.0.1:7777`.

## 6. Abrir a Federate Network

Abra **http://home.fed** em qualquer navegador comum, sem porta. Mais sites vão
aparecer na rede conforme a publicação for aberta.

## Verificação

```sh
federate doctor     # diagnóstico completo com correções
federate status     # status do daemon
federate open home.fed
```

Os sites visitados são armazenados em cache localmente e continuam funcionando mesmo quando o Node 1 está
temporariamente offline.
