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
// - Deep understanding (v2.2, per-assistant `deep`): /context does query rewriting, digests,
//   neighbour expansion and the token budget; prepareAssistantTurn then also returns `num_ctx`
//   and `think` for chat.js. The editor has the deep toggle + advanced options, instruction
//   templates, a per-document "what did it read?" dialog (extracted text, digest view/edit/
//   regenerate) and a test panel (retrieval + answer without saving a conversation).
// - Embedding model picker: catalog of Ollama embedding models (embed-catalog.js) with size, Arabic
//   support, a verdict for this PC (models/catalog.js rateModel on embed + chat model size) and an
//   "installed" marker; "Other…" allows any tag; download with progress; re-index offer on change.
// Cross-feature deps: sidebar (closeMobileSidebar), models (loadModels, fmtSize, modelInstalled, loadCaps,
//   supportsThinking; catalog.js rateModel via embed-catalog.js; its strings rateShort_*, arabic_*,
//   suggestSize, suggestArabic), chat (send: suggested questions).
// Events: listens to "current", "models", "language"; emits "current", "focus-composer", "assistants".

import { state, on, emit } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, el, iconButton, toast, openModal, confirmDialog } from "../../core/dom.js";
import { icon } from "../../core/icons.js";
import { t, registerStrings, getLanguage, formatNumber, localizeDigits, parseNumber } from "../../core/i18n.js";
import { closeMobileSidebar } from "../sidebar/sidebar.js";
import { loadModels, modelInstalled, loadCaps, supportsThinking, fmtSize } from "../models/models.js";
import { send } from "../chat/chat.js";
import strings, { NOT_IN_KNOWLEDGE, REPLY_LANGUAGE } from "./strings.js";
import { EMBED_CATALOG, findEmbed, describeEmbed, rateEmbed, DEFAULT_EMBED, arabicKey } from "./embed-catalog.js";

registerStrings("assistants", strings);

const BASE = "/api/app/assistants";
// Same list as the server's `extract::is_supported` (src/assistants/extract.rs).
const FILE_EXTS = ["txt", "md", "csv", "json", "xml", "yml", "yaml", "toml", "html", "css", "js", "ts", "py",
  "rs", "go", "java", "c", "cpp", "h", "cs", "php", "rb", "sh", "ps1", "sql", "ini", "log", "pdf"];
const FILE_ACCEPT = FILE_EXTS.map(e => "." + e).join(",");
const SNIPPET_CHARS = 280;
/** A last question shorter than this is a follow-up: the previous turn joins the search query. */
const SHORT_QUERY = 60;

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
      toast(plural("asst_corrupt", corrupt.split(",").length), "error", 9000);
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

/**
 * System message: instructions + strict/soft rules + numbered sources (or an explicit "no sources").
 * `deep`: the deep-understanding rules (read everything, reconcile, complete answers, exact numbers);
 * passages with `kind: "digest"` are labelled as the document's digest.
 */
export function buildAssistantSystem(a, results, question, deep = false) {
  const lang = detectLang(question);
  const sentence = NOT_IN_KNOWLEDGE[lang];
  // Last, so it wins over the language of earlier turns in a mixed-language chat.
  const replyIn = REPLY_LANGUAGE[lang];
  if (!results.length) {
    // No passage passed the threshold: say so plainly and forbid citations, so the model
    // doesn't answer from earlier turns and cite "[1]".
    const rules = a.strict ? t("asst_strictNoSources", { sentence }) : t("asst_softNoSources");
    return [(a.instructions || "").trim(), `## ${t("asst_rulesHead")}\n${rules}`, replyIn].filter(Boolean).join("\n\n");
  }
  const rules = deep
    ? (a.strict ? t("asst_deepStrictRules", { sentence }) : t("asst_deepSoftRules"))
    : (a.strict ? t("asst_strictRules", { sentence }) : t("asst_softRules"));
  const label = r => (r.kind === "digest" ? `${r.doc_name} — ${t("asst_srcDigest")}` : r.doc_name);
  const sources = results.map((r, i) => `[${t("asst_srcLabel")} ${i + 1}: ${label(r)}]\n${r.text}`).join("\n\n");
  return [
    (a.instructions || "").trim(),
    `## ${t("asst_rulesHead")}\n${rules}`,
    `## ${t("asst_sourcesHead")}\n${sources}`,
    replyIn,
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
 * follow-up ("and used products?", «وبدل السكن؟»), the previous user message is
 * added so the search knows what it refers to. prepareAssistantTurn also
 * searches the bare message, so a wrongly detected follow-up only adds candidates.
 */
export function retrievalQuery(messages) {
  const msgs = (messages || []).filter(m => m.role !== "assistant" || m.content);
  let li = -1;
  for (let i = msgs.length - 1; i >= 0; i--) if (msgs[i].role === "user" && msgs[i].content?.trim()) { li = i; break; }
  if (li < 0) return "";
  const last = msgs[li].content.trim();
  if (!isFollowUp(last)) return last;
  // Only the previous question is added: the previous *reply* dominated the query embedding
  // and pulled results back to the old topic (e.g. «و الضمان؟» after a battery answer).
  let prevUser = "";
  for (let i = li - 1; i >= 0; i--) {
    const m = msgs[i];
    if (m.role === "user" && m.content?.trim()) { prevUser = m.content.trim(); break; }
  }
  return [prevUser, last].filter(Boolean).join("\n");
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

/** Deep understanding: retrieval for one turn on the server (rewrite + digests + expansion + budget). */
async function deepContext(a, messages, model, signal) {
  const turns = (messages || [])
    .filter(m => (m.role === "user" || m.role === "assistant") && m.content?.trim())
    .map(m => ({ role: m.role, content: m.content }));
  return call("POST", `/${enc(a.id)}/context`, { messages: turns, model: model || state.model || null }, signal);
}

const snippetOf = text => (text.length > SNIPPET_CHARS ? text.slice(0, SNIPPET_CHARS) + "…" : text);

/**
 * Retrieve knowledge for the latest question of `messages` and build the turn:
 * { system, model, sources, num_ctx, think, debug }. `num_ctx`/`think` are null when the
 * app settings apply. `debug` (for the test panel): { query, rewritten, rewrite, passages,
 * budget, used, timings }. Throws AbortError if `signal` aborts.
 */
export async function buildTurn(a, messages, signal) {
  const lastUser = [...messages].reverse().find(m => m.role === "user");
  const question = lastUser?.content || "";
  // The assistant's preferred model, if installed (":latest" aware); its capabilities are
  // fetched now so chat.js can check supportsThinking() for it.
  const model = a.model && modelInstalled(a.model) ? a.model : null;
  if (model) await loadCaps(model);
  let results = [];
  let debug = { query: question, rewritten: null, rewrite: "skipped", passages: [], budget: 0, used: 0, timings: null };
  let numCtx = a.num_ctx || null;
  let think = typeof a.think === "boolean" ? a.think : null;
  const retrieve = async () => {
    if (a.deep) {
      const ctx = await deepContext(a, messages, model, signal);
      results = ctx.passages || [];
      numCtx = ctx.num_ctx || numCtx;
      think = ctx.think;
      debug = { query: ctx.query, rewritten: ctx.rewritten, rewrite: ctx.rewrite, passages: results,
        budget: ctx.budget_tokens, used: ctx.used_tokens, timings: ctx.timings, num_ctx: ctx.num_ctx };
    } else {
      const query = retrievalQuery(messages);
      if (!query.trim()) return;
      results = await searchKnowledge(a, query, question, signal);
      debug = { ...debug, rewritten: query !== question.trim() ? query : null, passages: results };
    }
  };
  if (question.trim()) {
    try {
      await retrieve();
    } catch (e) {
      if (e.name === "AbortError") throw e;
      if (e.code === "reindex_required") {
        // Index from an older version / another embedding model: offer to rebuild it now.
        const yes = await confirmDialog(withName("asst_reindexAsk", a.name), { title: t("asst_reindex"), okText: t("asst_reindex") });
        if (yes && await reindexFromChat(a, signal)) {
          try { await retrieve(); }
          catch (e2) { if (e2.name === "AbortError") throw e2; toast(t("asst_searchFailed", { error: e2.message }), "error", 7000); }
        }
      } else {
        toast(e.missingModel ? t("asst_modelMissing", { name: e.missingModel }) : t("asst_searchFailed", { error: e.message }), "error", 7000);
      }
    }
  }
  return {
    system: buildAssistantSystem(a, results, question, !!a.deep),
    model,
    num_ctx: numCtx,
    think,
    debug,
    sources: results.map((r, i) => ({
      n: i + 1,
      doc_name: r.doc_name,
      snippet: snippetOf(r.text),
      score: Math.round(r.score * 1000) / 1000,
      ...(r.kind === "digest" ? { kind: "digest" } : {}),
    })),
  };
}

/**
 * Retrieve passages for the latest question of `conv` and build the turn.
 * Returns { system, model, sources, num_ctx, think } or null (assistant gone → normal chat).
 * chat.js applies `num_ctx` (options.num_ctx) and `think` when they aren't null.
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
  const turn = await buildTurn(a, conv.messages, signal);
  delete turn.debug;
  return turn;
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
        s.kind === "digest" ? el("span", { class: "asst-badge" }, t("asst_kindDigest")) : null,
        el("span", { class: "asst-src-score", dir: "ltr" }, localizeDigits(Number(s.score).toFixed(2)))),
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


/** Default num_ctx of a deep-mode assistant (server: deep::DEEP_NUM_CTX). */
export const DEEP_NUM_CTX = 16384;
/** Above this, an 8B model may not fit an 8 GB GPU (the editor warns). */
export const COMFORT_NUM_CTX = 16384;
/** Allowed num_ctx range (server: deep::MIN_NUM_CTX / MAX_NUM_CTX). */
const MIN_NUM_CTX = 2048;
const MAX_NUM_CTX = 131072;
const TEXT_PAGE_CHARS = 20000;

let templatesCache = null;
/** Instruction templates from the server (`[{id, icon, name, description, instructions}]`). */
async function loadTemplates() {
  if (!templatesCache) {
    try { templatesCache = await call("GET", "/templates"); } catch { templatesCache = null; return []; }
  }
  return templatesCache;
}

/** Text of a {ar, en} pair in the UI language. */
const pick = pair => pair?.[getLanguage()] || pair?.ar || "";

/** Docs that need a digest (none yet, or the last attempt failed). */
const missingDigests = a => (a?.docs || []).filter(d => !d.digest || d.digest.status === "failed");

/** Digest badge text + class of a document. */
function digestBadge(d) {
  if (!d.digest) return [t("asst_digestNone"), "none"];
  if (d.digest.status === "failed") return [t("asst_digestFailed"), "failed"];
  if (d.digest.status === "edited") return [t("asst_digestEdited"), "ok"];
  if (d.digest.status === "short") return [t("asst_digestShort"), "none"];
  return [t("asst_digestOk"), "ok"];
}

/** Open the editor for `a` (an Assistant) or for a new assistant (null). */
export function openEditor(a) {
  const isNew = !a;
  let current = a ? { ...a } : null;
  let busy = false;
  let opCtl = null; // AbortController of the running upload/reindex/digest stream

  const name = el("input", { class: "input", dir: "auto", placeholder: t("asst_namePh"), maxlength: 80 });
  const iconIn = el("input", { class: "input asst-icon-input", maxlength: 8 });
  const desc = el("input", { class: "input", dir: "auto", placeholder: t("asst_descriptionPh") });
  const instr = el("textarea", { class: "input", rows: 5, dir: "auto", placeholder: t("asst_instructionsPh") });
  const model = el("select", { class: "input", dir: "ltr" });
  const strict = el("input", { type: "checkbox" });
  const embed = el("input", { class: "input", dir: "ltr", spellcheck: false });
  const deepIn = el("input", { type: "checkbox" });
  // Text, not type=number: that drops Arabic-Indic digits (١٢٣); parseNumber reads both.
  const ctxIn = el("input", { class: "input", type: "text", inputmode: "numeric", autocomplete: "off", dir: "ltr" });
  const thinkSel = el("select", { class: "input" },
    el("option", { value: "auto" }, t("asst_thinkAuto")),
    el("option", { value: "on" }, t("asst_thinkOn")),
    el("option", { value: "off" }, t("asst_thinkOff")));
  const ctxWarn = el("div", { class: "asst-warn-line", hidden: true }, el("span", { html: icon("alert", 14) }), el("span", {}));
  const tplSel = el("select", { class: "input" }, el("option", { value: "" }, t("asst_templateNone")));

  name.value = a?.name || "";
  iconIn.value = a?.icon || "🤖";
  desc.value = a?.description || "";
  instr.value = a?.instructions || "";
  strict.checked = a ? !!a.strict : true;
  embed.value = a?.embed_model || "bge-m3";
  deepIn.checked = a ? !!a.deep : true;
  ctxIn.value = a?.num_ctx ? localizeDigits(a.num_ctx) : "";
  thinkSel.value = a?.think === true ? "on" : a?.think === false ? "off" : "auto";

  const autoCtx = () => (deepIn.checked ? DEEP_NUM_CTX : Number(state.settings?.num_ctx || 8192));
  const renderCtx = () => {
    ctxIn.placeholder = t("asst_numCtxAuto", { n: autoCtx() });
    const n = parseNumber(ctxIn.value) || autoCtx();
    ctxWarn.hidden = n <= COMFORT_NUM_CTX;
    ctxWarn.lastElementChild.textContent = t("asst_numCtxWarn", { n: COMFORT_NUM_CTX });
  };
  ctxIn.addEventListener("input", renderCtx);
  // The server clamps num_ctx to [MIN_NUM_CTX, MAX_NUM_CTX]: clamp here too and say so.
  const clampCtx = () => {
    const v = parseNumber(ctxIn.value);
    if (!(v > 0)) { ctxIn.value = ""; renderCtx(); return; }
    const n = Math.min(MAX_NUM_CTX, Math.max(MIN_NUM_CTX, Math.round(v)));
    if (n !== v) {
      ctxIn.value = localizeDigits(n);
      toast(t("asst_numCtxClamped", { min: MIN_NUM_CTX, max: MAX_NUM_CTX, n }), "error", 6000);
    }
    renderCtx();
  };
  ctxIn.addEventListener("change", clampCtx);
  deepIn.addEventListener("change", renderCtx);
  renderCtx();

  loadTemplates().then(list => {
    tplSel.append(...list.map(tp => el("option", { value: tp.id }, `${tp.icon} ${pick(tp.name)}`)));
  });
  tplSel.addEventListener("change", async () => {
    const tp = (templatesCache || []).find(x => x.id === tplSel.value);
    tplSel.value = "";
    if (!tp) return;
    const text = pick(tp.instructions);
    if (instr.value.trim() && instr.value.trim() !== text.trim()) {
      const ok = await confirmDialog(t("asst_templateReplace", { name: pick(tp.name) }), { title: t("asst_template") });
      if (!ok) return;
    }
    instr.value = text;
    // Description and icon follow the template unless the user wrote their own.
    const tpls = templatesCache || [];
    const d0 = desc.value.trim(), i0 = iconIn.value.trim();
    if (!d0 || tpls.some(x => x.description?.ar === d0 || x.description?.en === d0)) desc.value = pick(tp.description);
    if (!i0 || i0 === "🤖" || tpls.some(x => x.icon === i0)) iconIn.value = tp.icon;
    instr.focus();
  });

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

  // --- embedding model picker (catalog + verdict for this PC) ---
  const embedSel = el("select", { class: "input asst-embed-select", dir: "ltr" });
  const embedCard = el("div", { class: "suggest-card asst-embed-card", hidden: true });
  const embedHw = el("div", { class: "muted asst-embed-hw", hidden: true });
  const embedWarn = el("div", { class: "asst-warn-line", hidden: true }, el("span", { html: icon("alert", 14) }), el("span", {}));
  embed.placeholder = t("asst_embedCustomPh");
  let sysInfo = null;
  const sameEmbed = (x, y) => String(x || "").trim().replace(/:latest$/, "") === String(y || "").trim().replace(/:latest$/, "");
  const chatName = () => model.value || state.model || "";
  const chatGb = () => {
    const n = chatName();
    const mm = state.models.find(x => x.name === n || x.name === `${n}:latest`);
    return mm ? Number(mm.size || 0) / 1024 ** 3 : 0;
  };
  const sizeText = gb => (gb < 1 ? `${Math.round(gb * 1000)} MB` : `${gb} GB`); // as ollama.com shows sizes
  const renderEmbedPicker = () => {
    const typed = embed.value.trim();
    const cg = chatGb();
    embedSel.replaceChildren(
      ...EMBED_CATALOG.map(m => {
        const r = rateEmbed(m, sysInfo, cg);
        return el("option", { value: m.name }, [
          m.recommended ? `★ ${m.name}` : m.name,
          sizeText(m.sizeGb),
          `${t("suggestArabic")}: ${t(arabicKey(m))}`,
          r?.short,
          modelInstalled(m.name) ? `✔ ${t("asst_embedInstalled")}` : null,
        ].filter(Boolean).join(" · "));
      }),
      el("option", { value: "" }, t("asst_embedOther")));
    const cat = findEmbed(typed);
    embedSel.value = cat ? cat.name : "";
    if (sysInfo) {
      const gpu = (sysInfo.gpus || []).filter(g => g.vram_gb).map(g => `${g.name} (${g.vram_gb} GB)`).join(" + ") || t("asst_embedNoGpu");
      embedHw.hidden = false;
      embedHw.replaceChildren(t("asst_embedHw", { ram: Math.round(sysInfo.ram_gb || 0) }), " ", el("bdi", { dir: "ltr" }, gpu));
    }
    // Explanation card.
    if (!cat) {
      embedCard.hidden = !typed;
      embedCard.className = "suggest-card asst-embed-card";
      embedCard.replaceChildren(el("p", { class: "suggest-desc" }, t("asst_embedUnknown")));
    } else {
      const r = rateEmbed(cat, sysInfo, cg);
      embedCard.hidden = false;
      embedCard.className = "suggest-card asst-embed-card" + (r ? ` lvl-${r.level}` : "");
      const fact = (label, value) => el("span", { class: "suggest-fact" }, el("span", { class: "muted" }, `${label}: `), el("bdi", {}, value));
      embedCard.replaceChildren(
        el("div", { class: "suggest-head" },
          el("span", { class: "model-name", dir: "ltr" }, cat.name),
          cat.recommended ? el("span", { class: "asst-badge ok" }, t("asst_embedRecommended")) : null,
          r ? el("span", { class: "suggest-badge" }, r.label) : null),
        el("p", { class: "suggest-desc" }, describeEmbed(cat)),
        el("div", { class: "suggest-facts" },
          fact(t("suggestSize"), sizeText(cat.sizeGb)),
          fact(t("suggestArabic"), t(arabicKey(cat))),
          modelInstalled(cat.name) ? el("span", { class: "suggest-fact" }, `✔ ${t("asst_embedInstalled")}`) : null),
        r ? el("p", { class: "suggest-reason" }, r.reason) : null,
        cg ? el("p", { class: "muted asst-embed-note" }, t("asst_embedWithChat", { chat: chatName(), gb: (Math.round(cg * 10) / 10).toString() })) : null);
    }
    // Changing the model of an assistant that has documents needs a full re-index.
    const n = current?.docs?.length || 0;
    embedWarn.hidden = !n || !typed || sameEmbed(typed, current?.embed_model);
    embedWarn.lastElementChild.textContent = t("asst_embedReindexWarn", { n });
  };
  embedSel.addEventListener("change", () => {
    if (embedSel.value) {
      embed.value = embedSel.value;
      embed.hidden = true;
    } else {
      embed.hidden = false;
      if (findEmbed(embed.value)) embed.value = "";
      embed.focus();
    }
    renderEmbedState();
    renderEmbedPicker();
  });
  embed.addEventListener("input", renderEmbedPicker);
  model.addEventListener("change", renderEmbedPicker);
  api.getSystemInfo().then(info => { sysInfo = info; renderEmbedPicker(); }).catch(() => {});
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
          text += ` — ${fmtSize(p.completed)} / ${fmtSize(p.total)} (${localizeDigits(pct.toFixed(0))}%)`;
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
  renderEmbedPicker();
  // The tag box is for "Other…": hidden while a catalog model is picked (typing never hides it).
  embed.hidden = !!findEmbed(embed.value);
  // In the editor the download button is right here: say so instead of "open the editor".
  const missingMsg = m => t("asst_modelMissingHere", { name: m });

  // --- knowledge ---
  const docsEl = el("div", { class: "asst-docs" });
  const progBar = el("div", { class: "asst-bar" }, el("div", { class: "asst-bar-fill" }));
  const progText = el("span", { class: "asst-progress-text muted" });
  const progCancel = el("button", { class: "link-btn", type: "button", onclick: () => opCtl?.abort() }, t("asst_cancel"));
  const progBox = el("div", { class: "asst-progress", hidden: true }, progBar,
    el("div", { class: "asst-progress-row" }, progText, progCancel));
  const staleBtn = el("button", { class: "btn small", type: "button", html: icon("refresh", 14) }, t("asst_reindexNow"));
  const staleEl = el("div", { class: "asst-stale", hidden: true }, el("span", { html: icon("alert", 15) }),
    el("span", { class: "asst-stale-text" }, t("asst_stale")), staleBtn);
  const digestAllBtn = el("button", { class: "btn small", type: "button", html: icon("bulb", 14) }, t("asst_digestAll"));
  const digestText = el("span", { class: "asst-stale-text" });
  const digestRow = el("div", { class: "asst-digest-row", hidden: true }, el("span", { html: icon("bulb", 15) }), digestText, digestAllBtn);
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
    staleEl, digestRow, drop, progBox, docsEl);
  const saveFirst = el("p", { class: "muted asst-save-first" }, t("asst_saveFirst"));

  const setBusy = on => {
    busy = on;
    [addNoteBtn, reindexBtn, staleBtn, digestAllBtn, saveBtn, delBtn].forEach(b => { if (b) b.disabled = on; });
    drop.classList.toggle("disabled", on);
  };
  const showProgress = p => {
    progBox.hidden = false;
    setBar(progBar, p.total ? (p.done / p.total) * 100 : 0);
    const label = t(`asst_status_${p.status}`);
    const status = p.total > 1 ? `${label} ${p.done}/${p.total}` : label;
    progText.textContent = p.name && p.files > 1
      ? t("asst_uploadingFile", { name: p.name, i: p.file, n: p.files, status })
      : status;
  };

  const renderDigestRow = () => {
    const missing = current?.deep ? missingDigests(current).length : 0;
    digestRow.hidden = !missing;
    digestText.textContent = plural("asst_digestMissingN", missing);
  };

  const renderDocs = () => {
    renderDigestRow();
    const docs = current?.docs || [];
    if (!docs.length) { docsEl.replaceChildren(el("div", { class: "asst-empty" }, t("asst_noDocs"))); return; }
    docsEl.replaceChildren(...docs.map(d => {
      const [badge, cls] = digestBadge(d);
      const showBadge = current?.deep || d.digest;
      const meta = [d.kind === "note" ? t("asst_kindNote") : t("asst_kindFile"), plural("asst_chunks", d.chunks)];
      if (d.pages) meta.push(plural("asst_pages", d.pages));
      return el("div", { class: "asst-doc" + (d.warning ? " has-warning" : "") },
        el("span", { class: "asst-doc-ic", html: icon(d.kind === "note" ? "edit" : "file", 16) }),
        el("div", { class: "asst-doc-main" },
          el("span", { class: "asst-doc-name", dir: "auto", title: d.name }, d.name),
          d.warning ? el("span", { class: "asst-doc-warn", title: t(`asst_warn_${d.warning}`) },
            el("span", { html: icon("alert", 13) }), t(`asst_warn_${d.warning}`)) : null),
        showBadge ? el("span", { class: `asst-badge ${cls}`, title: d.digest?.error || "" }, badge) : null,
        el("span", { class: "asst-doc-meta muted" }, meta.join(" · ")),
        iconButton("search", t("asst_viewDoc"), () => openDocViewer(d)),
        d.kind === "note" ? iconButton("edit", t("asst_editNote"), () => editNote(d)) : null,
        iconButton("trash", t("asst_deleteDoc"), () => deleteDoc(d), "icon-btn danger"));
    }));
  };

  const refresh = async () => {
    const fresh = await call("GET", `/${enc(current.id)}`);
    current = fresh;
    staleEl.hidden = !fresh.index?.stale;
    renderDocs();
  };

  /** Handle the final lines of an upload/reindex/digest stream; returns true if all succeeded. */
  const handleFinals = (finals, digestOp = false) => {
    let ok = true;
    for (const f of finals) {
      if (f.error) {
        ok = false;
        if (f.code === "no_text_scanned" || f.code === "no_text") {
          toast(t(`asst_err_${f.code}`, { name: f.name || "" }), "error", 9000);
          continue;
        }
        const msg = f.missing_model ? missingMsg(f.missing_model) : f.error;
        toast(f.name ? t("asst_docFailed", { name: f.name, error: msg }) : t("asst_failed", { error: msg }), "error", 8000);
      } else if (f.doc) {
        if (!digestOp) toast(t("asst_indexed", { name: f.doc.name }), "success");
        if (f.doc.warning) toast(`${f.doc.name}: ${t(`asst_warn_${f.doc.warning}`)}`, "error", 9000);
        if (f.digest_error) toast(t("asst_digestFailedMsg", { name: f.doc.name, error: f.digest_error }), "error", 9000);
      } else if (f.assistant) toast(digestOp ? t("asst_digestsDone") : t("asst_reindexed"), "success");
    }
    return ok;
  };

  /**
   * After a cancelled stream the server still finishes the current step and then records
   * the outcome (e.g. the digest state) and bumps `updated_at`. Wait for that (up to ~8 s)
   * so the badges don't show the state from before the server confirmed.
   */
  const settleAfterCancel = async since => {
    for (let i = 0; i < 16; i++) {
      await new Promise(r => setTimeout(r, 500));
      try {
        const fresh = await call("GET", `/${enc(current.id)}`);
        if ((fresh.updated_at || 0) > since) return;
      } catch { return; }
    }
  };

  const runOp = async (path, init, digestOp = false) => {
    if (busy) { toast(t("asst_busy")); return false; }
    setBusy(true);
    opCtl = new AbortController();
    const since = current?.updated_at || 0;
    let cancelled = false;
    showProgress({ status: digestOp ? "digesting" : "extracting", done: 0, total: 1 });
    try {
      const finals = await streamOp(path, { ...init, signal: opCtl.signal }, showProgress);
      return handleFinals(finals, digestOp);
    } catch (e) {
      if (e.name === "AbortError") { cancelled = true; toast(t("asst_cancelled")); }
      else if (e.code === "no_text_scanned" || e.code === "no_text") toast(t(`asst_err_${e.code}`, { name: "" }), "error", 9000);
      else toast(e.missingModel ? missingMsg(e.missingModel) : t("asst_failed", { error: e.message }), "error", 8000);
      return false;
    } finally {
      opCtl = null;
      progBox.hidden = true;
      if (cancelled) await settleAfterCancel(since);
      setBusy(false);
      try { await refresh(); } catch { /* deleted meanwhile */ }
      loadAssistants();
    }
  };

  /** The model that writes digests when the assistant has none: the app's current one. */
  const uiModel = () => state.model || "";

  const uploadFiles = files => {
    if (!files?.length || !current) return;
    // Unsupported files (e.g. a photo dropped along with documents) are skipped, not fatal.
    const good = files.filter(f => isSupportedFile(f.name));
    const skipped = files.filter(f => !isSupportedFile(f.name)).map(f => f.name);
    if (skipped.length) toast(t("asst_skipped", { names: skipped.join(listSep()) }), "error", 8000);
    if (!good.length) return;
    const fd = new FormData();
    for (const f of good) fd.append("file", f, f.name);
    return runOp(`/${enc(current.id)}/docs?model=${enc(uiModel())}`, { body: fd });
  };
  const addNote = (noteName, text) => runOp(`/${enc(current.id)}/docs?model=${enc(uiModel())}`, {
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name: noteName, text }),
  });
  const reindex = () => runOp(`/${enc(current.id)}/reindex`, {});
  const runDigests = (docId = null, force = false) => {
    if (!current.model && !uiModel()) { toast(t("asst_digestNoModel"), "error", 7000); return false; }
    return runOp(`/${enc(current.id)}/digest`, {
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ doc_id: docId, force, model: uiModel() || null }),
    }, true);
  };

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
  digestAllBtn.addEventListener("click", () => runDigests());

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

  /** "What did it read?": stats, the extracted text (paged) and the digest (view/edit/regenerate). */
  function openDocViewer(d) {
    const base = `/${enc(current.id)}/docs/${enc(d.id)}`;
    const stats = el("div", { class: "asst-viewer-stats muted" });
    const warn = d.warning ? el("div", { class: "asst-warn-line" }, el("span", { html: icon("alert", 14) }),
      el("span", {}, t(`asst_warn_${d.warning}`))) : null;
    const textEl = el("pre", { class: "asst-doc-text", dir: "auto" });
    const pageInfo = el("span", { class: "muted asst-page-info", dir: "auto" });
    const prev = el("button", { class: "btn small", type: "button" }, t("asst_prevPage"));
    const next = el("button", { class: "btn small", type: "button" }, t("asst_nextPage"));
    const textPane = el("div", { class: "asst-viewer-pane" }, textEl, el("div", { class: "asst-pager" }, prev, pageInfo, next));
    let offset = 0, total = 0;
    const loadPage = async off => {
      try {
        const r = await call("GET", `${base}?offset=${off}&limit=${TEXT_PAGE_CHARS}`);
        offset = r.offset; total = r.total_chars;
        textEl.textContent = r.text;
        textEl.scrollTop = 0;
        const pages = r.pages ? plural("asst_pages", r.pages) : t("asst_noPages");
        stats.textContent = [t("asst_docStats", { pages, chars: formatNumber(total), chunks: plural("asst_chunks", r.chunks) }),
          r.digest_chunks ? t("asst_digestChunks", { n: r.digest_chunks }) : null].filter(Boolean).join(" · ");
        pageInfo.textContent = t("asst_pageOf", { from: total ? offset + 1 : 0, to: Math.min(total, offset + TEXT_PAGE_CHARS), total });
        prev.disabled = offset <= 0;
        next.disabled = offset + TEXT_PAGE_CHARS >= total;
      } catch (e) { toast(e.message, "error"); }
    };
    prev.addEventListener("click", () => loadPage(Math.max(0, offset - TEXT_PAGE_CHARS)));
    next.addEventListener("click", () => loadPage(offset + TEXT_PAGE_CHARS));

    // Digest tab.
    const dgInfo = el("div", { class: "muted asst-viewer-stats" });
    const dgText = el("textarea", { class: "input asst-digest-text", rows: 16, dir: "auto" });
    const dgSave = el("button", { class: "btn small primary", type: "button" }, t("asst_digestSave"));
    const dgRegen = el("button", { class: "btn small", type: "button", html: icon("bulb", 14) }, t("asst_digestRegenerate"));
    const dgBar = el("div", { class: "asst-bar" }, el("div", { class: "asst-bar-fill" }));
    const dgProgText = el("span", { class: "asst-progress-text muted" });
    let dgCtl = null;
    const dgCancel = el("button", { class: "link-btn", type: "button", onclick: () => dgCtl?.abort() }, t("asst_cancel"));
    const dgProg = el("div", { class: "asst-progress", hidden: true }, dgBar, el("div", { class: "asst-progress-row" }, dgProgText, dgCancel));
    const digestPane = el("div", { class: "asst-viewer-pane", hidden: true }, dgInfo, dgText, dgProg,
      el("div", { class: "asst-k-actions" }, dgRegen, dgSave));
    const loadDigest = async () => {
      const doc = (current.docs || []).find(x => x.id === d.id) || d;
      let text = "";
      try { text = (await call("GET", `${base}/digest`)).text || ""; } catch { /* none yet */ }
      dgText.value = text;
      const g = doc.digest;
      const chars = formatNumber(g?.chars || 0);
      dgInfo.textContent = !g ? t("asst_digestEmpty")
        : g.status === "failed" ? `${t("asst_digestFailed")}: ${g.error || ""}`
        : g.status === "edited" ? t("asst_digestEditedInfo", { chars })
        : g.status === "short" ? t("asst_digestShortInfo", { chars })
        : t("asst_digestInfo", { model: g.model || "—", chars });
      dgRegen.lastChild.textContent = text ? t("asst_digestRegenerate") : t("asst_digestGenerate");
    };
    dgSave.addEventListener("click", async () => {
      if (busy) { toast(t("asst_busy")); return; }
      dgSave.disabled = true;
      try {
        await call("PUT", `${base}/digest`, { text: dgText.value });
        toast(t("asst_digestSaved"), "success");
        await refresh();
        await loadDigest();
      } catch (e) {
        toast(e.missingModel ? missingMsg(e.missingModel) : t("asst_failed", { error: e.message }), "error", 8000);
      } finally { dgSave.disabled = false; }
    });
    dgRegen.addEventListener("click", async () => {
      if (busy || dgCtl) { toast(t("asst_busy")); return; }
      if (!current.model && !uiModel()) { toast(t("asst_digestNoModel"), "error", 7000); return; }
      dgCtl = new AbortController();
      const since = current?.updated_at || 0;
      let cancelled = false;
      setBusy(true);
      dgRegen.disabled = dgSave.disabled = true;
      dgProg.hidden = false;
      const prog = p => {
        setBar(dgBar, p.total ? (p.done / p.total) * 100 : 0);
        const label = t(`asst_status_${p.status}`);
        dgProgText.textContent = p.total > 1 ? `${label} ${p.done}/${p.total}` : label;
      };
      prog({ status: "digesting", done: 0, total: 1 });
      try {
        const finals = await streamOp(`/${enc(current.id)}/digest`, {
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ doc_id: d.id, force: true, model: uiModel() || null }),
          signal: dgCtl.signal,
        }, prog);
        handleFinals(finals, true);
      } catch (e) {
        if (e.name === "AbortError") { cancelled = true; toast(t("asst_cancelled")); }
        else toast(e.missingModel ? missingMsg(e.missingModel) : t("asst_failed", { error: e.message }), "error", 8000);
      } finally {
        dgCtl = null;
        dgProg.hidden = true;
        if (cancelled) { dgProgText.textContent = ""; await settleAfterCancel(since); }
        dgRegen.disabled = dgSave.disabled = false;
        setBusy(false);
        try { await refresh(); await loadDigest(); } catch { /* deleted meanwhile */ }
        loadAssistants();
      }
    });

    const tabText = el("button", { class: "asst-tab active", type: "button", role: "tab" }, t("asst_docText"));
    const tabDigest = el("button", { class: "asst-tab", type: "button", role: "tab" }, t("asst_docDigest"));
    const show = which => {
      tabText.classList.toggle("active", which === "text");
      tabDigest.classList.toggle("active", which === "digest");
      textPane.hidden = which !== "text";
      digestPane.hidden = which !== "digest";
    };
    tabText.addEventListener("click", () => show("text"));
    tabDigest.addEventListener("click", () => show("digest"));
    const close = el("button", { class: "btn", type: "button", onclick: () => vm.close() }, t("asst_close"));
    const vm = openModal({
      title: el("bdi", {}, d.name),
      className: "asst-modal asst-viewer",
      body: el("div", { class: "form" }, stats, warn, el("div", { class: "asst-tabs", role: "tablist" }, tabText, tabDigest), textPane, digestPane),
      actions: [close],
      beforeClose: () => !dgCtl,
    });
    loadPage(0);
    loadDigest();
  }

  // --- test panel ---
  const testIn = el("textarea", { class: "input", rows: 2, dir: "auto", placeholder: t("asst_testPh") });
  const testAsk = el("button", { class: "btn small primary", type: "button" }, t("asst_testAsk"));
  const testClear = el("button", { class: "btn small", type: "button" }, t("asst_testClear"));
  const testOut = el("div", { class: "asst-test-out" });
  const testBox = el("details", { class: "asst-test" },
    el("summary", {}, t("asst_test")),
    el("p", { class: "muted asst-test-help" }, t("asst_testHelp")),
    testIn, el("div", { class: "asst-k-actions" }, testAsk, testClear), testOut);
  let testHistory = [];
  let testCtl = null;
  const secs = ms => localizeDigits((Number(ms || 0) / 1000).toFixed(1));
  const runTest = async () => {
    const q = testIn.value.trim();
    if (!q || testCtl) return;
    const a2 = current;
    testCtl = new AbortController();
    testAsk.disabled = true;
    const messages = [...testHistory, { role: "user", content: q }];
    const block = el("div", { class: "asst-test-turn" }, el("div", { class: "asst-test-q", dir: "auto" }, q));
    testOut.append(block);
    const status = el("div", { class: "muted asst-test-status" }, t("asst_status_embedding"));
    block.append(status);
    const t0 = performance.now();
    try {
      const turn = await buildTurn(a2, messages, testCtl.signal);
      const dbg = turn.debug;
      const tSearch = performance.now();
      const rw = dbg.rewritten ? el("div", { dir: "auto" }, el("b", {}, `${t("asst_testRewritten")}: `), dbg.rewritten,
        dbg.rewrite === "cached" ? el("span", { class: "muted" }, ` ${t("asst_testRewriteCached")}`) : null)
        : el("div", { class: "muted" }, dbg.rewrite === "failed" ? t("asst_testRewriteFailed") : t("asst_testNoRewrite"));
      const passages = dbg.passages.length
        ? el("ol", { class: "asst-test-passages" }, dbg.passages.map((p, i) => el("li", {},
          el("div", { class: "asst-src-head" },
            el("span", { class: "asst-src-n", dir: "ltr" }, `[${i + 1}]`),
            el("span", { class: "asst-src-name", dir: "auto" }, p.doc_name),
            p.kind === "digest" ? el("span", { class: "asst-badge" }, t("asst_kindDigest")) : null,
            p.expanded ? el("span", { class: "asst-badge" }, t("asst_kindExpanded")) : null,
            el("span", { class: "asst-src-score", dir: "ltr" }, localizeDigits(Number(p.score).toFixed(2)))),
          el("details", {}, el("summary", { class: "asst-src-snippet", dir: "auto" }, snippetOf(p.text).slice(0, 160)),
            el("div", { class: "asst-src-snippet", dir: "auto" }, p.text)))))
        : el("div", { class: "muted" }, t("asst_testNoPassages"));
      const budget = dbg.budget ? el("div", { class: "muted", dir: "auto" }, t("asst_testBudget", { used: dbg.used, budget: dbg.budget, ctx: dbg.num_ctx })) : null;
      // replaceWith() would print a null/undefined node (e.g. no budget when deep mode is off) as "null".
      status.replaceWith(...[
        el("div", { class: "asst-test-sec" }, el("b", {}, `${t("asst_testQuery")}: `), el("span", { dir: "auto" }, dbg.query)), rw,
        el("div", { class: "asst-test-sec" }, el("b", {}, `${t("asst_testPassages")} (${dbg.passages.length})`)), passages, budget,
      ].filter(n => n != null));
      // Answer (streamed, not saved).
      const s = state.settings || {};
      const mdl = turn.model || state.model;
      const payload = {
        model: mdl,
        messages: [{ role: "system", content: turn.system }, ...messages.map(m => ({ role: m.role, content: m.content }))],
        options: { temperature: Number(s.temperature ?? 0.7), num_ctx: Number(turn.num_ctx || s.num_ctx || 8192) },
      };
      if (supportsThinking(mdl)) payload.think = typeof turn.think === "boolean" ? turn.think : !!s.think;
      const think = el("details", { class: "asst-test-think", hidden: true }, el("summary", {}, t("thinking")), el("div", { dir: "auto" }));
      const ans = el("div", { class: "asst-test-answer", dir: "auto" });
      const times = el("div", { class: "muted asst-test-times", dir: "auto" });
      block.append(el("div", { class: "asst-test-sec" }, el("b", {}, t("asst_testAnswer"))), think, ans, times);
      let content = "", thinking = "";
      for await (const chunk of api.chatStream(payload, testCtl.signal)) {
        if (chunk.message) {
          content += chunk.message.content || "";
          thinking += chunk.message.thinking || "";
          ans.textContent = content;
          if (thinking) { think.hidden = false; think.lastElementChild.textContent = thinking; }
        }
      }
      const tEnd = performance.now();
      times.textContent = t("asst_testTimes", { rw: secs(dbg.timings?.rewrite_ms), s: secs(tSearch - t0), a: secs(tEnd - tSearch) });
      testHistory = [...messages, { role: "assistant", content }];
      testIn.value = "";
    } catch (e) {
      if (e.name !== "AbortError") block.append(el("div", { class: "asst-warn-line" }, t("asst_failed", { error: e.message })));
    } finally {
      testCtl = null;
      testAsk.disabled = false;
    }
  };
  testAsk.addEventListener("click", runTest);
  testIn.addEventListener("keydown", e => { if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); runTest(); } });
  testClear.addEventListener("click", () => { testCtl?.abort(); testHistory = []; testOut.replaceChildren(); });

  // --- footer ---
  const saveBtn = el("button", { class: "btn primary", type: "button" }, isNew ? t("asst_create") : t("asst_save"));
  const cancelBtn = el("button", { class: "btn", type: "button", onclick: () => m.close() }, t("cancel"));
  const delBtn = isNew ? null : el("button", { class: "btn asst-del", type: "button", html: icon("trash", 15) }, t("asst_delete"));
  const footSpacer = el("span", { class: "spacer" });

  saveBtn.addEventListener("click", async () => {
    clampCtx();
    const ctxVal = parseNumber(ctxIn.value);
    const body = {
      name: name.value.trim(),
      icon: iconIn.value.trim() || "🤖",
      description: desc.value.trim(),
      instructions: instr.value,
      model: model.value || null,
      strict: strict.checked,
      embed_model: embed.value.trim() || DEFAULT_EMBED,
      deep: deepIn.checked,
      num_ctx: ctxVal > 0 ? Math.round(ctxVal) : null,
      think: thinkSel.value === "auto" ? null : thinkSel.value === "on",
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
      const embedChanged = !sameEmbed(body.embed_model, current.embed_model);
      const deepTurnedOn = body.deep && !current.deep;
      current = { ...current, ...(await call("PUT", `/${enc(current.id)}`, body)) };
      toast(t("asst_saved"), "success");
      await loadAssistants();
      if (embedChanged && current.docs?.length) {
        // The old index can't be searched with another embedding model: offer the re-index now
        // (declining leaves the "re-index" banner).
        const ok = await confirmDialog(t("asst_embedReindexAsk", { n: current.docs.length, name: body.embed_model }),
          { title: t("asst_reindex"), okText: t("asst_reindexNow") });
        if (ok) await reindex();
        else await refresh();
      } else await refresh();
      renderEmbedPicker();
      // Deep understanding just enabled: offer to digest the existing documents now.
      const missing = missingDigests(current).length;
      if (deepTurnedOn && missing && await confirmDialog(t("asst_deepEnableAsk", { n: missing }), { title: t("asst_deep"), okText: t("asst_digestAll") })) {
        await runDigests();
      }
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

  const advanced = el("details", { class: "asst-advanced" },
    el("summary", {}, t("asst_advanced")),
    el("div", { class: "field-row" },
      field(t("asst_numCtx"), ctxIn),
      field(t("asst_think"), thinkSel)),
    ctxWarn);

  const body = el("div", { class: "form asst-editor" },
    el("div", { class: "field-row" },
      el("label", { class: "field asst-icon-field" }, el("span", { class: "field-label" }, t("asst_icon")), iconIn),
      field(t("asst_name"), name)),
    field(t("asst_description"), desc),
    field(t("asst_template"), tplSel),
    field(t("asst_instructions"), instr),
    field(t("asst_model"), model),
    el("div", { class: "field asst-embed-field" },
      el("label", { class: "field-label" }, t("asst_embedModel"), " ", embedState),
      el("div", { class: "asst-embed-row" }, embedSel, pullBtn),
      embed, embedHw, embedCard, embedWarn),
    pullBox,
    el("label", { class: "asst-toggle" }, strict, el("span", {}, t("asst_strict"))),
    el("div", { class: "asst-deep" },
      el("label", { class: "asst-toggle" }, deepIn, el("span", {}, el("b", {}, t("asst_deep")))),
      el("p", { class: "muted asst-deep-help" }, t("asst_deepHelp"))),
    advanced,
    isNew ? saveFirst : knowledge,
    isNew ? null : testBox);
  body.addEventListener("models", () => { fillModels(); renderEmbedState(); renderEmbedPicker(); });

  // The assistant's name is user text: isolate it so "Edit: <Arabic name>" keeps its order.
  const title = isNew ? t("asst_editorNew") : el("span", { dir: document.documentElement.dir || "rtl" }, withName("asst_editorEdit", a.name));
  const m = openModal({
    title,
    className: "asst-modal",
    body,
    actions: [delBtn, footSpacer, cancelBtn, saveBtn].filter(Boolean),
    beforeClose: () => {
      if (busy || pullCtl) return false;
      testCtl?.abort();
      return true;
    },
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
