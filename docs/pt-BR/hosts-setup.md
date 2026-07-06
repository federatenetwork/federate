# Configuração do arquivo hosts (MVP)

> [English version](../hosts-setup.md)

Até o resolvedor DNS local ficar pronto (fase 3), mapeie os domínios Federate
para o daemon local manualmente.

## Onde

- Linux / macOS: `/etc/hosts` (edite com `sudo`)
- Windows: `C:\Windows\System32\drivers\etc\hosts` (edite como Administrador)

## Adicione estas linhas

```txt
127.0.0.1 home.fed
```

Uma cópia pronta para anexar está em [`deploy/hosts-federate.txt`](../../deploy/hosts-federate.txt).

## Verifique

```sh
ping -c1 home.fed        # deve responder em 127.0.0.1
federate doctor          # verifica o arquivo hosts, entre outras coisas
```

Depois, com o `federated` rodando na porta 80, abra **http://home.fed**, sem porta.
