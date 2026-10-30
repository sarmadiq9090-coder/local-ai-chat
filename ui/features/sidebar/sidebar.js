// sidebar.js — the conversation sidebar and conversation persistence:
// list (grouped by date: today / yesterday / 7 days / 30 days / older),
// search filter, new chat, open, delete (confirm), rename (double-click),
// collapse/expand (desktop: remembered in localStorage; mobile: slide-over).
// Also owns persistConversation(), used by chat.js to PUT conversations, and the
// Ctrl+N "new chat" shortcut. Esc: closes the slide-over sidebar on narrow screens.
// Remembers the open conversation (localStorage) and reopens it on startup. Lists corrupt
// conversation files (backend meta {corrupt: true}) with a warning label so they can be deleted.
// Renaming sets `title_custom` on the conversation (chat.js then never re-titles it).

import { state, on, emit } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, el, iconButton, confirmDialog, toast, hasOpenModal } from "../../core/dom.js";
import { t, registerStrings } from "../../core/i18n.js";
import { icon } from "../../core/icons.js";
import strings from "./strings.js";

registerStrings("sidebar", strings);

const COLLAPSE_KEY = "ui.sidebarCollapsed";
const LAST_KEY = "ui.lastConversation"; // id of the conversation open at exit (reopened on startup)
const MOBILE = window.matchMedia("(max-width: 800px)");
let query = "";
let renaming = false; // suspend list re-renders while an inline rename input is open

// ---------- pure helpers ----------

/** Normalise Arabic/Latin text for forgiving search. */
export function normalizeSearch(s) {
  return String(s || "")
    .toLowerCase()
    .replace(/[ً-ْـ]/g, "") // tashkeel + tatweel
    .replace(/[أإآ]/g, "ا")
    .replace(/ة/g, "ه")
    .replace(/ى/g, "ي");
}

/** Date bucket key for a timestamp relative to `now`. */
export function dateGroup(ts, now = Date.now()) {
  const day = 86400000;
  const startToday = new Date(now); startToday.setHours(0, 0, 0, 0);
  const t0 = startToday.getTime();
  if (ts >= t0) return "groupToday";
  if (ts >= t0 - day) return "groupYesterday";
  if (ts >= t0 - 7 * day) return "groupWeek";
  if (ts >= t0 - 30 * day) return "groupMonth";
  return "groupOlder";
}

/** Short title from the first user message. */
export const titleFrom = text => {
  const s = String(text || "").replace(/\s+/g, " ").trim();
  return s.length > 48 ? s.slice(0, 48) + "…" : s;
};

const metaOf = c => ({ id: c.id, title: c.title, model: c.model ?? null, updated_at: c.updated_at });

// ---------- init / render ----------

export function initSidebar() {
  $("#search").addEventListener("input", e => { query = e.target.value; render(); });
  $("#newChat").addEventListener("click", newChat);
  $("#newChatTop").addEventListener("click", newChat);
  $("#sideToggle").addEventListener("click", toggleSidebar);
  $("#sideToggleTop").addEventListener("click", toggleSidebar);
  $("#backdrop").addEventListener("click", closeMobileSidebar);
  let collapsed = false;
  try { collapsed = localStorage.getItem(COLLAPSE_KEY) === "1"; } catch { /* storage unavailable */ }
  document.body.classList.toggle("side-collapsed", collapsed);
  MOBILE.addEventListener?.("change", closeMobileSidebar);
  on("conversations", render);
  on("current", render);
  on("current", rememberCurrent);
  on("language", render);
  document.addEventListener("keydown", e => {
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "n") {
      e.preventDefault(); // also blocks WebView2's "new window"
      if (!hasOpenModal()) newChat();
    }
  });
}

function render() {
  if (renaming) return;
  const box = $("#convs");
  const q = normalizeSearch(query.trim());
  const items = state.conversations.filter(c => !q || normalizeSearch(c.title || t("untitled")).includes(q));
  if (!items.length) {
    box.replaceChildren(el("div", { class: "conv-empty" }, q ? t("noMatches") : t("noConversations")));
    return;
  }
  const now = Date.now();
  const nodes = [];
  let lastGroup = null;
  for (const c of items) {
    const g = dateGroup(c.updated_at || 0, now);
    if (g !== lastGroup) { nodes.push(el("div", { class: "conv-group" }, t(g))); lastGroup = g; }
    nodes.push(convItem(c));
  }
  box.replaceChildren(...nodes);
}

function convItem(c) {
  if (c.corrupt) return corruptItem(c);
  const title = el("span", { class: "conv-title", dir: "auto", title: t("rename") }, c.title || t("untitled"));
  const item = el("div", {
    class: "conv" + (state.current?.id === c.id ? " active" : ""),
    dataset: { id: c.id },
    role: "button",
    tabindex: "0",
    onclick: () => openConversation(c.id),
    onkeydown: e => { if (e.key === "Enter" && e.target === item) openConversation(c.id); },
  }, title, iconButton("trash", t("delete"), e => { e.stopPropagation(); removeConversation(c); }, "icon-btn conv-del"));
  title.addEventListener("dblclick", e => { e.stopPropagation(); startRename(c, title); });
  return item;
}

/** A conversation file the backend could not parse: labelled, not renamable, deletable. */
function corruptItem(c) {
  const item = el("div", {
    class: "conv corrupt",
    dataset: { id: c.id },
    role: "button",
    tabindex: "0",
    title: t("corruptConvHint"),
    onclick: () => toast(t("corruptConvOpen"), "error", 6000),
    onkeydown: e => { if (e.key === "Enter" && e.target === item) toast(t("corruptConvOpen"), "error", 6000); },
  },
  el("span", { class: "conv-warn", html: icon("alert", 14) }),
  el("span", { class: "conv-title" }, t("corruptConv"), el("span", { class: "conv-id", dir: "ltr" }, ` ${c.id}`)),
  iconButton("trash", t("delete"), e => { e.stopPropagation(); removeConversation({ ...c, title: t("corruptConv") }); }, "icon-btn conv-del"));
  return item;
}

function startRename(meta, titleEl) {
  // A click that preceded the double-click may have re-rendered the list.
  if (!titleEl.isConnected) {
    titleEl = [...document.querySelectorAll("#convs .conv")].find(n => n.dataset.id === meta.id)?.querySelector(".conv-title");
    if (!titleEl) return;
  }
  renaming = true;
  const input = el("input", { class: "conv-rename", dir: "auto", value: meta.title || "" });
  let done = false;
  const finish = async commit => {
    if (done) return;
    done = true;
    renaming = false;
    const next = input.value.trim();
    input.replaceWith(titleEl);
    if (commit && next && next !== meta.title) await renameConversation(meta.id, next);
    else render();
  };
  input.addEventListener("keydown", e => {
    e.stopPropagation(); // keep Esc/Enter away from global shortcuts
    if (e.key === "Enter") finish(true);
    else if (e.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
  input.addEventListener("click", e => e.stopPropagation());
  titleEl.replaceWith(input);
  input.focus();
  input.select();
}

// ---------- operations ----------

/** Reload the conversation list from the backend. */
export async function refreshConversations() {
  try {
    state.conversations = (await api.listConversations()) || [];
  } catch (e) {
    console.warn("conversation list failed", e);
    state.conversations = [];
  }
  emit("conversations");
}

/** Open a stored conversation. */
export async function openConversation(id) {
  if (state.current?.id === id) { closeMobileSidebar(); return; }
  if (state.streaming) { toast(t("busy")); return; }
  try {
    const conv = await api.getConversation(id);
    conv.messages = Array.isArray(conv.messages) ? conv.messages : [];
    state.current = conv;
    emit("current");
    closeMobileSidebar();
  } catch (e) {
    const corrupt = e.code === "corrupt" || (e.status === 500 && /corrupt|parse|invalid|تالف/i.test(e.message));
    toast(corrupt ? t("corruptConvOpen") : t("loadFailed", { error: e.message }), "error", corrupt ? 6000 : undefined);
  }
}

function rememberCurrent() {
  try {
    if (state.current?.id) localStorage.setItem(LAST_KEY, state.current.id);
    else localStorage.removeItem(LAST_KEY);
  } catch { /* storage unavailable */ }
}

/** Startup: load the list, then reopen the conversation that was open last time (if it still exists). */
async function start() {
  let last = null;
  try { last = localStorage.getItem(LAST_KEY); } catch { /* storage unavailable */ }
  await refreshConversations();
  if (!last || state.current) return;
  const meta = state.conversations.find(c => c.id === last);
  if (!meta || meta.corrupt) { try { localStorage.removeItem(LAST_KEY); } catch { /* ignore */ } return; }
  try {
    const conv = await api.getConversation(last);
    if (state.current) return; // the user already started something else
    conv.messages = Array.isArray(conv.messages) ? conv.messages : [];
    state.current = conv;
    emit("current");
  } catch (e) {
    console.warn("reopening the last conversation failed", e);
  }
}

/** Start a fresh, unsaved conversation (created lazily on first send). */
export function newChat() {
  if (state.streaming) { toast(t("busy")); return; }
  state.current = null;
  emit("current");
  closeMobileSidebar();
  emit("focus-composer");
}

async function removeConversation(meta) {
  const ok = await confirmDialog(t("deleteConvConfirm", { title: meta.title || t("untitled") }),
    { title: t("deleteConvTitle"), okText: t("delete"), danger: true });
  if (!ok) return;
  if (state.streaming && state.current?.id === meta.id) { toast(t("busy")); return; }
  try {
    await api.deleteConversation(meta.id);
  } catch (e) {
    toast(e.message, "error");
    return;
  }
  state.conversations = state.conversations.filter(c => c.id !== meta.id);
  if (state.current?.id === meta.id) { state.current = null; emit("current"); }
  emit("conversations");
}

async function renameConversation(id, title) {
  try {
    const conv = state.current?.id === id ? state.current : await api.getConversation(id);
    conv.title = title;
    conv.title_custom = true; // user-chosen: editing the first message must not replace it
    await persistConversation(conv, { touch: false });
    if (state.current?.id === id) emit("current-title");
  } catch (e) {
    toast(t("saveFailed", { error: e.message }), "error");
  }
}

// Saves are serialised so an early PUT can never land after a later one.
let saveChain = Promise.resolve();

/**
 * PUT a conversation (snapshot taken now) and refresh its sidebar entry.
 * touch=true bumps updated_at (moves it to the top).
 */
export function persistConversation(conv, { touch = true } = {}) {
  const now = Date.now();
  conv.created_at ||= now;
  if (touch || !conv.updated_at) conv.updated_at = now;
  const snapshot = JSON.parse(JSON.stringify(conv));
  const run = async () => {
    try {
      await api.saveConversation(snapshot);
    } catch (e) {
      toast(t("saveFailed", { error: e.message }), "error");
      return;
    }
    const meta = metaOf(snapshot);
    const rest = state.conversations.filter(c => c.id !== meta.id);
    state.conversations = [meta, ...rest].sort((a, b) => (b.updated_at || 0) - (a.updated_at || 0));
    emit("conversations");
  };
  saveChain = saveChain.then(run, run);
  return saveChain;
}

// ---------- collapse ----------

export function toggleSidebar() {
  if (MOBILE.matches) {
    document.body.classList.toggle("side-open");
    return;
  }
  const collapsed = document.body.classList.toggle("side-collapsed");
  try { localStorage.setItem(COLLAPSE_KEY, collapsed ? "1" : "0"); } catch { /* ignore */ }
}

/** Close the slide-over sidebar on narrow screens. Returns true if it was open. */
export function closeMobileSidebar() {
  const wasOpen = document.body.classList.contains("side-open");
  document.body.classList.remove("side-open");
  return wasOpen;
}

export default { name: "sidebar", init: initSidebar, start, onEscape: closeMobileSidebar };
