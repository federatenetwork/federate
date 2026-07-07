*Uma web humana, feita por pessoas.*

**Esta é a porta de entrada. Três minutos e você está dentro.**

Nomes Federate como `home.fed` não existem na lista telefônica da internet
antiga. Uma configuração resolve isso, e a web normal continua funcionando.

## 1. Ligue o DNS da Federate (uma vez)

Escolha UM caminho:

**iPhone ou Mac (recomendado, vale para o aparelho inteiro)**

1. [Baixe o perfil de DNS](/federate-dns.mobileconfig)
2. Abra o arquivo baixado
3. Ajustes → Geral → Gerenciamento de Dispositivo → **Instalar**

**Só o navegador (30 segundos)**

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

- **Torne links fed:// clicáveis** - depois de compilar a CLI (abaixo),
  rode `federate handler install` uma vez e endereços como
  `fed://home.fed` passam a abrir no navegador.
- **Use a linha de comando** - o jeito nativo de navegar:

```
git clone https://github.com/c3b/federatenetwork
cargo build --release -p federate-cli
federate fetch fed://home.fed/
```

- **Rode um nó** - sirva DNS, gateway ou conteúdo para a rede. Comece por
  `docs/pt-BR/nodes.md` no repositório.

## O que é isto, em uma frase

Um espaço de nomes próprio (`.fed`, `.pagina`, `.rosa`, `.mosca` e mais 19),
onde todo nome e todo byte chegam assinados de ponta a ponta, operado por
gente, sem anúncios, sem vigilância, sem IA raspando o que é seu.
