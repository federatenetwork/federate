# Espaços de nomes

Todo nome Federate é um rótulo mais um TLD, como `home.fed` ou `voce.pagina`.
Os TLDs vivem no registro raiz assinado: nenhum servidor, nó ou gateway pode
inventar um.

| TLD | Para quê |
|---|---|
| `.fed` | Espaço oficial da Federate: especificações, registro, status, governança |
| `.busca` | Busca e descoberta da Federate (`fed.busca`): sem anúncios, sem rastreamento |
| `.pagina` | Sites pessoais, blogs, portfólios, ensaios (português) |
| `.pages` | Sites pessoais, blogs, portfólios, ensaios (inglês) |
| `.cara` | Identidade, perfis, páginas de pessoas, cartões públicos |
| `.comu` | Comunidades, grupos, coletivos, clubes |
| `.oi` | Ois casuais, páginas pessoais leves, cartões de contato |
| `.weblog` | Blogs, diários, registros contínuos |
| `.rosa` | Espaços criativos, visuais, poéticos, artísticos |
| `.mosca` | Internet esquisita: experimentos, memes, joguinhos, páginas underground |
| `.tipos` | Tipografia, type design, lettering, fontes (português) |
| `.types` | Tipografia, type design, lettering, fontes (inglês) |
| `.foto` | Fotografia, ensaios fotográficos, galerias (português) |
| `.pic` | Imagens, ilustração, recortes visuais (inglês) |
| `.vid` | Páginas de vídeo, canais, salas de exibição |
| `.sound` | Áudio, música, sound art, rádio, podcasts |
| `.records` | Selos musicais, discografias, arquivos |
| `.amarelo` `.azul` `.verde` `.preto` `.branco` `.blau` | Espaços de cores: espaços criativos e pessoais temáticos |

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
