// markdown.js — small, escape-first Markdown → HTML renderer.
// All source text is HTML-escaped before any markup is added, so model/user
// output can never inject tags. Block elements get dir="auto" so Arabic and
// English paragraphs each pick their own direction; code blocks are forced LTR
// (CSS gives each line unicode-bidi: plaintext). Code blocks carry a header
// with the language and a copy button (handled by installCodeCopy()).

import { escapeHtml as esc, copyText } from "./dom.js";
import { t } from "./i18n.js";
import { icon } from "./icons.js";

const PH = "\u0000"; // placeholder delimiter for inline code spans

/** Inline formatting: `code`, **bold**, *italic*, ~~strike~~, [link](http…). */
export function renderInline(src) {
  const codes = [];
  let s = String(src).replace(/`([^`]+)`/g, (_, c) => { codes.push(c); return PH + (codes.length - 1) + PH; });
  s = esc(s)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/__([^_]+)__/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>")
    .replace(/~~([^~]+)~~/g, "<del>$1</del>")
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
  return s.replace(new RegExp(PH + "(\\d+)" + PH, "g"), (_, i) => `<code>${esc(codes[+i])}</code>`);
}

/** Languages the preview panel can run (see preview.js). */
export const RUNNABLE = new Set(["html", "htm", "xhtml", "svg", "js", "javascript"]);

/**
 * Syntax-highlight with the bundled highlight.js (ui/vendor, a classic script
 * exposing window.hljs). Its output is escaped HTML; fall back to plain escaping.
 */
function highlight(lang, code) {
  const hljs = globalThis.hljs;
  const name = (lang || "").toLowerCase();
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

const splitRow = r => r.trim().replace(/^\||\|$/g, "").split("|").map(c => c.trim());

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
      const ordered = /^\s*\d/.test(l);
      const start = ordered ? parseInt(l, 10) : 1;
      const items = [];
      while (i < lines.length && (RE.listItem.test(lines[i]) || (items.length && /^\s{2,}\S/.test(lines[i])))) {
        if (RE.listItem.test(lines[i])) items.push(lines[i].replace(RE.listItem, ""));
        else items[items.length - 1] += " " + lines[i].trim();
        i++;
      }
      const lis = items.map(x => `<li dir="auto">${renderInline(x)}</li>`).join("");
      const dir = detectDir(items.join(" "));
      out += ordered
        ? `<ol dir="${dir}"${start > 1 ? ` start="${start}"` : ""}>${lis}</ol>`
        : `<ul dir="${dir}">${lis}</ul>`;
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
