// assistants.js — custom assistants with a knowledge base (RAG). See docs/specs/assistants.md.
// - Sidebar section "Assistants" (inserted above #convs from init()): list, "+ new", edit.
// - Editor modal: name/icon/description/instructions/model/strict, embedding model status + pull,
//   knowledge (file drag-and-drop/upload incl. PDF, notes), chunk counts, delete doc, re-index
//   with progress, delete assistant.
// - Chat integration: a conversation may carry `assistant_id`. state.assistantId is the assistant of
//   the open (or about-to-be-created) chat; chat.js copies it onto new conversations and calls
//   prepareAssistantTurn() before generating and sourcesEl() to render `sources` under replies.
//   A chip in the top bar shows the active assistant (and its model); the empty state of an
//   assistant chat shows its description and suggested questions (rendered into #examples).
// Cross-feature deps: sidebar (closeMobileSidebar), models (loadModels, toGB, modelInstalled, loadCaps),
//   chat (send: suggested questions).
// Events: listens to "current", "models", "language"; emits "current", "focus-composer", "assistants".

import { state, on, emit } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, el, iconButton, toast, openModal, confirmDialog } from "../../core/dom.js";
import { icon } from "../../core/icons.js";
import { t, registerStrings, getLanguage } from "../../core/i18n.js";
import { closeMobileSidebar } from "../sidebar/sidebar.js";
import { loadModels, toGB, modelInstalled, loadCaps } from "../models/models.js";
import { send } from "../chat/chat.js";
import strings, { NOT_IN_KNOWLEDGE } from "./strings.js";

registerStrings("assistants", strings);

const BASE = "/api/app/assistants";
// Same list as the server's `extract::is_supported` (src/assistants/extract.rs).
const FILE_EXTS = ["txt", "md", "csv", "json", "xml", "yml", "yaml", "toml", "html", "css", "js", "ts", "py",
  "rs", "go", "java", "c", "cpp", "h", "cs", "php", "rb", "sh", "ps1", "sql", "ini", "log", "pdf"];
const FILE_ACCEPT = FILE_EXTS.map(e => "." + e).join(",");
const SNIPPET_CHARS = 280;
/** A last question shorter than this is a follow-up: the previous turn joins the search query. */
const SHORT_QUERY = 60;
const PREV_REPLY_CHARS = 200;

let assistants = []; // Assistant[] (newest first)
let listEl, chipEl;

// ---------- small helpers ----------

/** Pluralized string: `key_<rule>` (Intl plural rules of the UI language), falling back to `key_other`. */
function plural(key, n) {
  let rule = "other";
  try { rule = new Intl.PluralRules(getLanguage()).select(n); } catch { /* old engine */ }
  if (n === 0 && t(`${key}_zero`) !== `${key}_zero`) rule = "zero";
  const k = `${key}_${rule}`;
  return t(t(k) !== k ? k : `${key}_other`, { n });
}

/** A translated template with `{name}` replaced by an isolated (<bdi>) user-provided name. */
function withName(template, name) {
  const [before, after = ""] = t(template).split("{name}");
  return el("span", {}, before, el("bdi", {}, name), after);
}

/** Whether a file name has an extension the server can index. */
export function isSupportedFile(name) {
  const i = String(name).lastIndexOf(".");
  return i >= 0 && FILE_EXTS.includes(String(name).slice(i + 1).toLowerCase());
}

const listSep = () => (getLanguage() === "ar" ? "، " : ", ");

// ---------- API ----------

async function call(method, path, body, signal) {
  const opts = { method, signal };
  if (body !== undefined) { opts.headers = { "Content-Type": "application/json" }; opts.body = JSON.stringify(body); }
  const res = await fetch(BASE + path, opts);
  if (!res.ok) throw await httpError(res);
  return res.json();
}

async function httpError(res) {
  let j = null;
  try { j = await res.json(); } catch { /* not JSON */ }
  const err = new Error(j?.error || `HTTP ${res.status}`);
  err.status = res.status;
  if (j?.missing_model) err.missingModel = j.missing_model;
  if (j?.code) err.code = j.code;
  return err;
}

const enc = encodeURIComponent;

/**
 * Run a streaming (NDJSON) operation. Progress lines go to onProgress; the
 * final lines ({doc} / {assistant} / {error}) are returned.
 */
async function streamOp(path, init, onProgress) {
  const res = await fetch(BASE + path, { method: "POST", ...init });
  if (!res.ok) throw await httpError(res);
  const finals = [];
  for await (const obj of api.readNdjson(res)) {
    if (obj && obj.status && !obj.error) onProgress?.(obj);
    else finals.push(obj);
  }
  return finals;
}

let corruptWarned = false;

/** Reload the assistant list and re-render the sidebar section + chip. */
export async function loadAssistants() {
  try {
    const res = await fetch(BASE);
    if (!res.ok) throw await httpError(res);
    assistants = (await res.json()) || [];
    // Unreadable assistant.json files were backed up and left out by the server.
    const corrupt = res.headers.get("x-corrupt-assistants");
    if (corrupt && !corruptWarned) {
      corruptWarned = true;
      toast(t("asst_corrupt", { n: corrupt.split(",").length }), "error", 9000);
    }
  } catch (e) {
    console.warn("assistant list failed", e);
    assistants = [];
  }
  renderList();
  renderChip();
  renderWelcome();
  emit("assistants");
}

const findAssistant = id => assistants.find(a => a.id === id) || null;

// ---------- sidebar section ----------

function initAssistants() {
  const convs = $("#convs");
  listEl = el("div", { class: "asst-list" });
  const title = el("span", { class: "asst-head-title" }, t("asst_section"));
  const addBtn = iconButton("plus", t("asst_new"), () => openEditor(null), "icon-btn asst-add");
  const section = el("section", { class: "asst-section", "aria-label": t("asst_section") },
    el("div", { class: "asst-head" }, title, addBtn), listEl);
  convs?.parentNode.insertBefore(section, convs);

  chipEl = el("button", { class: "asst-chip", type: "button", hidden: true, title: t("asst_chipTitle"),
    onclick: () => { const a = findAssistant(state.assistantId); if (a) openEditor(a); } });
  $("#top .top-start")?.append(chipEl);

  state.assistantId = null;
  // The open chat decides the active assistant (a new chat clears it; startChat sets it after).
  on("current", () => { state.assistantId = state.current?.assistant_id || null; renderList(); renderChip(); renderWelcome(); });
  on("models", () => {
    document.querySelectorAll(".asst-editor").forEach(n => n.dispatchEvent(new Event("models")));
    renderChip();
  });
  // UI language switch (event from the settings/i18n feature): re-label our DOM.
  on("language", () => {
    title.textContent = t("asst_section");
    section.setAttribute("aria-label", t("asst_section"));
    addBtn.title = t("asst_new");
    addBtn.setAttribute("aria-label", t("asst_new"));
    chipEl.title = t("asst_chipTitle");
    renderList();
    renderChip();
    renderWelcome(); // chat re-rendered the generic examples just before (it is earlier in the feature list)
  });
  renderList();
}

function renderList() {
  if (!listEl) return;
  if (!assistants.length) {
    listEl.replaceChildren(el("div", { class: "asst-empty" }, t("asst_none")));
    return;
  }
  listEl.replaceChildren(...assistants.map(a => {
    const item = el("div", {
      class: "asst-item" + (state.assistantId === a.id ? " active" : ""),
      role: "button",
      tabindex: "0",
      title: a.description || t("asst_chat", { name: a.name }),
      onclick: () => startChat(a.id),
      onkeydown: e => { if (e.key === "Enter" && e.target === item) startChat(a.id); },
    },
    el("span", { class: "asst-icon" }, a.icon || "🤖"),
    el("span", { class: "asst-name", dir: "auto" }, a.name),
    iconButton("edit", t("asst_edit"), e => { e.stopPropagation(); openEditor(a); }, "icon-btn asst-edit"));
    return item;
  }));
}

function renderChip() {
  if (!chipEl) return;
  const a = findAssistant(state.assistantId);
  chipEl.hidden = !a;
  if (!a) return;
  // The assistant's own model replaces the app model for its chats: show it in the chip.
  const model = a.model ? el("span", { class: "asst-chip-model", dir: "ltr",
    title: modelInstalled(a.model) ? a.model : t("asst_modelNotInstalled", { name: a.model }) },
  `· ${a.model}`) : null;
  if (model && !modelInstalled(a.model)) model.classList.add("missing");
  chipEl.replaceChildren(el("span", { class: "asst-icon" }, a.icon || "🤖"),
    el("span", { class: "asst-name", dir: "auto" }, a.name), ...(model ? [model] : []));
  chipEl.title = `${t("asst_chipTitle")}${a.model ? ` (${a.model})` : ""}`;
}

/** Start a new (lazily created) chat bound to an assistant. */
export function startChat(id) {
  if (state.streaming) { toast(t("busy")); return; }
  state.current = null;
  emit("current");          // resets state.assistantId (see the "current" listener) …
  state.assistantId = id;   // … then bind the upcoming conversation to this assistant
  renderList();
  renderChip();
  renderWelcome();
  closeMobileSidebar();
  emit("focus-composer");
}

// ---------- empty state of an assistant chat ----------

let genericExamples = null; // chat's example cards, restored when leaving an assistant chat

const docTitle = name => String(name).replace(/\.[a-z0-9]{1,5}$/i, "").replace(/[_-]+/g, " ").trim();

/** Up to 3 suggested questions from the assistant's document names (simple templates). */
export function suggestedQuestions(a, lang = getLanguage()) {
  const names = (a.docs || []).map(d => docTitle(d.name)).filter(Boolean).slice(0, 3);
  const templates = strings[lang]?.asst_suggest || strings.ar.asst_suggest;
  return names.map((n, i) => templates[i % templates.length].replace("{doc}", n));
}

/**
 * When the open chat is an empty assistant chat, show the assistant's description and
 * suggested questions in #examples instead of the generic cards (chat renders those).
 */
function renderWelcome() {
  const box = $("#examples");
  if (!box) return;
  const a = findAssistant(state.assistantId);
  const empty = !state.current?.messages?.length;
  const mine = box.querySelector(":scope > .asst-welcome");
  if (!a || !empty) {
    if (mine) box.replaceChildren(...(genericExamples || []));
    genericExamples = null;
    return;
  }
  if (!mine) genericExamples = [...box.children];
  const questions = suggestedQuestions(a);
  const head = el("div", { class: "asst-welcome" },
    el("span", { class: "asst-welcome-icon" }, a.icon || "🤖"),
    el("div", { class: "asst-welcome-text" },
      el("div", { class: "asst-welcome-name", dir: "auto" }, a.name),
      el("div", { class: "asst-welcome-desc", dir: "auto" }, a.description || t("asst_welcomeNoDesc"))));
  const cards = questions.map(q => el("button", {
    type: "button",
    class: "example asst-suggest",
    onclick: () => { if (!state.streaming) send(q); },
  }, el("span", { class: "ex-title" }, t("asst_suggestTitle")), el("span", { class: "ex-text", dir: "auto" }, q)));
  const noDocs = !a.docs?.length
    ? el("button", { type: "button", class: "example asst-suggest", onclick: () => openEditor(a) },
      el("span", { class: "ex-title" }, t("asst_knowledge")), el("span", { class: "ex-text" }, t("asst_welcomeAddDocs")))
    : null;
  box.replaceChildren(head, ...cards, ...(noDocs ? [noDocs] : []));
}

// ---------- chat integration (used by features/chat/chat.js) ----------

/** "ar" if the text is mostly Arabic script, else "en". */
export function detectLang(text) {
  const s = String(text || "");
  const ar = (s.match(/[؀-ۿݐ-ݿﭐ-﷿ﹰ-﻿]/g) || []).length;
  const lat = (s.match(/[A-Za-z]/g) || []).length;
  return ar >= lat && ar > 0 ? "ar" : "en";
}

const squash = s => String(s || "").replace(/\s+/g, " ").replace(/[.!؟?。]+$/u, "").trim();

/** Whether a reply is (just) the "not in knowledge" sentence, in either language. */
export function isNotInKnowledge(text) {
  const s = squash(text);
  return !!s && Object.values(NOT_IN_KNOWLEDGE).some(x => {
    const n = squash(x);
    return s === n || (s.includes(n) && s.length <= n.length + 12);
  });
}

/** System message: instructions + strict/soft rules + numbered sources (or an explicit "no sources"). */
export function buildAssistantSystem(a, results, question) {
  const sentence = NOT_IN_KNOWLEDGE[detectLang(question)];
  if (!results.length) {
    // No passage passed the threshold: say so plainly and forbid citations, so the model
    // doesn't answer from earlier turns and cite "[1]".
    const rules = a.strict ? t("asst_strictNoSources", { sentence }) : t("asst_softNoSources");
    return [(a.instructions || "").trim(), `## ${t("asst_rulesHead")}\n${rules}`].filter(Boolean).join("\n\n");
  }
  const rules = a.strict ? t("asst_strictRules", { sentence }) : t("asst_softRules");
  const sources = results.map((r, i) => `[${t("asst_srcLabel")} ${i + 1}: ${r.doc_name}]\n${r.text}`).join("\n\n");
  return [
    (a.instructions || "").trim(),
    `## ${t("asst_rulesHead")}\n${rules}`,
    `## ${t("asst_sourcesHead")}\n${sources}`,
  ].filter(Boolean).join("\n\n");
}

/** Openers of a follow-up question ("and …", "what about …", «وماذا عن …», «و…»). */
// Arabic «و/ف» prefixes also start ordinary words: a false match only adds a merged context search.
const FOLLOW_UP = /^(?:(?:and|or|but|also|so|then|what about|how about|what if|same for)\b|و|ف|ثم|أو|او|لكن|طيب|ماذا عن|أيضا|ايضا)/i;

/** Whether a (short) message reads like a follow-up that depends on the previous turn. */
export function isFollowUp(text) {
  const s = String(text || "").trim();
  if (!s || s.length >= SHORT_QUERY) return false;
  return FOLLOW_UP.test(s) || s.split(/\s+/).length <= 2;
}

/**
 * The knowledge-search query for the next reply: the last user message; when it is a short
 * follow-up ("and used products?", «وبدل السكن؟»), the previous user message and the start of
 * the last reply are added so the search knows what it refers to. prepareAssistantTurn also
 * searches the bare message, so a wrongly detected follow-up only adds candidates.
 */
export function retrievalQuery(messages) {
  const msgs = (messages || []).filter(m => m.role !== "assistant" || m.content);
  let li = -1;
  for (let i = msgs.length - 1; i >= 0; i--) if (msgs[i].role === "user" && msgs[i].content?.trim()) { li = i; break; }
  if (li < 0) return "";
  const last = msgs[li].content.trim();
  if (!isFollowUp(last)) return last;
  let prevUser = "", prevReply = "";
  for (let i = li - 1; i >= 0; i--) {
    const m = msgs[i];
    if (!prevReply && !prevUser && m.role === "assistant" && !isNotInKnowledge(m.content)) {
      prevReply = m.content.trim().slice(0, PREV_REPLY_CHARS);
    }
    if (m.role === "user" && m.content?.trim()) { prevUser = m.content.trim(); break; }
  }
  return [prevUser, prevReply, last].filter(Boolean).join("\n");
}

/** Re-index an assistant from the chat (stale index); resolves true on success. */
async function reindexFromChat(a, signal) {
  toast(withName("asst_reindexing", a.name), "info", 4000);
  let ok = false;
  try {
    const finals = await streamOp(`/${enc(a.id)}/reindex`, { signal });
    ok = finals.some(f => f.assistant);
    const err = finals.find(f => f.error);
    if (err) toast(err.missing_model ? t("asst_modelMissing", { name: err.missing_model }) : t("asst_failed", { error: err.error }), "error", 8000);
    else if (ok) toast(t("asst_reindexed"), "success");
  } catch (e) {
    if (e.name === "AbortError") throw e;
    toast(e.missingModel ? t("asst_modelMissing", { name: e.missingModel }) : t("asst_failed", { error: e.message }), "error", 8000);
  }
  loadAssistants();
  return ok;
}

const RELATIVE_WINDOW = 0.15; // same as the server's index::RELATIVE_WINDOW
const TOP_K = 5;

/**
 * Search for `query`; when it differs from the bare `question` (a follow-up with context),
 * search both and merge (best score per passage), so context can only add passages.
 */
async function searchKnowledge(a, query, question, signal) {
  const one = async q => (await call("POST", `/${enc(a.id)}/search`, { query: q }, signal))?.results || [];
  const bare = question.trim();
  if (!bare || query === bare) return one(query);
  const [r1, r2] = await Promise.all([one(bare), one(query)]);
  const best = new Map();
  for (const r of [...r1, ...r2]) {
    const key = `${r.doc_id}\u0000${r.text}`;
    if (!best.has(key) || best.get(key).score < r.score) best.set(key, r);
  }
  const merged = [...best.values()].sort((x, y) => y.score - x.score);
  const top = merged[0]?.score ?? 0;
  return merged.filter(r => r.score >= top - RELATIVE_WINDOW).slice(0, TOP_K);
}

/**
 * Retrieve passages for the latest question of `conv` and build the turn.
 * Returns { system, model, sources } or null (assistant gone → normal chat).
 * Throws AbortError if `signal` aborts.
 */
export async function prepareAssistantTurn(conv, signal) {
  let a = findAssistant(conv.assistant_id);
  if (!a) {
    try { a = await call("GET", `/${enc(conv.assistant_id)}`, undefined, signal); }
    catch (e) {
      if (e.name === "AbortError") throw e;
      toast(t("asst_unavailable"), "error");
      return null;
    }
  }
  const lastUser = [...conv.messages].reverse().find(m => m.role === "user");
  const question = lastUser?.content || "";
  const query = retrievalQuery(conv.messages);
  let results = [];
  if (query.trim()) {
    try {
      results = await searchKnowledge(a, query, question, signal);
    } catch (e) {
      if (e.name === "AbortError") throw e;
      if (e.code === "reindex_required") {
        // Index from an older version / another embedding model: offer to rebuild it now.
        const yes = await confirmDialog(withName("asst_reindexAsk", a.name), { title: t("asst_reindex"), okText: t("asst_reindex") });
        if (yes && await reindexFromChat(a, signal)) {
          try { results = await searchKnowledge(a, query, question, signal); }
          catch (e2) { if (e2.name === "AbortError") throw e2; toast(t("asst_searchFailed", { error: e2.message }), "error", 7000); }
        }
      } else {
        toast(e.missingModel ? t("asst_modelMissing", { name: e.missingModel }) : t("asst_searchFailed", { error: e.message }), "error", 7000);
      }
    }
  }
  // The assistant's preferred model, if installed (":latest" aware); its capabilities are
  // fetched now so chat.js can check supportsThinking() for it.
  const model = a.model && modelInstalled(a.model) ? a.model : null;
  if (model) await loadCaps(model);
  return {
    system: buildAssistantSystem(a, results, question),
    model,
    sources: results.map((r, i) => ({
      n: i + 1,
      doc_name: r.doc_name,
      snippet: r.text.length > SNIPPET_CHARS ? r.text.slice(0, SNIPPET_CHARS) + "…" : r.text,
      score: Math.round(r.score * 1000) / 1000,
    })),
  };
}

/** Visible answer text of a rendered reply body (without the thinking box). */
function answerText(body) {
  return [...body.childNodes].filter(n => !(n.matches?.("details.think"))).map(n => n.textContent).join(" ");
}

/** Remove "[1]"-style citations from a rendered reply that has no sources (outside code). */
function stripCitations(body) {
  const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT, {
    acceptNode: n => (n.parentElement?.closest("pre, code, details.think") ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
  const re = /\s?\[(?:(?:المصدر|Source)\s*)?\d{1,2}(?:[:،,][^\]]{0,80})?\](?!\()/g;
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (re.test(n.nodeValue)) { re.lastIndex = 0; n.nodeValue = n.nodeValue.replace(re, ""); }
  }
}

/**
 * Collapsible "Sources" list rendered under an assistant reply. chat.js puts it right after
 * the reply body: once there, it hides itself when the reply is the "not in knowledge"
 * sentence. In an assistant chat with no sources, a hidden marker strips stray "[n]"
 * citations from the reply instead.
 */
export function sourcesEl(sources) {
  if (!sources?.length) {
    if (!state.current?.assistant_id) return null;
    const marker = el("span", { class: "asst-nosrc", hidden: true });
    queueMicrotask(() => {
      const body = marker.previousElementSibling;
      if (body?.classList.contains("body")) stripCitations(body);
    });
    return marker;
  }
  const details = el("details", { class: "asst-sources" },
    el("summary", {}, el("span", { html: icon("file", 14) }), `${t("asst_sources")} (${sources.length})`),
    el("ol", {}, sources.map(s => el("li", {},
      el("div", { class: "asst-src-head" },
        el("span", { class: "asst-src-n", dir: "ltr" }, `[${s.n}]`),
        el("span", { class: "asst-src-name", dir: "auto" }, s.doc_name),
        el("span", { class: "asst-src-score", dir: "ltr" }, Number(s.score).toFixed(2))),
      el("div", { class: "asst-src-snippet", dir: "auto" }, s.snippet)))));
  queueMicrotask(() => {
    const body = details.previousElementSibling;
    if (body?.classList.contains("body") && isNotInKnowledge(answerText(body))) details.remove();
  });
  return details;
}

// ---------- editor ----------

const field = (label, control) => el("label", { class: "field" }, el("span", { class: "field-label" }, label), control);

const isEmbedName = n => /embed|bge|minilm|nomic|e5-/i.test(n);


/** Open the editor for `a` (an Assistant) or for a new assistant (null). */
export function openEditor(a) {
  const isNew = !a;
  let current = a ? { ...a } : null;
  let busy = false;

  const name = el("input", { class: "input", dir: "auto", placeholder: t("asst_namePh"), maxlength: 80 });
  const iconIn = el("input", { class: "input asst-icon-input", maxlength: 8 });
  const desc = el("input", { class: "input", dir: "auto", placeholder: t("asst_descriptionPh") });
  const instr = el("textarea", { class: "input", rows: 5, dir: "auto", placeholder: t("asst_instructionsPh") });
  const model = el("select", { class: "input", dir: "ltr" });
  const strict = el("input", { type: "checkbox" });
  const embed = el("input", { class: "input", dir: "ltr", spellcheck: false });

  name.value = a?.name || "";
  iconIn.value = a?.icon || "🤖";
  desc.value = a?.description || "";
  instr.value = a?.instructions || "";
  strict.checked = a ? !!a.strict : true;
  embed.value = a?.embed_model || "bge-m3";

  const fillModels = () => {
    const keep = model.value || a?.model || "";
    const names = state.models.map(m => m.name).filter(n => !isEmbedName(n));
    if (keep && !names.includes(keep)) names.unshift(keep);
    model.replaceChildren(el("option", { value: "" }, t("asst_modelDefault")), ...names.map(n => el("option", { value: n }, n)));
    model.value = keep;
  };
  fillModels();

  // --- embedding model status + pull ---
  const embedState = el("span", { class: "asst-embed-state" });
  const pullBtn = el("button", { class: "btn small", type: "button", html: icon("download", 14) }, t("asst_pull"));
  const pullBar = el("div", { class: "asst-bar" }, el("div", { class: "asst-bar-fill" }));
  const pullText = el("div", { class: "asst-progress-text muted", dir: "ltr" });
  const pullBox = el("div", { class: "asst-progress", hidden: true }, pullBar, pullText);
  let pullCtl = null;
  const renderEmbedState = () => {
    const m = embed.value.trim();
    const ok = m && modelInstalled(m);
    embedState.className = "asst-embed-state " + (ok ? "ok" : "missing");
    embedState.textContent = ok ? t("asst_embedInstalled") : t("asst_embedMissing");
    pullBtn.hidden = ok || !m || !!pullCtl;
  };
  embed.addEventListener("input", renderEmbedState);
  pullBtn.addEventListener("click", async () => {
    const m = embed.value.trim();
    if (!m || pullCtl) return;
    pullCtl = new AbortController();
    pullBox.hidden = false;
    renderEmbedState();
    setBar(pullBar, 0);
    pullText.textContent = t("asst_pulling");
    try {
      for await (const p of api.pullModel(m, pullCtl.signal)) {
        let text = p.status || "";
        if (p.total) {
          const pct = Math.min(100, ((p.completed || 0) / p.total) * 100);
          setBar(pullBar, pct);
          text += ` — ${toGB(p.completed)} / ${toGB(p.total)} GB (${pct.toFixed(0)}%)`;
        }
        pullText.textContent = text;
      }
      toast(t("asst_pullDone", { name: m }), "success");
      await loadModels();
    } catch (e) {
      if (e.name !== "AbortError") toast(t("asst_pullFailed", { error: e.message }), "error");
    } finally {
      pullCtl = null;
      pullBox.hidden = true;
      renderEmbedState();
    }
  });
  renderEmbedState();
  // In the editor the download button is right here: say so instead of "open the editor".
  const missingMsg = m => t("asst_modelMissingHere", { name: m });

  // --- knowledge ---
  const docsEl = el("div", { class: "asst-docs" });
  const progBar = el("div", { class: "asst-bar" }, el("div", { class: "asst-bar-fill" }));
  const progText = el("div", { class: "asst-progress-text muted" });
  const progBox = el("div", { class: "asst-progress", hidden: true }, progBar, progText);
  const staleBtn = el("button", { class: "btn small", type: "button", html: icon("refresh", 14) }, t("asst_reindexNow"));
  const staleEl = el("div", { class: "asst-stale", hidden: true }, el("span", { html: icon("alert", 15) }),
    el("span", { class: "asst-stale-text" }, t("asst_stale")), staleBtn);
  const fileIn = el("input", { type: "file", multiple: true, accept: FILE_ACCEPT, hidden: true });
  const drop = el("div", { class: "asst-drop" },
    el("span", { html: icon("file", 18) }),
    el("span", {}, t("asst_dropFiles"), " ",
      el("button", { class: "link-btn", type: "button", onclick: () => fileIn.click() }, t("asst_chooseFiles"))),
    el("span", { class: "asst-drop-types muted" }, t("asst_fileTypes")),
    fileIn);
  const addNoteBtn = el("button", { class: "btn small", type: "button", html: icon("edit", 14) }, t("asst_addNote"));
  const reindexBtn = el("button", { class: "btn small", type: "button", html: icon("refresh", 14) }, t("asst_reindex"));
  const knowledge = el("div", { class: "asst-knowledge" },
    el("div", { class: "section-head" }, el("h3", {}, t("asst_knowledge")),
      el("div", { class: "asst-k-actions" }, addNoteBtn, reindexBtn)),
    staleEl, drop, progBox, docsEl);
  const saveFirst = el("p", { class: "muted asst-save-first" }, t("asst_saveFirst"));

  const setBusy = on => {
    busy = on;
    [addNoteBtn, reindexBtn, staleBtn, saveBtn, delBtn].forEach(b => { if (b) b.disabled = on; });
    drop.classList.toggle("disabled", on);
  };
  const showProgress = p => {
    progBox.hidden = false;
    setBar(progBar, p.total ? (p.done / p.total) * 100 : 0);
    const label = t(`asst_status_${p.status}`);
    progText.textContent = p.total > 1 ? `${label} ${p.done}/${p.total}` : label;
  };

  const renderDocs = () => {
    const docs = current?.docs || [];
    if (!docs.length) { docsEl.replaceChildren(el("div", { class: "asst-empty" }, t("asst_noDocs"))); return; }
    docsEl.replaceChildren(...docs.map(d => el("div", { class: "asst-doc" + (d.warning ? " has-warning" : "") },
      el("span", { class: "asst-doc-ic", html: icon(d.kind === "note" ? "edit" : "file", 16) }),
      el("div", { class: "asst-doc-main" },
        el("span", { class: "asst-doc-name", dir: "auto", title: d.name }, d.name),
        d.warning ? el("span", { class: "asst-doc-warn", title: t(`asst_warn_${d.warning}`) },
          el("span", { html: icon("alert", 13) }), t(`asst_warn_${d.warning}`)) : null),
      el("span", { class: "asst-doc-meta muted" }, `${d.kind === "note" ? t("asst_kindNote") : t("asst_kindFile")} · ${plural("asst_chunks", d.chunks)}`),
      d.kind === "note" ? iconButton("edit", t("asst_editNote"), () => editNote(d)) : null,
      iconButton("trash", t("asst_deleteDoc"), () => deleteDoc(d), "icon-btn danger"))));
  };

  const refresh = async () => {
    const fresh = await call("GET", `/${enc(current.id)}`);
    current = fresh;
    staleEl.hidden = !fresh.index?.stale;
    renderDocs();
  };

  /** Handle the final lines of an upload/reindex stream; returns true if all succeeded. */
  const handleFinals = finals => {
    let ok = true;
    for (const f of finals) {
      if (f.error) {
        ok = false;
        const msg = f.missing_model ? missingMsg(f.missing_model) : f.error;
        toast(f.name ? t("asst_docFailed", { name: f.name, error: msg }) : t("asst_failed", { error: msg }), "error", 8000);
      } else if (f.doc) {
        toast(t("asst_indexed", { name: f.doc.name }), "success");
        if (f.doc.warning) toast(`${f.doc.name}: ${t(`asst_warn_${f.doc.warning}`)}`, "error", 9000);
      } else if (f.assistant) toast(t("asst_reindexed"), "success");
    }
    return ok;
  };

  const runOp = async (path, init) => {
    if (busy) { toast(t("asst_busy")); return false; }
    setBusy(true);
    showProgress({ status: "extracting", done: 0, total: 1 });
    try {
      const finals = await streamOp(path, init, showProgress);
      return handleFinals(finals);
    } catch (e) {
      toast(e.missingModel ? missingMsg(e.missingModel) : t("asst_failed", { error: e.message }), "error", 8000);
      return false;
    } finally {
      progBox.hidden = true;
      setBusy(false);
      try { await refresh(); } catch { /* deleted meanwhile */ }
      loadAssistants();
    }
  };

  const uploadFiles = files => {
    if (!files?.length || !current) return;
    // Unsupported files (e.g. a photo dropped along with documents) are skipped, not fatal.
    const good = files.filter(f => isSupportedFile(f.name));
    const skipped = files.filter(f => !isSupportedFile(f.name)).map(f => f.name);
    if (skipped.length) toast(t("asst_skipped", { names: skipped.join(listSep()) }), "error", 8000);
    if (!good.length) return;
    const fd = new FormData();
    for (const f of good) fd.append("file", f, f.name);
    return runOp(`/${enc(current.id)}/docs`, { body: fd });
  };
  const addNote = (noteName, text) => runOp(`/${enc(current.id)}/docs`, {
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name: noteName, text }),
  });
  const reindex = () => runOp(`/${enc(current.id)}/reindex`, {});

  fileIn.addEventListener("change", () => { uploadFiles([...fileIn.files]); fileIn.value = ""; });
  let depth = 0;
  drop.addEventListener("dragenter", e => { e.preventDefault(); depth++; drop.classList.add("drag"); });
  drop.addEventListener("dragover", e => e.preventDefault());
  drop.addEventListener("dragleave", () => { if (--depth <= 0) { depth = 0; drop.classList.remove("drag"); } });
  drop.addEventListener("drop", e => {
    e.preventDefault();
    e.stopPropagation();
    depth = 0;
    drop.classList.remove("drag");
    if (!busy) uploadFiles([...(e.dataTransfer?.files || [])]);
  });
  addNoteBtn.addEventListener("click", () => openNoteEditor(null, (n, txt) => addNote(n, txt)));
  reindexBtn.addEventListener("click", reindex);
  staleBtn.addEventListener("click", reindex);

  async function editNote(d) {
    let text = "";
    try { text = (await call("GET", `/${enc(current.id)}/docs/${enc(d.id)}`)).text || ""; }
    catch (e) { toast(e.message, "error"); return; }
    openNoteEditor({ name: d.name, text }, async (n, txt) => {
      // Notes are replaced: index the new version first, then drop the old one.
      if (await addNote(n, txt)) {
        try { await call("DELETE", `/${enc(current.id)}/docs/${enc(d.id)}`); } catch (e) { toast(e.message, "error"); }
        await refresh();
        loadAssistants();
      }
    });
  }

  async function deleteDoc(d) {
    if (busy) { toast(t("asst_busy")); return; }
    const ok = await confirmDialog(t("asst_deleteDocConfirm", { name: d.name }), { title: t("asst_deleteDoc"), okText: t("delete"), danger: true });
    if (!ok) return;
    try { await call("DELETE", `/${enc(current.id)}/docs/${enc(d.id)}`); } catch (e) { toast(e.message, "error"); }
    await refresh();
    loadAssistants();
  }

  // --- footer ---
  const saveBtn = el("button", { class: "btn primary", type: "button" }, isNew ? t("asst_create") : t("asst_save"));
  const cancelBtn = el("button", { class: "btn", type: "button", onclick: () => m.close() }, t("cancel"));
  const delBtn = isNew ? null : el("button", { class: "btn asst-del", type: "button", html: icon("trash", 15) }, t("asst_delete"));
  const footSpacer = el("span", { class: "spacer" });

  saveBtn.addEventListener("click", async () => {
    const body = {
      name: name.value.trim(),
      icon: iconIn.value.trim() || "🤖",
      description: desc.value.trim(),
      instructions: instr.value,
      model: model.value || null,
      strict: strict.checked,
      embed_model: embed.value.trim() || "bge-m3",
    };
    if (!body.name) { toast(t("asst_nameRequired"), "error"); name.focus(); return; }
    saveBtn.disabled = true;
    try {
      if (isNew) {
        const created = await call("POST", "", body);
        m.close(true);
        await loadAssistants();
        openEditor(created); // continue with knowledge
        return;
      }
      const embedChanged = body.embed_model !== current.embed_model;
      current = { ...current, ...(await call("PUT", `/${enc(current.id)}`, body)) };
      toast(t("asst_saved"), "success");
      await loadAssistants();
      if (embedChanged && current.docs?.length) await reindex();
      else await refresh();
    } catch (e) {
      toast(t("asst_failed", { error: e.message }), "error");
    } finally {
      saveBtn.disabled = busy;
    }
  });

  delBtn?.addEventListener("click", async () => {
    const ok = await confirmDialog(t("asst_deleteConfirm", { name: current.name }), { title: t("asst_deleteTitle"), okText: t("delete"), danger: true });
    if (!ok) return;
    try { await call("DELETE", `/${enc(current.id)}`); } catch (e) { toast(e.message, "error"); return; }
    m.close(true);
    if (state.assistantId === current.id && !state.current) state.assistantId = null;
    await loadAssistants();
  });

  const body = el("div", { class: "form asst-editor" },
    el("div", { class: "field-row" },
      el("label", { class: "field asst-icon-field" }, el("span", { class: "field-label" }, t("asst_icon")), iconIn),
      field(t("asst_name"), name)),
    field(t("asst_description"), desc),
    field(t("asst_instructions"), instr),
    el("div", { class: "field-row" },
      field(t("asst_model"), model),
      el("label", { class: "field" },
        el("span", { class: "field-label" }, t("asst_embedModel"), " ", embedState),
        el("div", { class: "asst-embed-row" }, embed, pullBtn))),
    pullBox,
    el("label", { class: "asst-toggle" }, strict, el("span", {}, t("asst_strict"))),
    isNew ? saveFirst : knowledge);
  body.addEventListener("models", () => { fillModels(); renderEmbedState(); });

  // The assistant's name is user text: isolate it so "Edit: <Arabic name>" keeps its order.
  const title = isNew ? t("asst_editorNew") : el("span", { dir: document.documentElement.dir || "rtl" }, withName("asst_editorEdit", a.name));
  const m = openModal({
    title,
    className: "asst-modal",
    body,
    actions: [delBtn, footSpacer, cancelBtn, saveBtn].filter(Boolean),
    beforeClose: () => !busy && !pullCtl,
  });
  if (!isNew) m.el.setAttribute("aria-label", t("asst_editorEdit", { name: a.name }));
  if (!isNew) refresh().catch(e => toast(e.message, "error"));
  else renderDocs();
}

/** Small modal to write/edit a note; onSave(name, text). */
function openNoteEditor(note, onSave) {
  const nm = el("input", { class: "input", dir: "auto", placeholder: t("asst_noteNamePh") });
  const tx = el("textarea", { class: "input", rows: 10, dir: "auto", placeholder: t("asst_noteTextPh") });
  nm.value = note?.name || "";
  tx.value = note?.text || "";
  const ok = el("button", { class: "btn primary", type: "button" }, t("asst_save"));
  const cancel = el("button", { class: "btn", type: "button", onclick: () => m.close() }, t("cancel"));
  ok.addEventListener("click", () => {
    const n = nm.value.trim(), txt = tx.value.trim();
    if (!n || !txt) { toast(t("asst_nameRequired"), "error"); return; }
    m.close(true);
    onSave(n, txt);
  });
  const m = openModal({
    title: note ? t("asst_editNote") : t("asst_addNote"),
    body: el("div", { class: "form" }, field(t("asst_noteName"), nm), field(t("asst_noteText"), tx)),
    actions: [cancel, ok],
  });
}

function setBar(bar, pct) {
  bar.firstElementChild.style.inlineSize = `${Math.max(0, Math.min(100, pct)).toFixed(1)}%`;
}

export default { name: "assistants", init: initAssistants, start: loadAssistants };
