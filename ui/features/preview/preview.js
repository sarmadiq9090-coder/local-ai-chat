// preview.js — live preview panel (like Claude's Artifacts) for HTML / SVG / JS
// code blocks. Clicking "تشغيل" on a code block builds a document and runs it
// in a sandboxed iframe (no same-origin access to the app or its API). The iframe loads
// frame.html, which signals "ready" and receives the document via postMessage: a real
// document does not inherit index.html's CSP (a srcdoc one would block its scripts).
// Console output and runtime errors are forwarded via postMessage and shown in
// a console drawer; "اطلب الإصلاح" sends the errors back to the model.
// Esc: closes the panel — also while the iframe has focus (the injected bridge forwards
// Escape), so pages that use Escape themselves will close the preview too. The "تشغيل" button itself is rendered by core/markdown.js
// (RUNNABLE languages); this module handles its clicks.
// Cross-feature deps: chat (send).

import { $, el, iconButton, toast, hasOpenModal } from "../../core/dom.js";
import { t, registerStrings } from "../../core/i18n.js";
import { icon } from "../../core/icons.js";
import { send } from "../chat/chat.js";
import { state, on } from "../../core/state.js";
import strings from "./strings.js";

registerStrings("preview", strings);

const WIDTH_KEY = "previewWidth";
const MAX_LOGS = 200;
const MIN_MAIN = 360;  // px kept for the chat next to the panel
const MIN_PANEL = 320; // px minimum panel width
const FRAME_URL = new URL("./frame.html", import.meta.url).href;

let panel, frame, consoleEl, errorBar, errorCount;
let current = null;  // { lang, code, siblings }
let pendingDoc = null; // document to hand to frame.html once it says "ready"
let logs = [];
let runStarted = 0; // performance.now() of the last run

/**
 * Injected first into every preview: forwards console + errors to the app, and Esc (the
 * iframe keeps keyboard focus after Run, so the app's Esc chain would never see it).
 * Error line numbers are relative to the failing <script> element; scripts copied from the
 * model's HTML carry data-pl = their starting line in that block, which is added back so the
 * reported line matches the code block (only when the script is known: current script, or
 * the page has exactly one such script).
 */
const BRIDGE = `<script>(() => {
  const post = (type, args) => { try { parent.postMessage({ __preview: true, type, text: args.map(a => {
    if (a instanceof Error) return a.stack || a.message;
    if (typeof a === "object") { try { return JSON.stringify(a); } catch { return String(a); } }
    return String(a); }).join(" ") }, "*"); } catch {} };
  for (const k of ["log", "info", "warn", "error"]) { const o = console[k]; console[k] = (...a) => { post(k, a); o.apply(console, a); }; }
  const lineOf = e => {
    if (!e.lineno) return 0;
    let s = document.currentScript;
    if (!s || !s.hasAttribute("data-pl")) { const all = document.querySelectorAll("script[data-pl]"); s = all.length === 1 ? all[0] : null; }
    return e.lineno + (s ? Number(s.getAttribute("data-pl")) || 0 : 0);
  };
  addEventListener("error", e => { const l = lineOf(e); post("error", [e.message + (l ? " (line " + l + ")" : "")]); });
  addEventListener("unhandledrejection", e => post("error", ["Unhandled promise rejection: " + (e.reason?.message || e.reason)]));
  addEventListener("keydown", e => { if (e.key === "Escape") try { parent.postMessage({ __preview: true, type: "key", key: "Escape" }, "*"); } catch {} }, true);
})();<\/script>`;

const isJs = lang => lang === "js" || lang === "javascript";

/** Value of an attribute inside a start tag (quoted or bare), or null. */
const attrOf = (tag, name) => {
  const m = tag.match(new RegExp(`\\s${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s>]+))`, "i"));
  return m ? (m[1] ?? m[2] ?? m[3]) : null;
};

/** A relative reference to a local file (would 404 against the app server): not a URL, not absolute. */
const isLocalRef = u => u != null && !/^([a-z][a-z0-9+.-]*:|\/\/)/i.test(u.trim());

const baseName = u => u.trim().split(/[?#]/)[0].split("/").pop().toLowerCase();

/**
 * Sibling blocks to inline for a `<link href>` / `<script src>` reference: the only unused block
 * of that type, else blocks that mention the file name in their first lines, else the next unused one.
 * Marks the chosen blocks as used. Returns their joined code ("" = nothing to inline).
 */
function pickSiblings(blocks, used, ref, sep) {
  const free = blocks.filter(b => !used.has(b));
  if (!free.length) return "";
  const name = baseName(ref);
  let chosen = free.length === 1 ? free : free.filter(b => name && b.code.split("\n", 3).join("\n").toLowerCase().includes(name));
  // No name match: take the next block in order (each further reference takes the next one),
  // so an unrelated example block later in the message isn't pulled in.
  if (!chosen.length) chosen = free.slice(0, 1);
  chosen.forEach(b => used.add(b));
  return chosen.map(b => b.code).join(sep);
}

const INLINE_SCRIPT = /<script\b(?![^>]*\ssrc\s*=)[^>]*>([\s\S]*?)<\/script\s*>/gi;

/** Build a full HTML document from a code block plus sibling css/js blocks of the same message. */
export function buildDocument(lang, code, siblings = []) {
  const cssBlocks = siblings.filter(s => s.lang === "css");
  const jsBlocks = siblings.filter(s => isJs(s.lang));
  const base = `<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">${BRIDGE}`;

  if (lang === "svg") {
    return `<!doctype html><html><head>${base}<style>html,body{margin:0;height:100%;display:grid;place-items:center;background:#fff}svg{max-width:100%;max-height:100%}</style></head><body>${code}</body></html>`;
  }
  if (isJs(lang)) {
    return `<!doctype html><html><head>${base}<style>body{font-family:system-ui,sans-serif;margin:16px}</style></head><body><script>${escapeScript(code)}<\/script></body></html>`;
  }

  // HTML. Models often split a page into ```html + ```css + ```js blocks and link them as
  // <link href="styles.css"> / <script src="script.js">: inline the sibling blocks in their place
  // and drop local references nothing can serve (they'd 404 against the app). CDN URLs stay.
  const hasOwnStyle = /<style[\s>]/i.test(code);
  const hasInlineScript = [...code.matchAll(INLINE_SCRIPT)].some(m => m[1].trim());
  const usedCss = new Set(), usedJs = new Set();
  let linkedLocal = false; // the page references local files itself (styles.css, script.js…)
  const early = [], deferred = []; // moved to the end of <body>: from <head> / with `defer` (in that order)

  // Mark the page's own inline scripts with their starting line (see BRIDGE: error line numbers).
  let html = code.replace(INLINE_SCRIPT, (tag, _body, offset) => {
    const line = code.slice(0, offset).split("\n").length - 1;
    return tag.replace(/^<script\b/i, `<script data-pl="${line}"`);
  });
  html = html.replace(/<link\b[^>]*>/gi, tag => {
    if (!/\srel\s*=\s*["']?stylesheet/i.test(tag)) return tag;
    const href = attrOf(tag, "href");
    if (!isLocalRef(href)) return tag;
    linkedLocal = true;
    const css = pickSiblings(cssBlocks, usedCss, href, "\n");
    return css ? `<style>${css.replace(/<\/style/gi, "<\\/style")}</style>` : "";
  });
  const bodyAt = html.search(/<body[\s>]/i);
  html = html.replace(/<script\b([^>]*)>\s*<\/script\s*>/gi, (tag, attrs, offset) => {
    const src = attrOf(tag, "src");
    if (!isLocalRef(src)) return tag;
    linkedLocal = true;
    const js = pickSiblings(jsBlocks, usedJs, src, "\n;\n");
    if (!js) return "";
    const module = /\stype\s*=\s*["']?module/i.test(attrs) ? ' type="module"' : "";
    const inlined = `<script${module}>${escapeScript(js)}<\/script>`;
    // An inline script ignores `defer`, and one in <head> would run before the body exists.
    if (/\sdefer\b/i.test(attrs)) { deferred.push(inlined); return ""; }
    if (bodyAt >= 0 ? offset < bodyAt : /<\/head>/i.test(html.slice(offset))) { early.push(inlined); return ""; }
    return inlined;
  });

  let doc = /<html[\s>]/i.test(html) ? html : `<!doctype html><html><head></head><body>${html}</body></html>`;
  if (!/<head[\s>]/i.test(doc)) doc = doc.replace(/<html[^>]*>/i, m => `${m}<head></head>`);
  doc = doc.replace(/<head[^>]*>/i, m => `${m}${base}`);
  // Unreferenced sibling blocks are added only when they surely belong to this page: it links
  // no local files itself (if it does, it said what it needs) and it is the message's only HTML
  // block (otherwise e.g. a separate JS example would run inside an unrelated page).
  const onlyPage = !siblings.some(s => /^(html?|xhtml)$/.test(s.lang));
  const mergeRest = onlyPage && !linkedLocal;
  const css = mergeRest ? cssBlocks.filter(b => !usedCss.has(b)).map(b => b.code).join("\n") : "";
  if (css && !hasOwnStyle) doc = doc.replace(/<\/head>/i, () => `<style>${css.replace(/<\/style/gi, "<\\/style")}</style></head>`);
  const js = hasInlineScript || !mergeRest ? "" : jsBlocks.filter(b => !usedJs.has(b)).map(b => b.code).join("\n;\n");
  const tail = early.join("") + deferred.join("") + (js ? `<script>${escapeScript(js)}<\/script>` : "");
  if (tail) doc = /<\/body>/i.test(doc) ? doc.replace(/<\/body>(?![\s\S]*<\/body>)/i, () => `${tail}</body>`) : doc + tail;
  return doc;
}

const escapeScript = s => s.replace(/<\/script/gi, "<\\/script");

function collectBlock(box) {
  return { lang: box.dataset.lang || "", code: box.querySelector("code")?.textContent ?? "" };
}

export function initPreview() {
  panel = el("aside", { id: "preview", "aria-label": t("preview"), hidden: true });
  const handle = el("div", { class: "preview-resize", title: t("maximize") });
  const head = el("div", { class: "preview-head" },
    el("span", { class: "preview-title", html: `${icon("play", 14)}<span>${t("preview")}</span>` }),
    el("span", { class: "preview-lang", dir: "ltr" }),
    el("span", { class: "spacer" }),
    iconButton("refresh", t("reload"), () => run()),
    iconButton("expand", t("maximize"), () => panel.classList.toggle("max")),
    iconButton("x", t("closeEsc"), () => closePreview()));
  frame = el("iframe", {
    class: "preview-frame",
    sandbox: "allow-scripts allow-modals allow-pointer-lock",
    title: t("preview"),
  });
  errorCount = el("span", { class: "preview-errcount" });
  errorBar = el("div", { class: "preview-errors", hidden: true },
    el("span", { html: icon("alert", 15) }), errorCount,
    el("span", { class: "spacer" }),
    el("button", { type: "button", class: "btn primary small", onclick: askForFix, html: `${icon("wrench", 14)}<span>${t("fixErrors")}</span>` }));
  consoleEl = el("pre", { class: "preview-console", dir: "ltr" });
  const drawer = el("details", { class: "preview-drawer" },
    el("summary", {}, t("consoleTitle")), consoleEl);
  panel.append(handle, head, frame, errorBar, drawer);
  $("#main").after(panel);
  on("language", () => relabel(head, handle, drawer));

  installResize(handle);
  restoreWidth();
  window.addEventListener("resize", applyWidth);
  $("#side")?.addEventListener("transitionend", applyWidth); // sidebar shown/collapsed: room changed

  document.addEventListener("click", e => {
    const btn = e.target.closest?.(".run-code");
    if (!btn) return;
    const box = btn.closest(".codebox");
    const others = [...(box.closest(".content")?.querySelectorAll(".codebox") ?? [])].filter(b => b !== box);
    current = { ...collectBlock(box), siblings: others.map(collectBlock) };
    open();
  });

  window.addEventListener("message", e => {
    if (e.source !== frame.contentWindow || !e.data?.__preview) return;
    if (e.data.type === "key") {
      if (e.data.key === "Escape" && !hasOpenModal()) closePreview();
      return;
    }
    if (e.data.type === "ready") {
      if (pendingDoc != null) frame.contentWindow.postMessage({ __previewDoc: pendingDoc }, "*");
      pendingDoc = null;
      return;
    }
    addLog(e.data.type, String(e.data.text ?? ""));
  });
}

/** Re-apply translated labels after a language switch. */
function relabel(head, handle, drawer) {
  panel.setAttribute("aria-label", t("preview"));
  frame.title = t("preview");
  handle.title = t("maximize");
  head.querySelector(".preview-title > span:last-child").textContent = t("preview");
  const [reload, max, close] = head.querySelectorAll(".icon-btn");
  [[reload, "reload"], [max, "maximize"], [close, "closeEsc"]].forEach(([b, k]) => { b.title = t(k); b.setAttribute("aria-label", b.title); });
  errorBar.querySelector(".btn span:last-child").textContent = t("fixErrors");
  drawer.querySelector("summary").textContent = t("consoleTitle");
  renderLogs();
}

function open() {
  panel.hidden = false;
  document.body.classList.add("previewing");
  panel.querySelector(".preview-lang").textContent = current.lang;
  applyWidth();
  run();
}

function run() {
  if (!current) return;
  logs = [];
  renderLogs();
  // A JS-only run has nothing to show but its console: open the drawer right away.
  const drawer = panel.querySelector(".preview-drawer");
  drawer.open = current.lang === "js" || current.lang === "javascript";
  runStarted = performance.now();
  pendingDoc = buildDocument(current.lang, current.code, current.siblings);
  frame.src = FRAME_URL;
  frame.onload = () => frame.focus(); // keyboard games get input immediately
}

/** Close the panel. Returns true if it was open (for the Esc key chain). */
export function closePreview() {
  if (!panel || panel.hidden) return false;
  panel.hidden = true;
  panel.classList.remove("max");
  document.body.classList.remove("previewing");
  pendingDoc = null;
  frame.src = "about:blank";
  return true;
}

function addLog(type, text) {
  logs.push({ type, text });
  // Output in the first second (e.g. a script that only logs) is likely what the user wants to see.
  if (performance.now() - runStarted < 1000) panel.querySelector(".preview-drawer").open = true;
  if (logs.length > MAX_LOGS) logs.shift();
  renderLogs();
}

function renderLogs() {
  consoleEl.replaceChildren(...logs.map(l => el("div", { class: `log ${l.type}` }, l.text)));
  consoleEl.scrollTop = consoleEl.scrollHeight;
  const errors = logs.filter(l => l.type === "error");
  errorBar.hidden = !errors.length;
  errorCount.textContent = t("previewErrors", { count: errors.length });
  if (errors.length) panel.querySelector(".preview-drawer").open = true;
}

function askForFix() {
  const errors = [...new Set(logs.filter(l => l.type === "error").map(l => l.text))].slice(0, 10).join("\n");
  if (!errors) return;
  if (state.streaming) { toast(t("previewWaitReply")); return; }
  send(t("fixPrompt", { errors }));
}

// ---------- resizing ----------

let savedWidth = 0; // the user's chosen width (px); the applied width is clamped to the window

function restoreWidth() {
  try {
    const w = parseInt(localStorage.getItem(WIDTH_KEY), 10);
    if (w > 0) savedWidth = w;
  } catch { /* storage unavailable */ }
  applyWidth();
}

/** Room left for the chat + panel: the window minus the part of the sidebar that is on screen. */
function freeWidth() {
  const r = $("#side")?.getBoundingClientRect();
  const side = r && getComputedStyle($("#side")).position !== "fixed"
    ? Math.max(0, Math.min(r.right, window.innerWidth) - Math.max(r.left, 0)) : 0;
  return window.innerWidth - side;
}

/** Keep ≥ MIN_MAIN px for the chat and ≥ MIN_PANEL px for the panel. */
const clampWidth = w => Math.max(MIN_PANEL, Math.min(w, freeWidth() - MIN_MAIN));

function applyWidth() {
  if (!panel || panel.hidden) return;
  panel.style.width = clampWidth(savedWidth || window.innerWidth * 0.46) + "px"; // 0.46 = CSS default 46vw
}

function installResize(handle) {
  handle.addEventListener("pointerdown", e => {
    e.preventDefault();
    handle.setPointerCapture(e.pointerId);
    document.body.classList.add("resizing");
    const rtl = document.dir === "rtl";
    const move = ev => {
      const rect = panel.getBoundingClientRect();
      // In RTL the panel sits on the left, so its free edge is the right one.
      const w = rtl ? ev.clientX - rect.left : rect.right - ev.clientX;
      panel.style.width = clampWidth(w) + "px";
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      document.body.classList.remove("resizing");
      const w = parseInt(panel.style.width, 10); // what the user sees is what we remember
      if (!(w > 0)) return;
      savedWidth = w;
      try { localStorage.setItem(WIDTH_KEY, String(w)); } catch { /* ignore */ }
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up, { once: true });
  });
}

export default { name: "preview", init: initPreview, onEscape: closePreview };
