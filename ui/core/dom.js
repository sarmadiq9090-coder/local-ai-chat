// dom.js — tiny DOM helpers shared by every module:
// $ / $$ selectors, el() element builder, escapeHtml, clipboard copy,
// toast notifications, a status line setter, and a modal/confirm system
// (with a stack so Esc closes the top-most dialog).

import { t } from "./i18n.js";
import { icon } from "./icons.js";

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

const ESC_MAP = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
/** Escape text for safe inclusion in HTML markup. */
export function escapeHtml(s) {
  return String(s ?? "").replace(/[&<>"']/g, c => ESC_MAP[c]);
}

/**
 * Build an element. props: class, text, html (TRUSTED markup only — icons or
 * output of renderMarkdown), dataset, style, on<Event> handlers, any attribute.
 * Boolean true/false attributes are set/removed. Children may be nodes, strings
 * (inserted as text), arrays, or null/false (skipped).
 */
export function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v == null || v === false) continue;
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k === "html") node.innerHTML = v;
    else if (k === "dataset") Object.assign(node.dataset, v);
    else if (k === "style" && typeof v === "object") Object.assign(node.style, v);
    else if (k.startsWith("on") && typeof v === "function") node.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k in node && typeof v !== "string") node[k] = v;
    else node.setAttribute(k, v === true ? "" : v);
  }
  appendChildren(node, children);
  return node;
}

function appendChildren(node, children) {
  for (const c of children) {
    if (c == null || c === false) continue;
    if (Array.isArray(c)) appendChildren(node, c);
    else node.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
}

/** Icon-only button with tooltip/aria label. */
export function iconButton(name, label, onClick, cls = "icon-btn") {
  return el("button", { class: cls, type: "button", title: label, "aria-label": label, html: icon(name), onclick: onClick });
}

/** Copy text to the clipboard (with a fallback for non-secure contexts). */
export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const ta = el("textarea", { style: { position: "fixed", opacity: "0" } });
    ta.value = text;
    document.body.append(ta);
    ta.select();
    let ok = false;
    try { ok = document.execCommand("copy"); } catch { /* ignore */ }
    ta.remove();
    return ok;
  }
}

/** Show a transient notification. type: "info" | "success" | "error". */
export function toast(message, type = "info", ms = 3500) {
  let host = $("#toasts");
  if (!host) { host = el("div", { id: "toasts" }); document.body.append(host); }
  const node = el("div", { class: `toast ${type}`, dir: "auto", role: "status" }, message);
  host.append(node);
  requestAnimationFrame(() => node.classList.add("show"));
  setTimeout(() => {
    node.classList.remove("show");
    setTimeout(() => node.remove(), 250);
  }, ms);
}

/** Set the small status text in the top bar. kind: "" | "ok" | "err". */
export function setStatus(text, kind = "") {
  const s = $("#status");
  if (!s) return;
  s.textContent = text;
  s.className = kind;
}

// ---------- modals ----------
const modalStack = [];

/**
 * Open a modal dialog. Returns { el, body, close(force) }.
 * beforeClose() may return false to veto a non-forced close.
 */
export function openModal({ title, body, actions = [], className = "", beforeClose, onClose }) {
  const bodyEl = el("div", { class: "modal-body" }, body);
  const footer = actions.length ? el("div", { class: "modal-foot" }, actions) : null;
  const handle = {};
  const dialog = el("div", { class: `modal ${className}`, role: "dialog", "aria-modal": "true" },
    el("div", { class: "modal-head" },
      el("h2", { dir: "auto" }, title),
      iconButton("x", t("cancel"), () => handle.close())),
    bodyEl, footer);
  const overlay = el("div", { class: "modal-overlay" }, dialog);
  overlay.addEventListener("mousedown", e => { if (e.target === overlay) handle.close(); });

  Object.assign(handle, {
    el: dialog,
    body: bodyEl,
    close(force = false) {
      if (!force && beforeClose && beforeClose() === false) return;
      const i = modalStack.indexOf(handle);
      if (i >= 0) modalStack.splice(i, 1);
      overlay.remove();
      onClose?.();
    },
  });
  modalStack.push(handle);
  document.body.append(overlay);
  requestAnimationFrame(() => overlay.classList.add("show"));
  const first = dialog.querySelector("input, textarea, select, .modal-foot .btn.primary");
  first?.focus();
  return handle;
}

/** Close the top-most modal. Returns true if one was open. */
export function closeTopModal() {
  const top = modalStack.at(-1);
  if (!top) return false;
  top.close();
  return true;
}

export const hasOpenModal = () => modalStack.length > 0;

/** Pseudo-feature so the modal stack takes its place in the Esc chain (features/index.js). */
export const modalsFeature = { name: "modals", onEscape: closeTopModal };

/** Promise-based confirm dialog → resolves true/false. */
export function confirmDialog(message, { title = t("confirm"), okText = t("confirm"), danger = false } = {}) {
  return new Promise(resolve => {
    let result = false;
    const ok = el("button", { class: `btn primary${danger ? " danger" : ""}`, type: "button", onclick: () => { result = true; m.close(true); } }, okText);
    const cancel = el("button", { class: "btn", type: "button", onclick: () => m.close(true) }, t("cancel"));
    const m = openModal({
      title,
      className: "small",
      body: el("p", { dir: "auto", class: "confirm-text" }, message),
      actions: [cancel, ok],
      onClose: () => resolve(result),
    });
    ok.focus();
  });
}
