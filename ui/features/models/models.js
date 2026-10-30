// models.js — everything about Ollama models:
//  * the top-bar model switcher (custom dropdown) + "deep thinking" toggle,
//  * capability lookup via /api/ollama/show (cached in state.caps),
//  * the model manager modal (list / delete / download with progress),
//  * hardware-aware download suggestions (catalog.js + /api/app/system).
// Emits "model" when the selection changes and "models" when the list reloads.
// Embedding-only models (capabilities from /api/ollama/tags, name heuristic as a
// fallback) are never auto-selected nor offered in the chat picker; the manager
// lists them with an "embedding" tag. clearModels() empties/disables the picker
// while Ollama is down (called by the status feature).
// Esc: closes the model dropdown. The installed list is first loaded by the
// status feature once Ollama is reachable.
// Cross-feature deps: settings (saveSettings).

import { state, emit, on } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, el, iconButton, openModal, confirmDialog, toast, setStatus } from "../../core/dom.js";
import { saveSettings } from "../settings/settings.js";
import { icon } from "../../core/icons.js";
import { t, registerStrings } from "../../core/i18n.js";
import { CATALOG, CATEGORIES, LEVELS, rateModel, discreteGpus, describe } from "./catalog.js";
import strings from "./strings.js";

registerStrings("models", strings);

/** Bytes → "4.7" (GB, one decimal). Pure. */
export const toGB = bytes => (Number(bytes || 0) / 1024 ** 3).toFixed(1);

/** Bytes → "4.7 GB", or "45 MB" below 0.1 GB (so small models don't show "0.0 GB"). Pure. */
export function fmtSize(bytes) {
  const n = Number(bytes || 0);
  if (n >= 0.1 * 1024 ** 3) return `${toGB(n)} GB`;
  return `${Math.max(n > 0 ? 1 : 0, Math.round(n / 1024 ** 2))} MB`;
}

const EMBED_NAME = /embed|bge|minilm|nomic|e5-/i; // same heuristic as features/assistants

/**
 * Can `m` (a /api/ollama/tags entry, or a name) chat? Uses `capabilities` when Ollama
 * reports them (embedding-only = has "embedding" but not "completion"), else the name.
 */
export function isChatModel(m) {
  const name = typeof m === "string" ? m : m?.name || "";
  const caps = (typeof m === "object" && Array.isArray(m?.capabilities) && m.capabilities.length)
    ? m.capabilities : state.caps[name];
  if (Array.isArray(caps) && caps.length) return caps.includes("completion") || !caps.includes("embedding");
  return !EMBED_NAME.test(name);
}

/** Installed models that can chat (what the top-bar picker offers). */
export const chatModels = () => state.models.filter(isChatModel);

/** Does the given model support Ollama's `think` option? (needs loadCaps(name) first) */
export const supportsThinking = name => (state.caps[name] || []).includes("thinking");

/** Is `name` installed? A tag-less name also matches its ":latest" tag. */
export const modelInstalled = name => !!name && state.models.some(m => m.name === name || m.name === `${name}:latest`);

/** Fetch and cache (state.caps) the capabilities of `name` once; [] if unknown. */
export async function loadCaps(name) {
  if (name && !(name in state.caps)) {
    try { state.caps[name] = (await api.showModel(name))?.capabilities || []; }
    catch { state.caps[name] = []; }
  }
  return state.caps[name] || [];
}

let menuOpen = false;
let lastModel = null; // selection before clearModels(), restored when Ollama comes back

export function initModels() {
  $("#modelBtn").addEventListener("click", e => { e.stopPropagation(); toggleModelMenu(); });
  document.addEventListener("click", e => { if (menuOpen && !e.target.closest("#modelMenu")) closeModelMenu(); });
  $("#think").addEventListener("change", e => {
    $("#thinkLbl").classList.toggle("on", e.target.checked);
    saveSettings({ think: e.target.checked }).catch(() => {});
  });
  $("#openModels").addEventListener("click", openModelManager);
  renderModelButton();
  updateThinkToggle();
  on("language", () => { closeModelMenu(); renderModelButton(); updateThinkToggle(); });
}

/**
 * Fetch installed models, rebuild the picker and pick the active chat model:
 * the current one, else settings.default_model, else the best-rated installed
 * catalog model, else the first chat-capable one. Never an embedding-only model.
 */
export async function loadModels() {
  try {
    const j = await api.listModels();
    state.models = (j?.models || []).slice().sort((a, b) => a.name.localeCompare(b.name));
    // /api/tags already reports capabilities on recent Ollama: no extra /api/show needed.
    for (const m of state.models) if (Array.isArray(m.capabilities) && m.capabilities.length) state.caps[m.name] = m.capabilities;
  } catch (e) {
    state.models = [];
    console.warn("model list failed", e);
  }
  $("#modelBtn").disabled = false;
  const names = chatModels().map(m => m.name);
  const preferred = [state.model, lastModel, state.settings?.default_model].find(n => n && names.includes(n));
  lastModel = null;
  await selectModel(preferred || await bestDefault(names) || null, false);
  if (!state.models.length) setStatus(t("noModels"), "err");
  else if (!names.length) setStatus(t("noChatModels"), "err");
  emit("models");
}

/** Empty and disable the picker (Ollama is down); loadModels() restores the selection. */
export function clearModels() {
  if (state.model) lastModel = state.model;
  closeModelMenu();
  state.models = [];
  state.model = null;
  $("#modelBtn").disabled = true;
  renderModelButton();
  updateThinkToggle();
  emit("model", null);
  emit("models");
}

/** Best installed catalog model for this computer (best rating, chat category first), else names[0]. */
async function bestDefault(names) {
  if (!names.length) return null;
  if (!systemInfo) systemInfo = await api.getSystemInfo().catch(() => null);
  const ranked = CATALOG
    .map((m, i) => ({ m, i, inst: names.find(n => n === m.name || n === `${m.name}:latest`) }))
    .filter(x => x.inst)
    .sort((a, b) => levelRank(rateModel(a.m, systemInfo)) - levelRank(rateModel(b.m, systemInfo))
      || (a.m.category !== "chat") - (b.m.category !== "chat") || a.i - b.i);
  return ranked[0]?.inst || names[0];
}

/** Make `name` the active model; optionally remember it as default_model. */
export async function selectModel(name, remember = true) {
  state.model = name;
  renderModelButton();
  await loadCaps(name);
  updateThinkToggle();
  emit("model", name);
  if (remember && name && state.settings?.default_model !== name) {
    saveSettings({ default_model: name }).catch(() => {});
  }
}

function renderModelButton() {
  const none = $("#modelBtn").disabled ? t("modelsUnavailable")
    : state.models.length && !chatModels().length ? t("noChatModel") : t("noModels");
  $("#modelName").textContent = state.model || none;
}

function updateThinkToggle() {
  const can = !!state.model && supportsThinking(state.model);
  const box = $("#think");
  box.disabled = !can;
  box.checked = can && !!state.settings?.think;
  const lbl = $("#thinkLbl");
  lbl.classList.toggle("disabled", !can);
  lbl.classList.toggle("on", box.checked); // class instead of :has() — more reliable repaint
  lbl.title = can ? t("deepThinking") : t("thinkUnsupported");
}

// ---------- dropdown ----------
function toggleModelMenu() { menuOpen ? closeModelMenu() : openModelMenu(); }

function openModelMenu() {
  const list = $("#modelList");
  const items = chatModels().map(m => el("button", {
    type: "button",
    class: "dd-item" + (m.name === state.model ? " active" : ""),
    role: "menuitemradio",
    "aria-checked": String(m.name === state.model),
    onclick: () => { closeModelMenu(); selectModel(m.name); },
  },
  el("span", { class: "dd-name", dir: "ltr" }, m.name),
  el("span", { class: "dd-meta", dir: "ltr" }, fmtSize(m.size)),
  el("span", { class: "dd-check", html: m.name === state.model ? icon("check", 16) : "" })));

  list.replaceChildren(
    ...(items.length ? items : [el("div", { class: "dd-empty" }, state.models.length ? t("noChatModels") : t("noModels"))]),
    el("div", { class: "dd-sep" }),
    el("button", { type: "button", class: "dd-item", onclick: () => { closeModelMenu(); openModelManager(); } },
      el("span", { class: "dd-icon", html: icon("cube", 16) }), el("span", { class: "dd-name" }, t("manageModels"))));
  list.hidden = false;
  menuOpen = true;
  $("#modelBtn").setAttribute("aria-expanded", "true");
}

/** Close the model dropdown. Returns true if it was open (for Esc handling). */
export function closeModelMenu() {
  if (!menuOpen) return false;
  $("#modelList").hidden = true;
  $("#modelBtn").setAttribute("aria-expanded", "false");
  menuOpen = false;
  return true;
}

// ---------- model manager modal ----------
export function openModelManager() {
  const rows = el("div", { class: "model-rows" });
  let pullCtl = null;

  const renderRows = () => {
    suggestions.refresh(); // "installed" marks follow the list
    if (!state.models.length) { rows.replaceChildren(el("p", { class: "muted" }, t("noModels"))); return; }
    rows.replaceChildren(...state.models.map(m => {
      const d = m.details || {};
      const meta = [d.parameter_size, d.quantization_level, fmtSize(m.size)].filter(Boolean).join(" · ");
      return el("div", { class: "model-row" },
        el("div", { class: "model-info" },
          el("div", { class: "model-name", dir: "ltr" }, m.name,
            !isChatModel(m) && el("span", { class: "model-tag", title: t("embedModelHint") }, t("embedModelTag"))),
          el("div", { class: "model-meta", dir: "ltr" }, meta)),
        iconButton("trash", t("delete"), () => removeModel(m.name), "icon-btn danger"));
    }));
  };

  const removeModel = async name => {
    const ok = await confirmDialog(t("deleteModelConfirm", { name }), { title: t("deleteModelTitle"), okText: t("delete"), danger: true });
    if (!ok) return;
    try {
      await api.deleteModel(name);
      toast(t("modelDeleted", { name }), "success");
      if (state.model === name) state.model = null;
      await loadModels();
      renderRows();
    } catch (e) {
      toast(t("modelDeleteFailed", { error: e.message }), "error");
    }
  };

  // download section
  const nameInput = el("input", { class: "input", dir: "ltr", placeholder: t("downloadPlaceholder"), spellcheck: "false" });
  const pullBtn = el("button", { class: "btn primary", type: "button", html: icon("download", 16) }, t("download"));
  const bar = el("div", { class: "progress-bar" });
  const progress = el("div", { class: "progress", hidden: true }, bar);
  const pullStatus = el("div", { class: "pull-status muted", dir: "auto" });
  const pullError = el("div", { class: "pull-error", dir: "auto", hidden: true });
  // Same confirmation for the Cancel button and for closing the dialog mid-download.
  const confirmStopPull = () => confirmDialog(t("cancelDownloadConfirm"),
    { title: t("cancelDownloadTitle"), okText: t("stopDownload"), cancelText: t("keepDownloading"), danger: true });
  const cancelPull = el("button", { class: "btn", type: "button", hidden: true,
    onclick: async () => { if (pullCtl && await confirmStopPull()) pullCtl?.abort(); } }, t("cancel"));
  const suggestions = suggestionPicker(nameInput);

  const setPulling = on => {
    pullBtn.disabled = on;
    nameInput.disabled = on;
    suggestions.select.disabled = on;
    progress.hidden = !on;
    cancelPull.hidden = !on;
  };

  const startPull = async () => {
    const name = nameInput.value.trim();
    if (!name || pullCtl) return;
    pullCtl = new AbortController();
    setPulling(true);
    bar.style.width = "0%";
    pullStatus.textContent = t("downloading");
    pullError.hidden = true;
    try {
      for await (const p of api.pullModel(name, pullCtl.signal)) {
        let text = pullStatusText(p.status);
        if (p.total) {
          const done = p.completed || 0;
          const pct = Math.min(100, (done / p.total) * 100);
          bar.style.width = pct.toFixed(1) + "%";
          text += ` — \u2066${fmtSize(done)} / ${fmtSize(p.total)} (${pct.toFixed(0)}%)\u2069`;
        }
        pullStatus.textContent = text;
      }
      bar.style.width = "100%";
      toast(t("downloadDone", { name }), "success");
      nameInput.value = "";
      await loadModels();
      renderRows();
    } catch (e) {
      pullStatus.textContent = "";
      if (e.name === "AbortError") toast(t("downloadCanceled"));
      else {
        const msg = pullErrorText(e.message);
        toast(t("downloadFailed", { error: msg }), "error");
        // Localized explanation, with Ollama's raw message in small print.
        pullError.replaceChildren(el("div", {}, t("downloadFailed", { error: msg })),
          msg !== e.message && el("small", { class: "muted", dir: "ltr" }, e.message));
        pullError.hidden = false;
      }
    } finally {
      pullCtl = null;
      setPulling(false);
    }
  };
  pullBtn.addEventListener("click", startPull);
  nameInput.addEventListener("keydown", e => { if (e.key === "Enter") { e.preventDefault(); startPull(); } });

  const refreshBtn = iconButton("refresh", t("refresh"), async () => { await loadModels(); renderRows(); });

  const m = openModal({
    title: t("modelsTitle"),
    className: "models",
    body: el("div", { class: "form" },
      el("div", { class: "section-head" }, el("h3", {}, t("installedModels")), refreshBtn),
      rows,
      el("h3", {}, t("downloadModel")),
      suggestions.node,
      el("div", { class: "pull-row" }, nameInput, pullBtn, cancelPull),
      progress,
      pullStatus,
      pullError),
    beforeClose: () => {
      if (!pullCtl) return true;
      confirmStopPull().then(ok => {
        if (ok) { pullCtl?.abort(); m.close(true); }
      });
      return false;
    },
  });
  renderRows();
  loadModels().then(renderRows);
}

// ---------- pull status / errors ----------
const PULL_STATUS = [
  [/^pulling manifest$/i, "pull_manifest"],
  [/^(pulling [0-9a-f]{6,}|downloading)/i, "pull_downloading"],
  [/^verifying (sha256 )?digest$/i, "pull_verifying"],
  [/^writing manifest$/i, "pull_writing"],
  [/^removing (any )?unused layers$/i, "pull_cleanup"],
  [/^success$/i, "pull_success"],
];

/** Ollama pull status → localized text; unknown statuses are shown as-is. */
export function pullStatusText(status) {
  const s = String(status || "").trim();
  const hit = PULL_STATUS.find(([re]) => re.test(s));
  return hit ? t(hit[1]) : s;
}

/** Known Ollama pull errors → localized message; others unchanged. */
export function pullErrorText(message) {
  const m = String(message || "");
  if (/file does not exist|manifest unknown|not found/i.test(m)) return t("pull_notFound");
  return m;
}

// ---------- download suggestions ----------
let systemInfo = null; // GET /api/app/system, cached after the first success

const fmtGb = x => String(Math.round(x * 10) / 10);
const levelRank = r => (r ? LEVELS.indexOf(r.level) : 0);

/**
 * Hardware line + explanation + catalog <select> (grouped by category, best first)
 * + details card for the picked model. Picking fills `input`; typing a catalog
 * tag in `input` selects it. Returns { node, select, refresh }.
 */
function suggestionPicker(input) {
  const hw = el("div", { class: "hw-summary" }, t("hwLoading"));
  const select = el("select", { class: "input suggest-select", dir: "ltr" });
  const card = el("div", { class: "suggest-card", hidden: true });

  const renderHw = failed => {
    if (failed) { hw.textContent = t("hwUnknown"); return; }
    const gpus = discreteGpus(systemInfo).map(g => (g.vram_gb ? `${g.name} (${fmtGb(g.vram_gb)} GB)` : g.name));
    hw.replaceChildren(`${t("hwYour")} `, el("bdi", {}, gpus.join(" + ") || t("hwNoGpu")),
      " · ", el("bdi", {}, t("hwRam", { ram: Math.round(systemInfo.ram_gb) })));
  };

  const fact = (label, value) => el("span", { class: "suggest-fact" },
    el("span", { class: "muted" }, `${label}: `), el("bdi", {}, value));

  const renderCard = () => {
    const m = CATALOG.find(x => x.name === select.value);
    card.hidden = !m;
    if (!m) return;
    const r = rateModel(m, systemInfo);
    card.className = "suggest-card" + (r ? ` lvl-${r.level}` : "");
    card.replaceChildren(
      el("div", { class: "suggest-head" },
        el("span", { class: "model-name", dir: "ltr" }, m.name),
        r && el("span", { class: "suggest-badge" }, r.label)),
      el("p", { class: "suggest-desc" }, describe(m)),
      el("div", { class: "suggest-facts" },
        fact(t("suggestSize"), `${m.sizeGb} GB`),
        fact(t("suggestParams"), m.params),
        m.moe && fact("MoE", t("suggestActive", { active: m.activeParams })),
        fact(t("suggestArabic"), t(`arabic_${m.arabic}`)),
        modelInstalled(m.name) && el("span", { class: "suggest-fact" }, `✔ ${t("suggestInstalled")}`)),
      r && el("p", { class: "suggest-reason" }, r.reason));
  };

  const refresh = () => {
    const rated = CATALOG.map(m => ({ m, r: rateModel(m, systemInfo) }));
    select.replaceChildren(
      el("option", { value: "" }, t("suggestPick")),
      ...CATEGORIES.map(cat => el("optgroup", { label: t(`cat_${cat}`) },
        ...rated.filter(x => x.m.category === cat)
          // best/good: bigger (smarter) first; slow/no: smaller (less slow) first
          .sort((a, b) => levelRank(a.r) - levelRank(b.r)
            || (levelRank(a.r) >= 2 ? a.m.sizeGb - b.m.sizeGb : b.m.sizeGb - a.m.sizeGb))
          .map(({ m, r }) => el("option", { value: m.name },
            [m.name, `${m.sizeGb} GB`, r?.short, modelInstalled(m.name) && t("suggestInstalled")].filter(Boolean).join(" · "))))));
    const typed = input.value.trim();
    select.value = CATALOG.some(m => m.name === typed) ? typed : "";
    renderCard();
  };

  select.addEventListener("change", () => { input.value = select.value; renderCard(); });
  input.addEventListener("input", () => {
    const typed = input.value.trim();
    select.value = CATALOG.some(m => m.name === typed) ? typed : "";
    renderCard();
  });

  if (systemInfo) renderHw(false);
  else {
    api.getSystemInfo()
      .then(s => { systemInfo = s; renderHw(false); refresh(); })
      .catch(() => renderHw(true));
  }

  const node = el("div", { class: "suggest" },
    hw,
    el("p", { class: "suggest-explain muted" }, t("suggestExplain")),
    select,
    card);
  return { node, select, refresh };
}

export default { name: "models", init: initModels, onEscape: closeModelMenu };
