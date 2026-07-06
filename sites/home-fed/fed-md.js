/*!
 * fed-md.js: Federate Network markdown renderer.
 *
 * Zero dependencies, XSS-safe (raw HTML in markdown is escaped, dangerous
 * link schemes are stripped). Works anywhere:
 *
 *   1. Auto-mount:  <script src="/fed-md.js" defer></script>
 *                   <article data-md-src="/page.md"></article>
 *      Every element with [data-md-src] is fetched and rendered.
 *      Add data-md-title to also set document.title from the first # h1.
 *
 *   2. Programmatic: fedMD.render(markdown)  -> html string
 *                    fedMD.mount(el, url)    -> Promise<void>
 *
 * Supported markdown: #..###### headings, paragraphs, **bold**, *italic*,
 * ~~strike~~, `code`, ``` fenced code blocks, [links](url), ![images](src),
 * > blockquotes, -/* and 1. lists (nested by 2 spaces), --- rules,
 * | tables |, and two-space line breaks.
 */
(function (root) {
  'use strict';

  // --- helpers -------------------------------------------------------------

  function escapeHtml(s) {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  // Allow only safe link/image destinations. Everything else becomes "#".
  function safeUrl(url) {
    var u = url.trim();
    if (/^(https?:|mailto:|\/|\.\/|\.\.\/|#)/i.test(u) && !/^javascript:/i.test(u)) {
      return u;
    }
    return '#';
  }

  // --- inline markdown (bold/italic/code/links/images/strike) ---------------

  function inline(text) {
    var out = '';
    var i = 0;
    while (i < text.length) {
      var rest = text.slice(i);
      var m;

      // escaped markdown character: \* \_ \` \[ etc.
      if ((m = rest.match(/^\\([\\`*_{}[\]()#+\-.!~|])/))) {
        out += escapeHtml(m[1]);
        i += m[0].length;
        continue;
      }
      // inline code: contents fully escaped, no nested parsing
      if ((m = rest.match(/^`([^`]+)`/))) {
        out += '<code>' + escapeHtml(m[1]) + '</code>';
        i += m[0].length;
        continue;
      }
      // image
      if ((m = rest.match(/^!\[([^\]]*)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)/))) {
        out += '<img src="' + escapeHtml(safeUrl(m[2])) + '" alt="' + escapeHtml(m[1]) + '"' +
          (m[3] ? ' title="' + escapeHtml(m[3]) + '"' : '') + '>';
        i += m[0].length;
        continue;
      }
      // link: label supports nested inline markdown
      if ((m = rest.match(/^\[([^\]]+)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)/))) {
        out += '<a href="' + escapeHtml(safeUrl(m[2])) + '"' +
          (m[3] ? ' title="' + escapeHtml(m[3]) + '"' : '') + '>' + inline(m[1]) + '</a>';
        i += m[0].length;
        continue;
      }
      // bold, italic, strike
      if ((m = rest.match(/^\*\*([^*]+)\*\*/)) || (m = rest.match(/^__([^_]+)__/))) {
        out += '<strong>' + inline(m[1]) + '</strong>';
        i += m[0].length;
        continue;
      }
      if ((m = rest.match(/^\*([^*]+)\*/)) || (m = rest.match(/^_([^_]+)_/))) {
        out += '<em>' + inline(m[1]) + '</em>';
        i += m[0].length;
        continue;
      }
      if ((m = rest.match(/^~~([^~]+)~~/))) {
        out += '<del>' + inline(m[1]) + '</del>';
        i += m[0].length;
        continue;
      }
      // two trailing spaces before newline = hard break (handled per line)
      out += escapeHtml(text[i]);
      i += 1;
    }
    return out;
  }

  // --- block markdown --------------------------------------------------------

  function render(md) {
    var lines = String(md).replace(/\r\n?/g, '\n').split('\n');
    var html = [];
    var i = 0;

    function paragraphBuffer(buf) {
      if (!buf.length) return;
      var joined = buf
        .map(function (l) { return /\s\s$/.test(l) ? inline(l.replace(/\s+$/, '')) + '<br>' : inline(l.trim()); })
        .join('\n');
      html.push('<p>' + joined + '</p>');
      buf.length = 0;
    }

    // Parse a run of list items at one indentation level; recurses deeper.
    function parseList(start, indent) {
      var ordered = /^\s*\d+\.\s/.test(lines[start]);
      var out = ordered ? '<ol>' : '<ul>';
      var j = start;
      var itemRe = new RegExp('^\\s{' + indent + '}(?:([-*+])|(\\d+\\.))\\s+(.*)$');
      while (j < lines.length) {
        var m = lines[j].match(itemRe);
        if (!m) break;
        var content = m[3];
        j += 1;
        // Deeper-indented lines belong to this item (sub-list or continuation).
        var subStart = j;
        var deeper = new RegExp('^\\s{' + (indent + 2) + ',}\\S');
        while (j < lines.length && (deeper.test(lines[j]) || lines[j].trim() === '')) {
          if (lines[j].trim() === '' && !(j + 1 < lines.length && deeper.test(lines[j + 1]))) break;
          j += 1;
        }
        var sub = '';
        if (j > subStart) {
          var subLines = lines.slice(subStart, j);
          var subItem = new RegExp('^\\s{' + (indent + 2) + '}(?:[-*+]|\\d+\\.)\\s');
          if (subLines.some(function (l) { return subItem.test(l); })) {
            var nested = parseList(subStart, indent + 2);
            sub = nested.html;
          } else {
            sub = ' ' + subLines.map(function (l) { return inline(l.trim()); }).join(' ');
          }
        }
        out += '<li>' + inline(content) + sub + '</li>';
      }
      out += ordered ? '</ol>' : '</ul>';
      return { html: out, next: j };
    }

    var para = [];
    while (i < lines.length) {
      var line = lines[i];

      // blank line: flush paragraph
      if (line.trim() === '') { paragraphBuffer(para); i += 1; continue; }

      // fenced code block
      var fence = line.match(/^```(\S*)\s*$/);
      if (fence) {
        paragraphBuffer(para);
        var code = [];
        i += 1;
        while (i < lines.length && !/^```\s*$/.test(lines[i])) { code.push(lines[i]); i += 1; }
        i += 1; // closing fence
        html.push('<pre><code' + (fence[1] ? ' class="language-' + escapeHtml(fence[1]) + '"' : '') + '>' +
          escapeHtml(code.join('\n')) + '</code></pre>');
        continue;
      }

      // heading
      var h = line.match(/^(#{1,6})\s+(.*?)\s*#*\s*$/);
      if (h) {
        paragraphBuffer(para);
        var level = h[1].length;
        html.push('<h' + level + '>' + inline(h[2]) + '</h' + level + '>');
        i += 1;
        continue;
      }

      // horizontal rule
      if (/^\s*((-\s*){3,}|(\*\s*){3,}|(_\s*){3,})$/.test(line)) {
        paragraphBuffer(para);
        html.push('<hr>');
        i += 1;
        continue;
      }

      // blockquote (consumes consecutive > lines, rendered recursively)
      if (/^\s*>/.test(line)) {
        paragraphBuffer(para);
        var quote = [];
        while (i < lines.length && /^\s*>/.test(lines[i])) {
          quote.push(lines[i].replace(/^\s*>\s?/, ''));
          i += 1;
        }
        html.push('<blockquote>' + render(quote.join('\n')) + '</blockquote>');
        continue;
      }

      // table: header row + |---| separator
      if (/\|/.test(line) && i + 1 < lines.length && /^\s*\|?[\s:|-]+\|[\s:|-]*$/.test(lines[i + 1])) {
        paragraphBuffer(para);
        var cells = function (l) {
          return l.replace(/^\s*\|/, '').replace(/\|\s*$/, '').split('|').map(function (c) { return c.trim(); });
        };
        var thead = '<tr>' + cells(line).map(function (c) { return '<th>' + inline(c) + '</th>'; }).join('') + '</tr>';
        i += 2;
        var body = '';
        while (i < lines.length && /\|/.test(lines[i]) && lines[i].trim() !== '') {
          body += '<tr>' + cells(lines[i]).map(function (c) { return '<td>' + inline(c) + '</td>'; }).join('') + '</tr>';
          i += 1;
        }
        html.push('<table><thead>' + thead + '</thead><tbody>' + body + '</tbody></table>');
        continue;
      }

      // list
      if (/^\s*(?:[-*+]|\d+\.)\s+/.test(line)) {
        paragraphBuffer(para);
        var indent = (line.match(/^\s*/) || [''])[0].length;
        var list = parseList(i, indent);
        html.push(list.html);
        i = list.next;
        continue;
      }

      // paragraph line
      para.push(line);
      i += 1;
    }
    paragraphBuffer(para);
    return html.join('\n');
  }

  // --- mounting ---------------------------------------------------------------

  function mount(el, url) {
    el.setAttribute('aria-busy', 'true');
    return fetch(url)
      .then(function (r) {
        if (!r.ok) throw new Error('HTTP ' + r.status);
        return r.text();
      })
      .then(function (md) {
        el.innerHTML = render(md);
        el.removeAttribute('aria-busy');
        if (el.hasAttribute('data-md-title')) {
          var h1 = el.querySelector('h1');
          if (h1) document.title = h1.textContent + ' - Federate Network';
        }
      })
      .catch(function (e) {
        el.removeAttribute('aria-busy');
        el.innerHTML = '<p class="md-error">Could not load <code>' + escapeHtml(url) +
          '</code> (' + escapeHtml(e.message) + '). Reload to try again.</p>';
      });
  }

  function auto() {
    var nodes = document.querySelectorAll('[data-md-src]');
    for (var i = 0; i < nodes.length; i++) {
      mount(nodes[i], nodes[i].getAttribute('data-md-src'));
    }
  }

  var api = { render: render, mount: mount };

  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api; // node (tests)
  }
  if (root && root.document) {
    root.fedMD = api;
    if (root.document.readyState === 'loading') {
      root.document.addEventListener('DOMContentLoaded', auto);
    } else {
      auto();
    }
  }
})(typeof window !== 'undefined' ? window : null);
