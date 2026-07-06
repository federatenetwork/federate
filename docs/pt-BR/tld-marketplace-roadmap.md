# Roadmap do Marketplace de TLDs

> [English version](../tld-marketplace-roadmap.md)

O Federate controla a raiz; os TLDs serão delegados a operadores; os
operadores vendem domínios dentro dos seus TLDs. **Nenhum pagamento está
implementado hoje.** Este documento é o plano.

## Fase 1: agora (este repositório)

TLDs oficiais estáticos (`.fed .pagina .rosa .cara .mosca .tipos .types`),
listas de bloqueio (`blocked_tlds.txt` + reservados/política/proteção de
marca), registros de domínio gerenciados pela raiz, cadeia de confiança
Ed25519 completa, exemplo semente de um TLD delegado (`.femboy`) cuja
resolução retorna `DelegatedRegistryNotImplemented`. Mutações administrativas
são apenas via dados semente (editar listas de bloqueio / sites, reiniciar o
Node 1).

## Fase 2: solicitações de TLD e aprovação administrativa

`federate tld apply <tld>` envia uma solicitação assinada (validada contra
todas as listas de bloqueio) ao Node 1; registros de TLD `pending` aparecem na
zona raiz; o administrador da raiz aprova/rejeita (`federate tld approve
--owner <pk> --operator <pk>`). As APIs de mutação usam requisições assinadas
com proteção contra replay via nonce/desafio (ver docs/signatures.md). Ainda
sem dinheiro.

## Fase 3: compra de TLD / integração de pagamento

Os metadados de preço passam a valer; as solicitações carregam pagamento;
começa a imposição de expiração/renovação (TLDs expirados param de resolver
após o período de carência). Os trilhos de pagamento ficam deliberadamente
sem especificação até aqui.

## Fase 4: dashboard do operador de TLD

Dashboard web para operadores: emitir/suspender/revogar domínios sob seu TLD,
gerenciar chaves de operador, publicar endpoints de registro, ver logs de
auditoria.

## Fase 5: venda de domínios dentro de TLDs delegados

Operadores precificam e vendem domínios (`eu.femboy`) para registrantes.
Registros de domínio assinados pela chave do operador; manifests assinados
pela chave de dono do comprador. Fluxos voltados ao registrante na CLI + no
dashboard.

## Fase 6: registros delegados externos

O resolvedor implementa `delegated_http` (API de registro hospedada pelo
operador) e `delegated_manifest` (manifest de registro assinado, distribuído
como conteúdo). O caminho de verificação já está fixado: o registro de TLD
assinado pela raiz nomeia a chave do operador; todo registro de domínio deve
ser verificado contra ela. O `DelegatedRegistryNotImplemented` desaparece.

## Fase 7: mirrors federados da raiz e delegação assinada

Múltiplos mirrors da raiz servem a mesma zona assinada pela raiz (os mirrors
são trustless porque tudo é assinado). Os metadados de bootstrap listam os
mirrors; os daemons fazem failover. A rotação da chave raiz e o multisig para
TLDs de alto valor chegam aqui.
