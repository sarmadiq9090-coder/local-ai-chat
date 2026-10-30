// preview.js — live preview panel (like Claude's Artifacts) for HTML / SVG / JS
// code blocks. Clicking "تشغيل" on a code block builds a document and runs it
// in a sandboxed iframe (no same-origin access to the app or its API).
// Console output and runtime errors are forwarded via postMessage and shown in
// a console drawer; "اطلب الإصلاح" sends the errors back to the model.

import { $, el, iconButton } from "./dom.js";
import { t } from "./i18n.js";
import { icon } from "./icons.js";
import { send } from "./chat.js";
import { state } from "./state.js";

const WIDTH_KEY = "previewWidth";
const MAX_LOGS = 200;

let panel, frame, consoleEl, errorBar, errorCount;
let current = null;  // { lang, code, siblings }
let logs = [];

/** Injected first into every preview: forwards console + errors to the app. */
const BRIDGE = `<script>(() => {
  const post = (type, args) => { try { parent.postMessage({ __preview: true, type, text: args.map(a => {
    if (a instanceof Error) return a.stack || a.message;
    if (typeof a === "object") { try { return JSON.stringify(a); } catch { return String(a); } }
    return String(a); }).join(" ") }, "*"); } catch {} };
  for (const k of ["log", "info", "warn", "error"]) { const o = console[k]; console[k] = (...a) => { post(k, a); o.apply(console, a); }; }
  addEventListener("error", e => post("error", [e.message + (e.lineno ? " (line " + e.lineno + ")" : "")]));
  addEventListener("unhandledrejection", e => post("error", ["Unhandled promise rejection: " + (e.reason?.message || e.reason)]));
})();<\/script>`;

/** Build a full HTML document from a code block plus sibling css/js blocks of the same message. */
export function buildDocument(lang, code, siblings = []) {
  const css = siblings.filter(s => s.lang === "css").map(s => s.code).join("\n");
  const js = siblings.filter(s => s.lang === "js" || s.lang === "javascript").map(s => s.code).join("\n;\n");
  const base = `<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">${BRIDGE}`;

  if (lang === "svg") {
    return `<!doctype html><html><head>${base}<style>html,body{margin:0;height:100%;display:grid;place-items:center;background:#fff}svg{max-width:100%;max-height:100%}</style></head><body>${code}</body></html>`;
  }
  if (lang === "js" || lang === "javascript") {
    return `<!doctype html><html><head>${base}<style>body{font-family:system-ui,sans-serif;margin:16px}</style></head><body><script>${escapeScript(code)}<\/script></body></html>`;
  }

  // HTML: inject the bridge into <head>, and sibling css/js only if the page has none of its own.
  let doc = /<html[\s>]/i.test(code) ? code : `<!doctype html><html><head></head><body>${code}</body></html>`;
  if (!/<head[\s>]/i.test(doc)) doc = doc.replace(/<html[^>]*>/i, m => `${m}<head></head>`);
  doc = doc.replace(/<head[^>]*>/i, m => `${m}${base}`);
  if (css && !/<style[\s>]/i.test(code)) doc = doc.replace(/<\/head>/i, `<style>${css}</style></head>`);
  if (js && !/<script[\s>]/i.test(code)) {
    const tag = `<script>${escapeScript(js)}<\/script>`;
    doc = /<\/body>/i.test(doc) ? doc.replace(/<\/body>(?![\s\S]*<\/body>)/i, `${tag}</body>`) : doc + tag;
  }
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
    iconButton("x", t("close"), () => closePreview()));
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

  installResize(handle);
  restoreWidth();

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
    addLog(e.data.type, String(e.data.text ?? ""));
  });
}

function open() {
  panel.hidden = false;
  document.body.classList.add("previewing");
  panel.querySelector(".preview-lang").textContent = current.lang;
  run();
}

function run() {
  if (!current) return;
  logs = [];
  renderLogs();
  frame.srcdoc = buildDocument(current.lang, current.code, current.siblings);
  frame.onload = () => frame.focus(); // keyboard games get input immediately
}

/** Close the panel. Returns true if it was open (for the Esc key chain). */
export function closePreview() {
  if (!panel || panel.hidden) return false;
  panel.hidden = true;
  panel.classList.remove("max");
  document.body.classList.remove("previewing");
  frame.srcdoc = "";
  return true;
}

function addLog(type, text) {
  logs.push({ type, text });
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
  if (!errors || state.streaming) return;
  send(t("fixPrompt", { errors }));
}

// ---------- resizing ----------

function restoreWidth() {
  try {
    const w = parseInt(localStorage.getItem(WIDTH_KEY), 10);
    if (w > 0) panel.style.width = w + "px";
  } catch { /* storage unavailable */ }
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
      panel.style.width = Math.max(320, Math.min(w, window.innerWidth - 360)) + "px";
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      document.body.classList.remove("resizing");
      try { localStorage.setItem(WIDTH_KEY, String(parseInt(panel.style.width, 10))); } catch { /* ignore */ }
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up, { once: true });
  });
}
