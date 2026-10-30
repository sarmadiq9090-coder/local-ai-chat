// chat.js — the conversation view and composer:
// message rendering (user bubbles, full-width assistant replies, hover actions),
// sending + NDJSON streaming (rAF-throttled painting, stop, tokens/s),
// regenerate, edit-and-resend, text-file attachments (button + drag & drop),
// empty state with example prompts, smart auto-scroll and a scroll-to-bottom button.
// Esc: stops a running generation, else leaves edit mode.
// Events: emits "ollama-maybe-down" when a reply fails because Ollama can't be reached
// (the status feature re-checks and updates the status line).
// Cross-feature deps: sidebar (persistConversation, titleFrom), models (supportsThinking),
// exporter (fenceFor), assistants (prepareAssistantTurn, sourcesEl: RAG turns + sources list).

import { state, on, emit } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, el, iconButton, copyText, toast, openModal } from "../../core/dom.js";
import { renderMarkdown } from "../../core/markdown.js";
import { persistConversation, titleFrom } from "../sidebar/sidebar.js";
import { supportsThinking } from "../models/models.js";
import { fenceFor } from "../exporter/exporter.js";
import { prepareAssistantTurn, sourcesEl } from "../assistants/assistants.js";
import { icon } from "../../core/icons.js";
import { t, registerStrings } from "../../core/i18n.js";
import strings from "./strings.js";

registerStrings("chat", strings);

const MAX_FILE_BYTES = 200 * 1024;
const STICK_PX = 120;      // "near bottom" threshold for auto-scroll
const MAX_INPUT_PX = 240;  // composer auto-grow limit

let listEl, msgsEl, inputEl, sendBtn, chipsEl, boxEl;
let stickToBottom = true;
let relabelAfterStream = false; // language changed mid-stream: re-render once the reply ends

// ---------- pure helpers ----------

/** User text + attachments in the form sent to the model. */
export function withAttachments(content, attachments) {
  let s = content || "";
  for (const a of attachments || []) {
    const f = fenceFor(a.content);
    s += `\n\n--- ${t("fileHeader")}: ${a.name} ---\n${f}\n${a.content}\n${f}`;
  }
  return s;
}

/** Stored messages → Ollama chat messages (system prompt first). */
export function buildChatMessages(messages, systemPrompt) {
  const out = [];
  if (systemPrompt && systemPrompt.trim()) out.push({ role: "system", content: systemPrompt });
  for (const m of messages) {
    if (m.role === "assistant" && !m.content) continue; // failed/empty replies
    out.push({ role: m.role, content: m.role === "user" ? withAttachments(m.content, m.attachments) : m.content });
  }
  return out;
}

/** Human-readable size: bytes below 1 KB, else KB with one decimal. Pure. */
export const formatSize = bytes => (bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`);

/**
 * Decode a text file's bytes. UTF-8 first; if that gives many U+FFFD and the bytes look like
 * Windows ANSI Arabic (Windows-1256, common for files saved by older Arabic Windows tools),
 * decode as Windows-1256 instead. Returns null for binary / undecodable content. Pure.
 */
export function decodeText(bytes) {
  const utf8 = new TextDecoder("utf-8").decode(bytes);
  if (utf8.includes("\u0000")) return null;
  const tooMany = s => (s.match(/�/g) || []).length > Math.max(8, s.length / 100);
  if (!tooMany(utf8)) return utf8.replace(/^﻿/, "");
  let ansi;
  try { ansi = new TextDecoder("windows-1256").decode(bytes); } catch { return null; }
  const high = bytes.reduce((n, b) => n + (b >= 0x80 ? 1 : 0), 0);
  const arabic = (ansi.match(/[؀-ۿ]/g) || []).length;
  return high && arabic >= high * 0.5 && !tooMany(ansi) ? ansi : null;
}

/** Friendly text for a failed reply; also signals the status feature when Ollama looks down. */
function replyError(e, model) {
  const msg = String(e?.message || e || "");
  if (e?.code === "ollama_unreachable" || e?.status === 502 ||
      /ollama_unreachable|not reachable|غير متاح|failed to fetch|network ?error|connection refused/i.test(msg)) {
    emit("ollama-maybe-down");
    return t("errOllamaDown");
  }
  if (/does not support (chat|generate)/i.test(msg)) return t("errNoChatSupport", { model: model || "" });
  return t("errorPrefix") + msg;
}

const newId = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 8);

function createConversation() {
  const now = Date.now();
  return { id: newId(), title: "", model: state.model, created_at: now, updated_at: now, messages: [] };
}

// ---------- init ----------

export function initChat() {
  listEl = $("#list");
  msgsEl = $("#msgs");
  inputEl = $("#input");
  sendBtn = $("#send");
  chipsEl = $("#chips");
  boxEl = $("#box");

  inputEl.addEventListener("input", () => { autoGrow(); updateSendButton(); });
  inputEl.addEventListener("keydown", e => {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); if (!state.streaming) send(); }
  });
  sendBtn.addEventListener("click", () => (state.streaming ? stopGeneration() : send()));

  $("#attachBtn").addEventListener("click", () => $("#fileInput").click());
  $("#fileInput").addEventListener("change", e => { addFiles(e.target.files); e.target.value = ""; });
  $("#cancelEdit").addEventListener("click", () => cancelEdit(true));
  $("#toBottom").addEventListener("click", () => { stickToBottom = true; scrollToBottom(true, true); });

  msgsEl.addEventListener("scroll", () => {
    stickToBottom = distanceFromBottom() < STICK_PX;
    updateToBottomButton();
  }, { passive: true });

  setupDragAndDrop();
  renderExamples();

  on("current", () => { cancelEdit(false); renderConversation(); });
  on("current-title", renderTitle);
  on("streaming", updateSendButton);
  on("focus-composer", () => inputEl.focus());
  on("language", () => {
    renderExamples();
    updateSendButton();
    // Re-render messages (code-block buttons, action tooltips) unless a reply is streaming.
    if (state.streaming) { relabelAfterStream = true; renderTitle(); }
    else rerenderKeepingScroll();
  });

  renderConversation();
  updateSendButton();
}

// ---------- rendering ----------

function renderTitle() {
  $("#convTitle").textContent = state.current?.title || "";
  document.title = state.current?.title ? `${state.current.title} — ${t("appTitle")}` : t("appTitle");
}

/** Full re-render of the current conversation (or the empty state). */
export function renderConversation() {
  const conv = state.current;
  const empty = !conv || !conv.messages.length;
  $("#main").classList.toggle("is-empty", empty);
  renderTitle();
  if (empty) { listEl.replaceChildren(); updateToBottomButton(); return; }
  listEl.replaceChildren(...conv.messages.map((m, i) => messageEl(m, i, conv)));
  stickToBottom = true;
  scrollToBottom(true);
}

/** Re-render the conversation (e.g. new UI language) without jumping the scroll position. */
function rerenderKeepingScroll() {
  const top = msgsEl.scrollTop;
  renderConversation();
  msgsEl.scrollTop = top;
}

function renderExamples() {
  const box = $("#examples");
  box.replaceChildren(...t("examples").map(ex => el("button", {
    type: "button",
    class: "example",
    onclick: () => { if (!state.streaming) send(ex.text); },
  }, el("span", { class: "ex-title" }, ex.title), el("span", { class: "ex-text", dir: "auto" }, ex.text))));
}

function attachmentChip(a, onRemove) {
  return el("span", { class: "chip", title: a.name },
    el("span", { class: "chip-icon", html: icon("file", 14) }),
    el("button", { type: "button", class: "chip-name", dir: "auto", onclick: () => previewAttachment(a) }, a.name),
    el("span", { class: "chip-size", dir: "ltr" }, formatSize(new Blob([a.content]).size)),
    onRemove && iconButton("x", t("removeAttachment"), onRemove, "chip-x"));
}

function previewAttachment(a) {
  openModal({
    title: a.name,
    className: "wide",
    body: el("div", { class: "codebox", dir: "ltr" }, el("pre", {}, el("code", {}, a.content))),
  });
}

/** Build one message element. `streaming` omits the action row. */
function messageEl(m, index, conv, streaming = false) {
  if (m.role === "user") {
    const node = el("div", { class: "msg user", dataset: { index } },
      m.attachments?.length ? el("div", { class: "msg-chips" }, m.attachments.map(a => attachmentChip(a))) : null,
      m.content ? el("div", { class: "bubble", dir: "auto" }, m.content) : null,
      el("div", { class: "actions" },
        iconButton("copy", t("copy"), e => copyWithFeedback(e.currentTarget, withAttachments(m.content, m.attachments))),
        iconButton("edit", t("edit"), () => startEdit(index))));
    return node;
  }
  const body = el("div", { class: "body" });
  fillAssistant(body, m, streaming);
  const isLast = index === conv.messages.length - 1;
  return el("div", { class: "msg ai", dataset: { index } },
    body,
    streaming ? null : sourcesEl(m.sources), // assistants: collapsible "Sources" list
    streaming ? null : el("div", { class: "actions" },
      m.content ? iconButton("copy", t("copy"), e => copyWithFeedback(e.currentTarget, m.content)) : null,
      isLast ? iconButton("refresh", t("regenerate"), regenerate) : null,
      el("span", { class: "meta", dir: "ltr" }, [m.model, m.stats].filter(Boolean).join(" · ")),
      m.stopped ? el("span", { class: "meta stopped" }, t("stopped")) : null));
}

/**
 * (Re)fill an assistant body. The <details> thinking box is kept between
 * frames so the user's open/closed choice survives streaming updates.
 */
function fillAssistant(body, m, streaming) {
  let think = body.querySelector(":scope > details.think");
  if (m.thinking) {
    if (!think) {
      think = el("details", { class: "think" },
        el("summary", {}, el("span", { class: "think-ic", html: icon("bulb", 15) }), el("span", { class: "think-label" })),
        el("div", { class: "think-text", dir: "auto" }));
      think.open = !!streaming;
      body.prepend(think);
    }
    think.querySelector(".think-label").textContent = streaming && !m.content ? t("thinkingLive") : t("thinking");
    think.querySelector(".think-text").textContent = m.thinking;
    if (streaming && m.content && !think.dataset.autoClosed) { think.open = false; think.dataset.autoClosed = "1"; }
  } else if (think) {
    think.remove();
  }

  let content = body.querySelector(":scope > .content");
  if (!content) { content = el("div", { class: "content" }); body.append(content); }
  content.innerHTML = renderMarkdown(m.content || ""); // renderMarkdown escapes all text
  if (m.error) content.append(el("p", { class: "errmsg", dir: "auto" }, m.error));
  if (streaming) appendCursor(content, !m.content && !m.thinking);
}

/** Put the blinking cursor at the end of the last rendered block. */
function appendCursor(content, waiting) {
  let target = content.lastElementChild;
  if (target?.classList.contains("codebox")) target = target.querySelector("code");
  else {
    while (target?.lastElementChild && /^(UL|OL|LI|BLOCKQUOTE|DIV|TABLE|TBODY|TR)$/.test(target.tagName)) {
      target = target.lastElementChild;
    }
  }
  (target || content).append(el("span", { class: waiting ? "cursor waiting" : "cursor" }));
}

async function copyWithFeedback(btn, text) {
  const ok = await copyText(text);
  if (!ok) { toast(t("copyFailed"), "error"); return; }
  btn.innerHTML = icon("check");
  btn.classList.add("done");
  setTimeout(() => { btn.innerHTML = icon("copy"); btn.classList.remove("done"); }, 1400);
}

// ---------- scrolling ----------

const distanceFromBottom = () => msgsEl.scrollHeight - msgsEl.scrollTop - msgsEl.clientHeight;

function scrollToBottom(force = false, smooth = false) {
  if (force || stickToBottom) msgsEl.scrollTo({ top: msgsEl.scrollHeight, behavior: smooth ? "smooth" : "auto" });
  updateToBottomButton();
}

function updateToBottomButton() {
  const show = !$("#main").classList.contains("is-empty") && distanceFromBottom() > STICK_PX * 2;
  $("#toBottom").classList.toggle("show", show);
}

// ---------- composer ----------

function autoGrow() {
  inputEl.style.height = "auto";
  inputEl.style.height = Math.min(inputEl.scrollHeight, MAX_INPUT_PX) + "px";
}

function updateSendButton() {
  const streaming = state.streaming;
  sendBtn.classList.toggle("stop", streaming);
  sendBtn.innerHTML = icon(streaming ? "stop" : "arrowUp", 18);
  sendBtn.title = streaming ? t("stop") : t("send");
  sendBtn.setAttribute("aria-label", sendBtn.title);
  sendBtn.disabled = !streaming && !inputEl.value.trim() && !state.pendingAttachments.length;
  document.body.classList.toggle("streaming", streaming);
}

function renderChips() {
  chipsEl.replaceChildren(...state.pendingAttachments.map((a, i) =>
    attachmentChip(a, () => { state.pendingAttachments.splice(i, 1); renderChips(); updateSendButton(); })));
  chipsEl.hidden = !state.pendingAttachments.length;
}

function setComposer(text, attachments = []) {
  inputEl.value = text;
  state.pendingAttachments = attachments.map(a => ({ ...a }));
  renderChips();
  autoGrow();
  updateSendButton();
}

// ---------- attachments ----------

const readBytes = file => file.arrayBuffer().then(b => new Uint8Array(b));

/** Read dropped/picked files as text (≤200 KB, rejects binary). */
export async function addFiles(fileList) {
  for (const file of fileList || []) {
    if (file.size > MAX_FILE_BYTES) { toast(t("fileTooBig", { name: file.name, limit: MAX_FILE_BYTES / 1024 }), "error"); continue; }
    try {
      const text = decodeText(await readBytes(file));
      if (text == null) { toast(t("fileNotText", { name: file.name }), "error", 6000); continue; }
      state.pendingAttachments.push({ name: file.name, content: text });
    } catch {
      toast(t("fileReadFailed", { name: file.name }), "error");
    }
  }
  renderChips();
  updateSendButton();
  inputEl.focus();
}

function setupDragAndDrop() {
  // Never let a stray drop navigate the WebView away from the app.
  window.addEventListener("dragover", e => e.preventDefault());
  window.addEventListener("drop", e => e.preventDefault());
  let depth = 0;
  const hasFiles = e => [...(e.dataTransfer?.types || [])].includes("Files");
  boxEl.addEventListener("dragenter", e => { if (!hasFiles(e)) return; depth++; boxEl.classList.add("drag"); });
  boxEl.addEventListener("dragleave", () => { if (--depth <= 0) { depth = 0; boxEl.classList.remove("drag"); } });
  boxEl.addEventListener("drop", e => {
    e.preventDefault();
    depth = 0;
    boxEl.classList.remove("drag");
    if (e.dataTransfer?.files?.length) addFiles(e.dataTransfer.files);
  });
}

// ---------- edit ----------

function startEdit(index) {
  if (state.streaming) { toast(t("busy")); return; }
  const m = state.current?.messages[index];
  if (!m) return;
  state.editIndex = index;
  setComposer(m.content || "", m.attachments || []);
  $("#editBar").hidden = false;
  listEl.querySelectorAll(".msg.editing").forEach(n => n.classList.remove("editing"));
  listEl.querySelector(`.msg[data-index="${index}"]`)?.classList.add("editing");
  inputEl.focus();
}

/** Leave edit mode; `clear` also empties the composer. */
export function cancelEdit(clear) {
  if (state.editIndex == null) return;
  state.editIndex = null;
  $("#editBar").hidden = true;
  listEl.querySelectorAll(".msg.editing").forEach(n => n.classList.remove("editing"));
  if (clear) setComposer("");
}

// ---------- send / generate ----------

/** Send the composer contents (or `presetText`, e.g. an example prompt). */
export async function send(presetText) {
  if (state.streaming) return;
  const text = (presetText ?? inputEl.value).trim();
  const attachments = presetText != null ? [] : state.pendingAttachments;
  if (!text && !attachments.length) return;
  if (!state.model) { toast(t("noModelSelected"), "error"); return; }

  let conv = state.current;
  const isNew = !conv;
  if (isNew) conv = createConversation();
  if (isNew && state.assistantId) conv.assistant_id = state.assistantId; // assistants: bind the new chat
  const editing = isNew ? null : state.editIndex;
  if (editing != null) conv.messages = conv.messages.slice(0, editing);
  state.editIndex = null;
  $("#editBar").hidden = true;

  // Editing the first message of a chat whose title came from it (not renamed): retitle it.
  if (editing === 0 && !conv.title_custom) conv.title = "";
  const userMsg = { role: "user", content: text };
  if (attachments.length) userMsg.attachments = attachments.map(a => ({ name: a.name, content: a.content }));
  conv.messages.push(userMsg);
  if (!conv.title) conv.title = titleFrom(text || attachments[0]?.name) || t("untitled");

  setComposer("");
  if (isNew) { state.current = conv; emit("current"); } // also renders
  else renderConversation();
  persistConversation(conv);
  await generate(conv);
}

/** Drop the last assistant reply and generate it again. */
export async function regenerate() {
  const conv = state.current;
  if (state.streaming || !conv) return;
  if (!state.model) { toast(t("noModelSelected"), "error"); return; }
  if (conv.messages.at(-1)?.role === "assistant") conv.messages.pop();
  if (!conv.messages.length) return;
  listEl.lastElementChild?.classList.contains("ai") && listEl.lastElementChild.remove();
  await generate(conv);
}

/** Abort the running generation (no-op if idle). Returns true if it stopped one. */
export function stopGeneration() {
  if (!state.controller) return false;
  state.controller.abort();
  return true;
}

async function generate(conv) {
  const model = state.model;
  const s = state.settings || {};
  const payload = {
    model,
    messages: buildChatMessages(conv.messages, s.system_prompt),
    options: { temperature: Number(s.temperature ?? 0.7), num_ctx: Number(s.num_ctx ?? 8192) },
  };
  if (supportsThinking(model)) payload.think = !!s.think;

  const ai = { role: "assistant", model, content: "", thinking: "" };
  conv.messages.push(ai);
  conv.model = model;
  const index = conv.messages.length - 1;
  const aiEl = messageEl(ai, index, conv, true);
  const body = aiEl.querySelector(".body");
  listEl.append(aiEl);
  stickToBottom = true;
  scrollToBottom(true);

  const controller = new AbortController();
  state.controller = controller;
  state.streaming = true;
  emit("streaming", true);

  // rAF-throttled painting; `finished` guarantees no late frame re-adds the cursor.
  let frame = 0;
  let finished = false;
  const paint = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (finished) return;
      fillAssistant(body, ai, true);
      scrollToBottom();
    });
  };

  try {
    // Assistants (RAG): retrieve knowledge and use the assistant's system prompt/model (features/assistants).
    const rag = conv.assistant_id ? await prepareAssistantTurn(conv, controller.signal) : null;
    if (rag) {
      payload.messages = buildChatMessages(conv.messages, rag.system); // the empty `ai` is skipped
      if (rag.model) {
        payload.model = ai.model = conv.model = rag.model;
        payload.think = supportsThinking(rag.model) ? !!s.think : undefined;
      }
      if (rag.sources.length) ai.sources = rag.sources;
    }
    for await (const chunk of api.chatStream(payload, controller.signal)) {
      if (chunk.message) {
        ai.content += chunk.message.content || "";
        ai.thinking += chunk.message.thinking || "";
      }
      if (chunk.done && chunk.eval_count && chunk.eval_duration) {
        ai.stats = `${(chunk.eval_count / (chunk.eval_duration / 1e9)).toFixed(1)} tokens/s`;
      }
      paint();
    }
  } catch (e) {
    if (e.name === "AbortError") ai.stopped = true;
    else ai.error = replyError(e, ai.model);
  } finally {
    finished = true;
    if (frame) cancelAnimationFrame(frame);
    if (!ai.thinking) delete ai.thinking;
    state.controller = null;
    state.streaming = false;
    emit("streaming", false);
    if (relabelAfterStream) {
      // The language changed while streaming: older messages still carry the old
      // labels (Copy/Run, tooltips), so re-render everything now that the reply is done.
      relabelAfterStream = false;
      const follow = stickToBottom;
      rerenderKeepingScroll();
      if (follow) scrollToBottom(true);
    } else if (state.current === conv && aiEl.isConnected) {
      aiEl.replaceWith(messageEl(ai, index, conv));
      scrollToBottom();
    }
    await persistConversation(conv);
  }
}

/** Esc: stop generation first; otherwise leave edit mode (clearing the composer). */
function onEscape() {
  if (stopGeneration()) return true;
  if (state.editIndex == null) return false;
  cancelEdit(true);
  return true;
}

export default { name: "chat", init: initChat, onEscape };
