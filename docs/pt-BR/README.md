# Federate Network

> [English version](../en-US/README.md)

Um protocolo/runtime de web alternativa e humana que roda sobre a internet existente.

Navegadores normais. Sem portas nas URLs. Abra `http://home.fed`.

```
domain → local Federate resolver/daemon → Federate root zone → domain record
       → signed manifest → content hashes → content blocks → browser response
```

## Componentes

| Binário | Papel |
|---|---|
| `federate-server` | Node 1 - registro raiz, zona raiz, manifests/blocos, diretório de nós, bootstrap |
| `federated` | Daemon local de desktop - gateway do navegador em `127.0.0.1:80`, API local na `:7777` |
| `federate-dnsd` | Nó DNS do Federate - responde TLDs do Federate com IPs de gateways saudáveis (qualquer um pode rodar) |
| `federate-gatewayd` | Nó gateway público - serve sites Federate para navegadores (qualquer um pode rodar) |
| `federate-noded` | Nó multifunção - gateway/dns/storage/cdn/search/bootstrap/root-mirror |
| `federate-searchd` | Nó de busca - indexa páginas públicas, `/v1/search` |
| `federate` | CLI - comandos de status, doctor, resolve, cache, open, node/dns/gateway/directory |

## Início rápido (dev local)

```sh
cargo build --release
./target/release/federate-server --listen 127.0.0.1:9000 &          # Node 1 (dev)
sudo ./target/release/federated --bootstrap http://127.0.0.1:9000    # daemon na porta 80
```

Adicione os mapeamentos no arquivo hosts ([hosts-setup.md](hosts-setup.md)) e então abra **http://home.fed**.

## Documentação

- [vision.md](vision.md) - a Federate como internet alternativa em overlay
- [overlay-network.md](overlay-network.md) - o mapa de camadas: o que é da Federate e o que ela reaproveita
- [federate-uri.md](federate-uri.md) - o formato nativo de endereçamento `fed://`
- [native-protocol.md](native-protocol.md) - o protocolo nativo de nós/clientes e o transporte
- [browser-compatibility.md](browser-compatibility.md) - pontes DNS/HTTP para navegadores normais
- [future-federate-browser.md](future-federate-browser.md) - a fronteira do cliente nativo
- [non-html-runtime-roadmap.md](non-html-runtime-roadmap.md) - documentos, pacotes e apps além do HTML
- [decentralization.md](decentralization.md) - o que é ou não descentralizado, cadeia de confiança
- [nodes.md](nodes.md) - rodando seu próprio nó, papéis, configuração, registro
- [dns-nodes.md](dns-nodes.md) - rodando um nó DNS do Federate
- [gateway-nodes.md](gateway-nodes.md) - rodando um nó gateway
- [storage-cdn-nodes.md](storage-cdn-nodes.md) - nós de storage/CDN
- [root-mirrors.md](root-mirrors.md) - espelhando a zona raiz assinada
- [node-directory.md](node-directory.md) - registro de nós, saúde, API de descoberta
- [architecture.md](architecture.md) - crates, camadas, motor de resolução
- [protocol.md](protocol.md) - zona raiz, manifests, endereçamento por conteúdo
- [manifesto.md](manifesto.md) - por que o Federate existe
- [markdown-pages.md](markdown-pages.md) - páginas oficiais em markdown + o renderizador `fed-md.js`
- [dns-resolver.md](dns-resolver.md) - resolvedor DNS local planejado
- [deployment-vps.md](deployment-vps.md) - implantando o Node 1
- [desktop-setup.md](desktop-setup.md) - onboarding de amigos
- [hosts-setup.md](hosts-setup.md) - mapeamentos no arquivo hosts
- [port-80-setup.md](port-80-setup.md) - URLs sem porta
- [https-local.md](https-local.md) - planos de HTTPS interno / CA local
- [tld-hierarchy.md](tld-hierarchy.md) - registro raiz, operadores de TLD, delegação
- [root-registry.md](root-registry.md) - o root registry persistente e mutável em runtime
- [mutations.md](mutations.md) - mutações assinadas, desafio-resposta, log de auditoria
- [publishing.md](publishing.md) - publicando sites pela API de ingestão
- [security.md](security.md) - modelo de segurança do registry em runtime
- [signatures.md](signatures.md) - cadeia de confiança, assinatura canônica
- [blocked-tlds.md](blocked-tlds.md) - listas de bloqueio IANA/reservados/política
- [tld-marketplace-roadmap.md](tld-marketplace-roadmap.md) - fases futuras do marketplace
- [troubleshooting.md](troubleshooting.md)

## TLDs

- Núcleo: `.fed` `.busca`
- Pessoas: `.pagina` `.pages` `.cara` `.comu` `.oi` `.weblog`
- Criativos: `.rosa` `.mosca` `.tipos` `.types`
- Mídia: `.foto` `.pic` `.vid` `.sound` `.records`
- Cores: `.amarelo` `.azul` `.verde` `.preto` `.branco` `.blau`

## Roadmap

1. **Fase 1 (este repositório)**: Node 1, daemon local, configuração via arquivo hosts, raiz interna, cinco TLDs, sites estáticos, acesso por navegador normal.
2. Publicação: `federate deploy ./dist --domain example.pagina`
3. Resolvedor DNS local de verdade, integração automática com o SO, sem edições manuais do hosts.
4. Nós de amigos, descoberta de peers, conteúdo hospedado por usuários.
5. Replicação, pinning, cache/CDN distribuído, seleção do nó mais próximo.
6. UI de registro, propriedade de domínios, solicitações de TLD, governança.
7. Instalador desktop, CA raiz local do Federate, HTTPS para domínios internos.
8. Clientes móveis.
