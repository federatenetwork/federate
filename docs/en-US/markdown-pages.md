# Markdown pages (`fed-md.js`)

> [Versão em português (pt-BR)](../pt-BR/markdown-pages.md)

Official Federate pages are written as plain markdown files and rendered in
the browser by `fed-md.js`: a zero-dependency, XSS-safe renderer that any
Federate site can copy and use. Maintenance = edit a `.md` file, restart
Node 1 (or republish), done.

## Drop-in usage

```html
<link rel="stylesheet" href="/style.css">
<script src="/fed-md.js" defer></script>

<article class="md" data-md-src="/index.md"></article>
```

Every element with `data-md-src` is fetched and rendered on page load.
Add `data-md-title` to set `document.title` from the page's first `# h1`.

## Programmatic usage

```js
fedMD.render("# Hello **world**")   // -> html string
fedMD.mount(element, "/page.md")    // fetch + render into element (Promise)
```

## Supported markdown

Headings `#` to `######`, paragraphs, `**bold**`, `*italic*`, `~~strike~~`,
`` `code` ``, ``` fenced code blocks, `[links](url)`, `![images](src)`,
`>` blockquotes, `-`/`1.` lists (nest with 2 spaces), `---` rules,
`| tables |`, two-trailing-space line breaks, and `\` escapes.

## Security

- Raw HTML in markdown is **escaped**, never injected.
- Link/image URLs are allowlisted (`http(s):`, `mailto:`, relative, `#`);
  `javascript:`/`data:` become `#`.
- Content still flows through the normal Federate chain: signed manifest,
  hash-verified blocks; `fed-md.js` renders only what the manifest vouches for.

## Current pages

| Page | Source |
|---|---|
| `home.fed/` | `sites/home-fed/index.en.md` + `index.pt.md` (language dropdown) |
| `home.fed/manifesto.html` | `sites/home-fed/manifesto.pt.md` + `manifesto.en.md` (language toggle) |

Without JavaScript, pages show a `<noscript>` note linking to the raw `.md`,
which is human-readable by design.
