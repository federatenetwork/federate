# Resolvedor DNS Local Planejado

> [English version](../en-US/dns-resolver.md)

Não implementado no MVP. O MVP usa mapeamentos no arquivo hosts. Este documento
mais o crate `federate-dns` definem a fronteira para que o DNS possa ser adicionado sem
tocar no gateway.

## O que ele fará

- Escutar em localhost (por exemplo, `127.0.0.1:53`) ou em um endereço de resolvedor configurado pelo sistema operacional.
- Responder os TLDs Federate (`.fed`, `.pagina`, `.rosa`, `.cara`, `.mosca`, `.busca`, `.types`):
  - modo gateway: retornar `127.0.0.1` para que o gateway local `federated` sirva o site
  - modos futuros: retornar IPs de gateways remotos ou IPs de serviços locais
- **Encaminhar todas as outras consultas** para o resolvedor upstream normal do usuário;
  o DNS normal da internet nunca deve quebrar.
- Usar o mesmo motor de resolução (`federate-resolution`) do gateway HTTP
  para qualquer coisa além de nome→IP (por exemplo, verificar se um domínio existe).
- Ser instalado automaticamente pelo futuro instalador para desktop (substituindo as edições manuais
  do arquivo hosts; fase 3 do roadmap).

## Planos de integração com sistemas operacionais

- **macOS**: `/etc/resolver/fed` (e um por TLD) apontando para o resolvedor local, oferecendo resolução por TLD sem tocar no DNS global.
- **Linux**: domínios de roteamento do systemd-resolved (`~fed`, `~pagina`, …) via `resolvectl`, ou uma entrada NSS/dnsmasq.
- **Windows**: regras NRPT (Name Resolution Policy Table) por TLD.

## Por que o DNS sozinho não é o runtime do Federate

O DNS só responde **para onde um nome deve ir**. O daemon/runtime ainda cuida de:

- validação da zona raiz
- resolução de registros de domínio
- manifests
- hashes de conteúdo
- cache
- descoberta de peers / futura CDN / replicação
- identidade de nó
- publicação
- servir conteúdo para o navegador

O crate `federate-dns` atualmente fornece o trait `FederateNameResolver` e um
`StubResolver` que fixa o contrato: TLD Federate → `Some(127.0.0.1)`,
todo o resto → `None` (encaminhar ao upstream).
