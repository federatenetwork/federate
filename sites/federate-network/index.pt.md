*Uma web humana, feita por pessoas.*

**Esta é a porta de entrada. Três minutos e você está dentro.**

Nomes Federate como `home.fed` não existem na lista telefônica da internet
antiga. Uma configuração resolve isso, e a web normal continua funcionando.

## 1. Um comando e você está dentro (Mac, Linux, Windows)

**Mac ou Linux** - cole no terminal:

```
curl -fsSL https://federate.network/install.sh | bash
```

**Windows** - cole no PowerShell:

```
iex (irm https://federate.network/install.ps1)
```

Isso instala a CLI `federate`, sobe um resolvedor local verificador
(todo TLD da Federate, atual e futuro, respondido a partir da zona raiz
assinada), aponta o DNS do sistema para ele, torna links `fed://`
clicáveis e roda um teste ao vivo. Desfazer: `sudo federate dns uninstall`.

**iPhone ou iPad?**

1. [Baixe o perfil de DNS](/federate-dns.mobileconfig)
2. Abra o arquivo baixado
3. Ajustes → Geral → Gerenciamento de Dispositivo → **Instalar**

**Só o navegador, sem instalar nada (30 segundos)**

Chrome, Edge ou Firefox → Configurações → **DNS seguro** → provedor
personalizado → cole:

`https://federate.network/dns-query`

Funciona em qualquer rede: casa, trabalho, 4G. Nenhum roteador ou provedor
consegue bloquear.

## 2. Abra sua primeira página Federate

Vá para [http://home.fed](http://home.fed)

Se abriu: você está na rede. Essa página chegou assinada pela chave raiz e
verificada por hash, bloco por bloco. Se não abriu, o passo 1 ainda não
foi aplicado (feche e abra o navegador).

## 3. Vá mais fundo

- **Leia o manifesto** - [http://home.fed](http://home.fed) explica por que
  isto existe: sem feeds, sem raspagem, sem treinamento de IA.
- **Publique seu site** - empacote uma pasta com `index.html` e publique
  sob um nome seu:

```
federate publish package ./meu-site --domain voce.pagina
```

- **Use a linha de comando** - o instalador do passo 1 já te deu o jeito
  nativo de navegar:

```
federate fetch fed://home.fed/
```

- **Rode um nó** - sirva DNS, gateway ou conteúdo para a rede. Comece por
  `docs/pt-BR/nodes.md` no repositório.

## O que é isto, em uma frase

Um espaço de nomes próprio (`.fed`, `.pagina`, `.rosa`, `.mosca` e mais 19),
onde todo nome e todo byte chegam assinados de ponta a ponta, operado por
gente, sem anúncios, sem vigilância, sem IA raspando o que é seu.
