# Páginas em Markdown (`fed-md.js`)

> [English version](../markdown-pages.md)

As páginas oficiais da Federate são escritas como arquivos markdown simples
e renderizadas no navegador pelo `fed-md.js`: um renderizador sem
dependências e seguro contra XSS que qualquer site Federate pode copiar e
usar. Manutenção = editar um arquivo `.md`, reiniciar o Node 1 (ou
republicar), pronto.

## Uso direto (drop-in)

```html
<link rel="stylesheet" href="/style.css">
<script src="/fed-md.js" defer></script>

<article class="md" data-md-src="/index.md"></article>
```

Todo elemento com `data-md-src` é buscado e renderizado no carregamento da
página. Adicione `data-md-title` para definir o `document.title` a partir do
primeiro `# h1` da página.

## Uso programático

```js
fedMD.render("# Hello **world**")   // -> string html
fedMD.mount(element, "/page.md")    // busca + renderiza dentro do elemento (Promise)
```

## Markdown suportado

Títulos de `#` a `######`, parágrafos, `**negrito**`, `*itálico*`,
`~~tachado~~`, `` `código` ``, blocos de código cercados com ```,
`[links](url)`, `![imagens](src)`, citações com `>`, listas `-`/`1.`
(aninhadas com 2 espaços), réguas `---`, `| tabelas |`, quebras de linha com
dois espaços no final e escapes com `\`.

## Segurança

- HTML bruto dentro do markdown é **escapado**, nunca injetado.
- URLs de links/imagens passam por uma lista de permissões (`http(s):`,
  `mailto:`, relativas, `#`); `javascript:`/`data:` viram `#`.
- O conteúdo continua fluindo pela cadeia normal da Federate: manifesto
  assinado, blocos verificados por hash; o `fed-md.js` renderiza apenas o
  que o manifesto atesta.

## Páginas atuais

| Página | Fonte |
|---|---|
| `home.fed/` | `sites/home-fed/index.en.md` + `index.pt.md` (seletor de idioma) |
| `home.fed/manifesto.html` | `sites/home-fed/manifesto.pt.md` + `manifesto.en.md` (alternância de idioma) |

Sem JavaScript, as páginas exibem um aviso `<noscript>` com link para o
`.md` bruto, que é legível por humanos por concepção.
