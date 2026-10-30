// markdown.js — small, escape-first Markdown → HTML renderer.
// All source text is HTML-escaped before any markup is added, so model/user
// output can never inject tags. Block elements get dir="auto" so Arabic and
// English paragraphs each pick their own direction; code blocks are forced LTR
// (CSS gives each line unicode-bidi: plaintext). Code blocks carry a header
// with the language and a copy button (handled by installCodeCopy()).

import { escapeHtml as esc, copyText } from "./dom.js";
import { t } from "./i18n.js";
import { icon } from "./icons.js";

const PH = "\u0000"; // placeholder delimiter for inline code spans, link tags, table cells

/**
 * Inline formatting: `code`, **bold**, *italic*, ~~strike~~, [link](http…).
 * Code spans and link tags are swapped for placeholders first, so `*`/`_` inside them
 * (e.g. a URL with __init__) are never treated as emphasis. Emphasis follows simplified
 * CommonMark flanking rules: no `*`/`__` opener inside a word or right after `(`/`,`,
 * none followed by a space, and no closer preceded by a space — so `f(*args, **kwargs)`
 * and `2*3*4` stay plain.
 */
export function renderInline(src) {
  const held = [];
  const hold = html => { held.push(html); return PH + (held.length - 1) + PH; };
  let s = String(src)
    .replace(/`([^`]+)`/g, (_, c) => hold(`<code>${esc(c)}</code>`))
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, (_, text, url) =>
      hold(`<a href="${esc(url)}" target="_blank" rel="noopener noreferrer">`) + text + hold("</a>"));
  s = esc(s)
    .replace(/\*\*(?=[^\s*])([^*]*[^\s*])\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^\p{L}\p{N}_])__(?=[^\s_])([^_]*[^\s_])__(?![\p{L}\p{N}_])/gu, "$1<strong>$2</strong>")
    .replace(/(^|[^\p{L}\p{N}_*(,])\*(?=[^\s*])([^*]*[^\s*])\*(?![\p{L}\p{N}_*])/gu, "$1<em>$2</em>")
    .replace(/~~([^~]+)~~/g, "<del>$1</del>");
  return s.replace(new RegExp(PH + "(\\d+)" + PH, "g"), (_, i) => held[+i]);
}

/** Languages the preview panel can run (see preview.js). */
export const RUNNABLE = new Set(["html", "htm", "xhtml", "svg", "js", "javascript"]);

/**
 * Syntax-highlight with the bundled highlight.js (ui/vendor, a classic script
 * exposing window.hljs). Its output is escaped HTML; fall back to plain escaping.
 */
const HL_ALIAS = { htm: "xml", xhtml: "xml", svg: "xml" }; // fence names highlight.js doesn't know

function highlight(lang, code) {
  const hljs = globalThis.hljs;
  const lower = (lang || "").toLowerCase();
  const name = HL_ALIAS[lower] || lower;
  try {
    if (hljs && name && hljs.getLanguage(name)) return hljs.highlight(code, { language: name, ignoreIllegals: true }).value;
  } catch { /* fall through */ }
  return esc(code);
}

function codeBlock(lang, code) {
  const name = (lang || "").toLowerCase();
  const run = RUNNABLE.has(name)
    ? `<button type="button" class="code-btn run-code" title="${esc(t("runPreview"))}">${icon("play", 14)}<span>${esc(t("run"))}</span></button>`
    : "";
  return `<div class="codebox" dir="ltr" data-lang="${esc(name)}"><div class="codebar"><span class="lang">${esc(lang || "code")}</span>` +
    `<span class="code-actions">${run}<button type="button" class="code-btn copy-code" title="${esc(t("copy"))}">${icon("copy", 14)}<span>${esc(t("copy"))}</span></button></span></div>` +
    `<pre><code class="hljs">${highlight(name, code)}</code></pre></div>`;
}

const RE = {
  fence: /^\s*(`{3,}|~{3,})\s*([\w+#.-]*)/,
  heading: /^(#{1,6})\s+(.*?)\s*#*\s*$/,
  hr: /^\s*([-*_])(\s*\1){2,}\s*$/,
  tableRow: /^\s*\|.*\|\s*$/,
  tableSep: /^\s*\|[\s:|-]+\|\s*$/,
  quote: /^\s*>/,
  listItem: /^\s*([-*+]|\d+[.)])\s+/,
  blank: /^\s*$/,
  blockStart: /^\s*(`{3,}|~{3,}|#{1,6}\s|>|([-*+]|\d+[.)])\s)/,
};

const STRONG = /([A-Za-z\u00C0-\u024F\u0370-\u03FF\u0400-\u04FF])|([\u0590-\u08FF\uFB1D-\uFDFF\uFE70-\uFEFC])/;

/**
 * Direction of a block from its first strong character ("rtl" | "ltr" | "auto").
 * Used for list/table/quote containers: the browser's dir="auto" would skip
 * children that carry their own dir attribute, so we decide it here.
 */
export function detectDir(text) {
  const plain = String(text).replace(/`[^`]*`/g, "").replace(/\]\([^)]*\)/g, "]");
  const m = plain.match(STRONG);
  return !m ? "auto" : m[2] ? "rtl" : "ltr";
}

/** Table row → cells. `|` inside inline code or escaped as `\|` does not split a cell. */
const splitRow = r => {
  const held = [];
  const s = r.trim()
    .replace(/`[^`]*`/g, m => { held.push(m); return PH + (held.length - 1) + PH; })
    .replace(/\\\|/g, () => { held.push("|"); return PH + (held.length - 1) + PH; });
  return s.replace(/^\||\|$/g, "").split("|")
    .map(c => c.trim().replace(new RegExp(PH + "(\\d+)" + PH, "g"), (_, i) => held[+i]));
};

const indentOf = line => line.match(/^[ \t]*/)[0].replace(/\t/g, "    ").length;

/** <ul>/<ol> for parsed list items (type and start number come from the first item's marker). */
function listHtml(items) {
  const first = items[0].line;
  const ordered = /^\s*\d/.test(first);
  const start = ordered ? parseInt(first, 10) : 1;
  const lis = items.map(x =>
    `<li dir="auto">${renderInline(x.text)}${x.children?.length ? listHtml(x.children) : ""}</li>`).join("");
  const dir = detectDir(items.map(x => x.text).join(" "));
  return ordered
    ? `<ol dir="${dir}"${start > 1 ? ` start="${start}"` : ""}>${lis}</ol>`
    : `<ul dir="${dir}">${lis}</ul>`;
}

/** Render a Markdown string to safe HTML. Tolerates unterminated fences (streaming). */
export function renderMarkdown(src) {
  const lines = String(src ?? "").replace(/\r/g, "").replaceAll(PH, "").split("\n");
  let out = "";
  let i = 0;
  while (i < lines.length) {
    const l = lines[i];

    const fence = l.match(RE.fence);
    if (fence) {
      const close = new RegExp("^\\s*" + fence[1][0] + "{" + fence[1].length + ",}\\s*$");
      const buf = [];
      i++;
      while (i < lines.length && !close.test(lines[i])) buf.push(lines[i++]);
      i++;
      out += codeBlock(fence[2], buf.join("\n"));
      continue;
    }
    if (RE.blank.test(l)) { i++; continue; }

    const h = l.match(RE.heading);
    if (h) { const n = h[1].length; out += `<h${n} dir="auto">${renderInline(h[2])}</h${n}>`; i++; continue; }

    if (RE.hr.test(l)) { out += "<hr>"; i++; continue; }

    if (RE.tableRow.test(l) && i + 1 < lines.length && RE.tableSep.test(lines[i + 1])) {
      let tbl = `<div class="table-wrap"><table dir="${detectDir(l)}"><thead><tr>` +
        splitRow(l).map(c => `<th>${renderInline(c)}</th>`).join("") + "</tr></thead><tbody>";
      i += 2;
      while (i < lines.length && RE.tableRow.test(lines[i])) {
        tbl += "<tr>" + splitRow(lines[i++]).map(c => `<td>${renderInline(c)}</td>`).join("") + "</tr>";
      }
      out += tbl + "</tbody></table></div>";
      continue;
    }

    if (RE.quote.test(l)) {
      const buf = [];
      while (i < lines.length && RE.quote.test(lines[i])) buf.push(lines[i++].replace(/^\s*>\s?/, ""));
      out += `<blockquote dir="${detectDir(buf.join(" "))}">${renderMarkdown(buf.join("\n"))}</blockquote>`;
      continue;
    }

    if (RE.listItem.test(l)) {
      // One level of nesting: items indented 2+ spaces deeper than the list's first item
      // become a sub-list of the previous item (deeper levels join that sub-list).
      const baseIndent = indentOf(l);
      const items = []; // { line, text, children: [{ line, text }] }
      while (i < lines.length && (RE.listItem.test(lines[i]) || (items.length && /^\s{2,}\S/.test(lines[i])))) {
        const line = lines[i++];
        const last = items[items.length - 1];
        if (!RE.listItem.test(line)) {
          const target = last.children.length ? last.children[last.children.length - 1] : last;
          target.text += " " + line.trim();
        } else if (last && indentOf(line) >= baseIndent + 2) {
          last.children.push({ line, text: line.replace(RE.listItem, "") });
        } else {
          items.push({ line, text: line.replace(RE.listItem, ""), children: [] });
        }
      }
      out += listHtml(items);
      continue;
    }

    const buf = [];
    while (i < lines.length && !RE.blank.test(lines[i]) && !RE.blockStart.test(lines[i])) buf.push(lines[i++]);
    if (!buf.length) buf.push(lines[i++]);
    out += `<p dir="auto">${buf.map(renderInline).join("<br>")}</p>`;
  }
  return out;
}

/** One delegated click handler for every code block's copy button. Call once. */
export function installCodeCopy(root = document) {
  root.addEventListener("click", async e => {
    const btn = e.target.closest?.(".copy-code");
    if (!btn) return;
    const code = btn.closest(".codebox")?.querySelector("code")?.textContent ?? "";
    const ok = await copyText(code);
    const label = btn.querySelector("span");
    if (!label) return;
    label.textContent = ok ? t("copied") : t("copyFailed");
    btn.classList.toggle("done", ok);
    setTimeout(() => { label.textContent = t("copy"); btn.classList.remove("done"); }, 1500);
  });
}
