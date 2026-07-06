# Espelhos da raiz

> [English version](../root-mirrors.md)

Um espelho da raiz distribui cópias assinadas da zona raiz para que a rede
não dependa de um único VPS. Espelhos **não podem criar nem modificar
TLDs**, e a criptografia torna qualquer trapaça inútil.

## Como funciona

1. O espelho busca a zona raiz no Node 1 (ou em outro espelho).
2. Ele verifica a assinatura da zona contra a Federate Root Key fixada.
   Uma zona que não pode ser verificada nunca é armazenada nem servida.
3. Ele serve a zona verificada em `GET /v1/root`, atualizando a cada minuto.

## Por que espelhos não conseguem trapacear

Todo consumidor (daemon, nó DNS, gateway, CLI) verifica a assinatura da zona
raiz contra sua própria chave raiz fixada **antes de confiar em qualquer
dado** - não importa de onde os bytes vieram. Um espelho que altere um
registro de TLD, adicione um domínio ou desbloqueie um TLD bloqueado produz
uma zona que falha na verificação e é rejeitada por todos os clientes.

Os espelhos distribuem; a Federate Root Key decide.

## Execute um

```toml
# federate.toml
[node]
roles = ["root-mirror"]
region = "eu-de"
public_ip = "x.x.x.x"

[network]
bootstrap = "https://federate.network"
root_key = "<FEDERATE_ROOT_PUBLIC_KEY_HEX>"   # obrigatório na prática: fixe a chave
```

```sh
federate-noded --config federate.toml
```

Outros nós podem então usar o espelho como sua fonte da raiz:

```sh
federate-dnsd --bootstrap http://<mirror-ip>:8080 --root-key <ROOT_KEY>
```
