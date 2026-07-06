# Espaços de nomes

Todo nome Federate é um rótulo mais um TLD, como `home.fed` ou `voce.pagina`.
Os TLDs vivem no registro raiz assinado: nenhum servidor, nó ou gateway pode
inventar um.

| TLD | Para quê |
|---|---|
| `.fed` | Espaço oficial da Federate: especificações, registro, status, governança |
| `.pagina` | Sites pessoais, blogs, portfólios, ensaios |
| `.rosa` | Espaços criativos, visuais, poéticos, artísticos |
| `.cara` | Identidade, perfis, páginas de pessoas, cartões públicos |
| `.mosca` | Internet esquisita: experimentos, memes, joguinhos, páginas underground |
| `.busca` | Busca e descoberta da Federate (`fed.busca`): sem anúncios, sem rastreamento |
| `.types` | Tipografia, type design, lettering, fontes |

Todos são oficiais e operados pela raiz por enquanto. TLDs delegados, operados
pela comunidade, chegam nas próximas fases.

## Como um nome vira página

```
domínio → zona raiz → TLD → registro do domínio → manifest → blocos → seu navegador
```

Cada seta é uma verificação; se qualquer uma falhar, nada é servido:

1. **Zona raiz** - o mapa assinado de todos os TLDs. Sua máquina verifica a
   assinatura contra a chave raiz fixada; um servidor adulterado é rejeitado.
2. **Registro do TLD** - assinado pela chave raiz; diz quem opera o TLD.
3. **Registro do domínio** - assinado pela chave do operador do TLD; diz quem
   é dono do nome e aponta para o manifest do site.
4. **Manifest** - assinado pela chave do dono do domínio; lista cada arquivo
   do site e o hash do conteúdo de cada um.
5. **Blocos de conteúdo** - cada arquivo é conferido byte a byte contra seu
   hash antes de chegar ao navegador.

Nenhum servidor é confiado às cegas: nós distribuem dados assinados, as
assinaturas decidem o que é válido.

## Verifique um nome

```
federate tld check seunome
federate domain check voce.pagina
```

Registro de domínios e aplicação para novos TLDs abrem nas próximas fases;
sem pagamentos por enquanto.

[← voltar para home.fed](/)
