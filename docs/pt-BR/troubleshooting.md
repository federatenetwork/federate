# Solução de problemas

> [English version](../en-US/troubleshooting.md)

Primeiro passo, sempre:

```sh
federate doctor
```

Ele verifica o status do daemon, a acessibilidade do Node 1, a porta 80, o
arquivo hosts, o cache da zona raiz, a resolução de domínios, a saúde do
gateway e o conteúdo em cache.

## O navegador diz que o site home.fed não pode ser acessado

1. Faltam entradas no arquivo hosts → [hosts-setup.md](hosts-setup.md). Verifique:
   `ping -c1 home.fed` deve responder em `127.0.0.1`.
2. Daemon não está rodando → execute `federated`.
3. Daemon rodando em uma porta alternativa, não na 80 → confira os logs de
   inicialização; veja [port-80-setup.md](port-80-setup.md). `federate port-check` ajuda.

## "federated could not bind to 127.0.0.1:80"

Outro processo está usando a porta 80 (`sudo lsof -i :80`) ou faltam privilégios.
Correções por SO em [port-80-setup.md](port-80-setup.md).

## Página "Domain not found in Federate Network"

O nome é um TLD Federate válido, mas não tem registro na zona raiz. Confira
os domínios registrados com `federate root show`. Se o Node 1 o adicionou
recentemente, reinicie o `federated` (a atualização da raiz sob demanda está
no roadmap).

## Página "Federate resolution error"

Node 1 inacessível e conteúdo ainda não está em cache. Verifique
`curl https://federate.network/health`, sua conexão com a internet e
`federate status` (`node1_reachable`). Sites em cache continuam funcionando
offline; os que não estão em cache precisam do Node 1.

## Erros de hash mismatch nos logs

O conteúdo baixado não bateu com o hash do manifesto (corrupção ou adulteração).
O daemon se recusa a servi-lo. Limpe o cache e tente de novo:
`federate cache clear`.

## Conteúdo desatualizado depois de publicar uma atualização

Manifestos e blocos são endereçados por conteúdo, então as atualizações chegam
via uma nova zona raiz (novo hash de manifesto). Reinicie o `federated` para
forçar uma atualização da raiz, ou use `federate cache clear` para um reset completo.

## Sites normais quebraram?

O MVP não altera nada global; apenas as linhas que você adicionou no arquivo
hosts e `127.0.0.1:80`. Remova as linhas do hosts para desfazer completamente.
