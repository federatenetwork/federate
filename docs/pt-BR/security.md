# Modelo de segurança do registry em runtime

> [English version (en-US)](../en-US/security.md)

Esta página cobre as propriedades de segurança do root registry
persistente e mutável em runtime e das suas APIs de mutação/ingestão. A
cadeia de assinaturas em si (assinatura canônica, cadeia de confiança,
pinagem de chave) está em [signatures.md](signatures.md).

## Papéis e chaves

| Chave | Autoridade |
|---|---|
| Federate Root Key | assina a zona, registros de TLD, eventos de auditoria; delega TLDs; enforcement de emergência |
| Chave de operador de TLD | emite/atualiza/suspende domínios dentro do próprio TLD; move o ponteiro do registry delegado |
| Chave do dono do domínio | assina manifests; publica/atualiza o próprio domínio |
| Chave de identidade do nó | só identidade de transporte; nenhuma autoridade sobre o registry |

Chaves privadas nunca aparecem em registro persistido, resposta de API ou
mutação. O servidor guarda as chaves raiz e do operador oficial para
contra-assinar mutações aceitas; donos e operadores delegados assinam nas
próprias máquinas.

## Anti-replay, em camadas

1. **Desafio-resposta com nonce**: toda mutação embute um nonce de uso
   único emitido pelo servidor (TTL de 5 minutos). Reuso é rejeitado antes
   de qualquer outra coisa rodar.
2. **Janela de timestamp**: envelopes com mais de 5 minutos são
   rejeitados; timestamps ilegíveis contam como expirados (fail closed).
3. **Ids de mutação auto-certificantes**: `mutation_id` é o BLAKE3 do
   envelope; ids aceitos são persistidos em `mutations.jsonl`, então um
   replay é rejeitado mesmo depois de um reinício.
4. **Versões monotônicas por alvo**: cada mutação precisa avançar
   estritamente a versão do seu alvo; mutações capturadas e reenviadas ou
   reordenadas não conseguem reverter um domínio ou TLD.
5. **Monotonicidade da zona raiz**: cada mutação aceita re-assina a zona
   com `max(anterior + 1, agora)`; clientes continuam rejeitando zonas
   mais antigas.
6. **Versões de registry delegado**: um registry re-apontado precisa
   carregar uma versão de operador estritamente maior.

## Autorização fail-closed

A autoridade deriva do estado assinado ATUAL, nunca do request: a chave do
dono no registro de domínio existente, a chave do operador no registro de
TLD assinado pela raiz, a chave raiz na zona. Assinantes desconhecidos,
assinantes errados, operadores de outro TLD e transições de status
proibidas são todos rejeitados com erro explícito, e nada é aplicado pela
metade: o caminho de mutação trabalha numa cópia e só faz commit depois que
a nova zona se auto-verifica.

## Evidência de adulteração

- `state.json` é re-verificado contra a chave raiz pinada em todo boot; um
  arquivo adulterado para o nó.
- Registries delegados são re-verificados contra as chaves de operador na
  carga.
- Manifests e blocos são re-conferidos contra seus endereços de conteúdo na
  carga e na leitura; entradas corrompidas são descartadas, nunca servidas.
- Toda mutação aceita anexa um evento de auditoria assinado pela raiz
  carregando o BLAKE3 da zona antes e depois, então o log de auditoria
  encadeia o histórico de estado; `federate registry verify` re-confere
  tudo sob demanda.

## Limitações conhecidas (deliberadas, documentadas)

- **Autoridade raiz única**: um Node 1 guarda a chave raiz. Mirrors são
  trustless para leitura, mas mutações têm um único nó que aceita.
- **Sem rotação de chave ainda**: uma chave raiz/operador/dono vazada não
  tem caminho de rollover contra-assinado (veja trabalho futuro em
  [signatures.md](signatures.md)). Mantenha backups offline; arquivos 0600.
- **Sem rate limiting** nos endpoints de nonce/mutação/ingestão ainda; um
  deploy público deve colocá-los atrás de limite no reverse-proxy.
- **Publicação first-come** sob TLDs oficiais nesta fase; sem pagamento ou
  vínculo de identidade.
- **Nonces são em memória**: um reinício limpa desafios não usados
  (clientes só pedem outro); o histórico de mutações aceitas é durável.
