# Configuração da porta 80: URLs sem porta

> [English version](../port-80-setup.md)

Toda a experiência do usuário é `http://home.fed`, **sem porta**. Os navegadores
usam a porta 80 para `http://` simples, então o `federated` precisa escutar em
`127.0.0.1:80`. Escutar em portas abaixo de 1024 exige privilégios na maioria
dos sistemas.

## Linux

Preferido: conceda a capability uma única vez:

```sh
cargo build --release
sudo setcap 'cap_net_bind_service=+ep' ./target/release/federated
./target/release/federated
```

Ou instale o serviço de usuário do systemd (`deploy/systemd/federated.service`),
que usa `AmbientCapabilities=CAP_NET_BIND_SERVICE`.

## macOS

Preferido: redirecionamento de porta com pf. O `federated` roda **sem privilégios**
na 8787 e o kernel encaminha a porta 80 do loopback para ele. URLs sem porta
funcionam, sem processo root, sem arquivos pertencentes ao root:

```sh
echo "rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 80 -> 127.0.0.1 port 8787" | sudo pfctl -ef -
./target/release/federated --gateway-addr 127.0.0.1:8787
```

A regra do pf é apagada ao reiniciar. Para persistir, adicione-a a `/etc/pf.conf`
(logo após a linha `rdr-anchor "com.apple/*"`; o pf exige que regras de tradução
fiquem nessa seção) e recarregue com `sudo pfctl -f /etc/pf.conf`.

Alternativa: instale o daemon do launchd
(`deploy/launchd/network.federate.federated.plist`), que roda como root na
inicialização e escuta diretamente na porta 80:

```sh
sudo cp deploy/launchd/network.federate.federated.plist /Library/LaunchDaemons/
sudo launchctl load /Library/LaunchDaemons/network.federate.federated.plist
```

**Nunca rode o `federated` com `sudo` puro em um terminal.** No macOS, o sudo pode
preservar o `$HOME`, fazendo o daemon gravar arquivos pertencentes ao root em
`~/Library/Application Support/federate`; depois disso, rodá-lo como seu usuário
normal falha com `Error: Io(PermissionDenied)` antes da primeira linha de log.
Repare com:

```sh
sudo chown -R "$(whoami)":staff ~/Library/Application\ Support/federate
```

## Windows

MVP: rode o terminal **como Administrador** e então execute `federated.exe`.
Um serviço Windows de verdade está planejado (veja `deploy/windows-service/`).

## Se o bind falhar

O `federated` imprime uma explicação clara com correções por SO. Verifique com:

```sh
federate port-check
```

## Alternativa para desenvolvimento

`federated --gateway-addr 127.0.0.1:8787` funciona para desenvolvimento, mas não
é o fluxo documentado para o usuário; o fluxo principal é `http://home.fed` sem porta.
