# Federate Network

> [English version](README.md)

Um protocolo/runtime web alternativo e humano que roda sobre a internet existente.

Navegadores normais. Sem portas nas URLs. Abra `http://home.fed`.

```
domain → local Federate resolver/daemon → Federate root zone → domain record
       → signed manifest → content hashes → content blocks → browser response
```

## Componentes

| Binário | Papel |
|---|---|
| `federate-server` | Node 1 - servidor público de bootstrap/plano de controle (zona raiz, manifestos, blocos) |
| `federated` | Daemon desktop local - gateway do navegador em `127.0.0.1:80`, API local na `:7777` |
| `federate` | CLI - status, doctor, resolve, cache, ferramentas de nó/dns/gateway/diretório |
| `federate-dnsd` | Nó DNS (UDP+TCP 53) - responde TLDs Federate com IPs de gateways saudáveis |
| `federate-gatewayd` | Nó gateway público - serve sites verificados para navegadores |
| `federate-noded` | Nó multi-papel - gateway/dns/storage/cdn/busca/espelho-raiz em uma config |
| `federate-searchd` | Nó de busca - sem anúncios, sem rastreamento, sem treinamento de IA |

## Início rápido (dev local)

```sh
cargo build --release
./target/release/federate-server --listen 127.0.0.1:9000 &          # Node 1 (dev)
sudo ./target/release/federated --bootstrap http://127.0.0.1:9000    # daemon na porta 80
```

Adicione os mapeamentos no arquivo hosts ([hosts-setup.md](docs/pt-BR/hosts-setup.md)) e abra **http://home.fed**.

## Documentação

- [vision.md](docs/pt-BR/vision.md) - a Federate como internet alternativa em overlay
- [federate-uri.md](docs/pt-BR/federate-uri.md) - o formato nativo `fed://`
- [native-protocol.md](docs/pt-BR/native-protocol.md) - o protocolo nativo e o transporte
- [architecture.md](docs/pt-BR/architecture.md) - crates, camadas, motor de resolução
- [protocol.md](docs/pt-BR/protocol.md) - zona raiz, manifestos, endereçamento por conteúdo
- [manifesto.md](docs/pt-BR/manifesto.md) - por que o Federate existe
- [dns-resolver.md](docs/pt-BR/dns-resolver.md) - resolvedor DNS local planejado
- [deployment-vps.md](docs/pt-BR/deployment-vps.md) - implantação do Node 1
- [desktop-setup.md](docs/pt-BR/desktop-setup.md) - onboarding de amigos
- [hosts-setup.md](docs/pt-BR/hosts-setup.md) - mapeamentos do arquivo hosts
- [port-80-setup.md](docs/pt-BR/port-80-setup.md) - URLs sem porta
- [https-local.md](docs/pt-BR/https-local.md) - HTTPS interno / planos de CA local
- [tld-hierarchy.md](docs/pt-BR/tld-hierarchy.md) - registro raiz, operadores de TLD, delegação
- [signatures.md](docs/pt-BR/signatures.md) - cadeia de confiança, assinatura canônica
- [blocked-tlds.md](docs/pt-BR/blocked-tlds.md) - listas de bloqueio IANA/reservados/política
- [tld-marketplace-roadmap.md](docs/pt-BR/tld-marketplace-roadmap.md) - fases futuras do marketplace
- [troubleshooting.md](docs/pt-BR/troubleshooting.md)

## TLDs

- Núcleo: `.fed` `.busca`
- Pessoas: `.pagina` `.pages` `.cara` `.comu` `.oi` `.weblog`
- Criativos: `.rosa` `.mosca` `.tipos` `.types`
- Mídia: `.foto` `.pic` `.vid` `.sound` `.records`
- Cores: `.amarelo` `.azul` `.verde` `.preto` `.branco` `.blau`

## Roadmap

1. **Fase 1 (este repositório)**: Node 1, daemon local, configuração do arquivo hosts, raiz interna, cinco TLDs, sites estáticos, acesso por navegador normal.
2. Publicação: `federate deploy ./dist --domain example.pagina`
3. Resolvedor DNS local de verdade, integração automática com o SO, sem edições manuais do hosts.
4. Nós de amigos, descoberta de pares, conteúdo hospedado por usuários.
5. Replicação, pinning, cache/CDN distribuído, seleção do nó mais próximo.
6. UI de registro, propriedade de domínios, solicitações de TLD, governança.
7. Instalador desktop, CA raiz local do Federate, HTTPS para domínios internos.
8. Clientes móveis.

## Licença

[MIT](LICENSE)
