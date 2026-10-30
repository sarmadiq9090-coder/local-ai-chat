// settings.js — app settings: load/save via /api/app/settings, theme and UI
// language application, and the settings modal (language, system prompt,
// temperature, context length, theme, Ollama URL). Emits "settings" after every change and "ollama-url-changed"
// when the Ollama address is edited. Owns the sidebar "settings" button.
// Other features wait for `settingsLoaded` before using state.settings at startup.
// "settings-open" payload: { form, draft, contribute(fn) } — a feature may append a section to
// `form`, read its initial values from `draft`, and register fn() → partial settings patch that is
// saved with the dialog's Save (and kept as the draft across a live language rebuild); Cancel drops it.
// Writes localStorage "ui.theme" (read by core/boot.js before first paint) whenever the saved theme changes.

import { state, emit } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, el, openModal, toast } from "../../core/dom.js";
import { t, registerStrings, setLanguage, getLanguage } from "../../core/i18n.js";
import strings from "./strings.js";

registerStrings("settings", strings);

/** Built-in system prompts per UI language (must match config.rs DEFAULT_PROMPT_*). */
export const DEFAULT_PROMPTS = {
  ar: "أنت مساعد ذكي ومفيد. أجب دائمًا بنفس لغة المستخدم؛ إذا كتب بالعربية فأجب بعربية فصحى واضحة. ضع أي كود داخل كتل ``` مع اسم اللغة.",
  en: "You are a helpful, smart assistant. Always reply in the user's language; if they write in English, answer in clear, natural English. Put any code inside ``` blocks with the language name.",
};

/** A prompt still equal to a built-in default follows the language; custom ones are kept. Pure. */
export function promptForLanguage(prompt, lang) {
  const isDefault = Object.values(DEFAULT_PROMPTS).includes(String(prompt ?? "").trim());
  return isDefault ? DEFAULT_PROMPTS[lang] ?? DEFAULT_PROMPTS.ar : prompt;
}

const LANGUAGES = [["ar", "العربية"], ["en", "English"]]; // endonyms: readable in either UI language

export const DEFAULT_SETTINGS = {
  ollama_url: "http://127.0.0.1:11434",
  system_prompt: DEFAULT_PROMPTS.ar,
  temperature: 0.7,
  num_ctx: 8192,
  theme: "dark",
  default_model: null,
  think: false,
  language: "ar",
  auto_update: true,
};

const CTX_OPTIONS = [2048, 4096, 8192, 16384, 32768];
const darkQuery = window.matchMedia?.("(prefers-color-scheme: dark)");

/** Resolve "system" to dark/light and set <html data-theme>. */
export function applyTheme(theme) {
  const resolved = theme === "system" ? (darkQuery?.matches ? "dark" : "light") : (theme === "light" ? "light" : "dark");
  document.documentElement.dataset.theme = resolved;
}

const THEME_KEY = "ui.theme"; // read by core/boot.js before first paint
/** Persist the saved (not previewed) theme for the next startup. */
function rememberTheme(theme) {
  try { localStorage.setItem(THEME_KEY, theme || "dark"); } catch { /* storage unavailable */ }
}

darkQuery?.addEventListener?.("change", () => {
  if (state.settings?.theme === "system") applyTheme("system");
});

let markLoaded;
/** Resolves once the first loadSettings() has finished (startup ordering). */
export const settingsLoaded = new Promise(resolve => { markLoaded = resolve; });

// True once settings were read from the backend. Until then state.settings holds
// defaults, and saving them would overwrite the real file: saveSettings() retries the load first.
let loadedFromServer = false;
// True while the settings dialog is open: it owns the live theme/language preview, so saves made
// meanwhile (e.g. the auto-update toggle) must not re-apply the saved theme/language over it.
let previewing = false;

async function fetchSettings() {
  const s = (await api.getSettings()) || {};
  loadedFromServer = true;
  return s;
}

/** Fetch settings from the backend (defaults on failure) and apply the theme. */
export async function loadSettings() {
  let s = {};
  try { s = await fetchSettings(); } catch (e) { loadedFromServer = false; console.warn("settings load failed", e); }
  state.settings = { ...DEFAULT_SETTINGS, ...s };
  applyTheme(state.settings.theme);
  if (loadedFromServer) rememberTheme(state.settings.theme);
  setLanguage(state.settings.language);
  emit("settings");
  markLoaded();
  return state.settings;
}

/** Merge `patch` into settings and persist. Returns the saved settings. */
let saveChain = Promise.resolve();

export async function saveSettings(patch) {
  if (!loadedFromServer) {
    // Startup load failed: never PUT defaults over the real file. Retry the load first.
    try {
      state.settings = { ...DEFAULT_SETTINGS, ...(await fetchSettings()) };
    } catch (e) {
      toast(t("settingsLoadFailed", { error: e.message }), "error");
      throw e;
    }
  }
  state.settings = { ...state.settings, ...patch };
  // Serialize saves so a slower earlier response can't overwrite a newer one.
  const snapshot = { ...state.settings };
  const request = saveChain.then(() => api.saveSettings(snapshot));
  saveChain = request.catch(() => {});
  try {
    const saved = await request;
    if (saved && typeof saved === "object") state.settings = { ...DEFAULT_SETTINGS, ...saved };
    rememberTheme(state.settings.theme);
  } catch (e) {
    toast(t("settingsSaveFailed", { error: e.message }), "error");
    throw e;
  } finally {
    if (!previewing) {
      applyTheme(state.settings.theme);
      setLanguage(state.settings.language);
    }
    emit("settings");
  }
  return state.settings;
}

let fieldSeq = 0;
/** Labelled form row; the label is tied to `control` (or its first form control) via for/id. */
const field = (label, control, extra) => {
  const target = control.matches("input, select, textarea") ? control : control.querySelector("input, select, textarea");
  if (target && !target.id) target.id = `set-field-${++fieldSeq}`;
  return el("div", { class: "field" }, el("label", { class: "field-label", for: target?.id }, label, extra), control);
};

/**
 * Open the settings dialog. `draft` (unsaved field values) is used when the
 * dialog rebuilds itself after a live language switch.
 */
export function openSettings(draft) {
  previewing = true;
  const s = { ...(state.settings || DEFAULT_SETTINGS), ...(draft || {}) };

  const prompt = el("textarea", { rows: 6, dir: "auto", class: "input" });
  prompt.value = s.system_prompt || "";

  const tempOut = el("output", { class: "range-val", dir: "ltr" }, Number(s.temperature).toFixed(1));
  const temp = el("input", { type: "range", min: "0", max: "2", step: "0.1", class: "range" });
  temp.value = String(s.temperature);
  temp.addEventListener("input", () => { tempOut.textContent = Number(temp.value).toFixed(1); });

  const ctx = el("select", { class: "input", dir: "ltr" },
    CTX_OPTIONS.map(n => el("option", { value: String(n) }, n.toLocaleString("en-US"))));
  ctx.value = String(CTX_OPTIONS.includes(Number(s.num_ctx)) ? s.num_ctx : 8192);

  const theme = el("select", { class: "input" },
    el("option", { value: "dark" }, t("themeDark")),
    el("option", { value: "light" }, t("themeLight")),
    el("option", { value: "system" }, t("themeSystem")));
  theme.value = s.theme || "dark";
  theme.addEventListener("change", () => applyTheme(theme.value)); // live preview

  const url = el("input", { type: "url", class: "input", dir: "ltr", spellcheck: false });
  url.value = s.ollama_url || DEFAULT_SETTINGS.ollama_url;

  const language = el("select", { class: "input" },
    LANGUAGES.map(([code, name]) => el("option", { value: code, lang: code, dir: code === "ar" ? "rtl" : "ltr" }, name)));
  language.value = getLanguage();
  // Sections added by other features (see "settings-open"): each returns a partial patch.
  const contributors = [];
  const contributed = () => Object.assign({}, ...contributors.map(fn => { try { return fn() || {}; } catch { return {}; } }));

  // Live switch: apply the language, then rebuild the dialog in it, keeping unsaved edits.
  language.addEventListener("change", () => {
    const next = language.value;
    const values = {
      ...(draft || {}),
      ...contributed(),
      system_prompt: promptForLanguage(prompt.value, next),
      temperature: Number(temp.value),
      num_ctx: Number(ctx.value),
      theme: theme.value,
      ollama_url: url.value,
    };
    rebuilding = true;
    m.close(true);
    setLanguage(next);
    openSettings(values);
  });

  let saved = false;
  let rebuilding = false;
  const saveBtn = el("button", { class: "btn primary", type: "button" }, t("save"));
  const cancelBtn = el("button", { class: "btn", type: "button", onclick: () => m.close() }, t("cancel"));

  saveBtn.addEventListener("click", async () => {
    const oldUrl = state.settings?.ollama_url;
    saveBtn.disabled = true;
    try {
      await saveSettings({
        ...contributed(),
        system_prompt: prompt.value,
        temperature: Number(temp.value),
        num_ctx: Number(ctx.value),
        theme: theme.value,
        language: language.value,
        ollama_url: url.value.trim() || DEFAULT_SETTINGS.ollama_url,
      });
      saved = true;
      previewing = false;
      applyTheme(state.settings.theme);
      setLanguage(state.settings.language);
      toast(t("saved"), "success");
      m.close(true);
      if (state.settings.ollama_url !== oldUrl) emit("ollama-url-changed");
    } catch {
      saveBtn.disabled = false;
    }
  });

  const m = openModal({
    title: t("settingsTitle"),
    className: "settings",
    body: el("div", { class: "form" },
      field(t("language"), language),
      field(t("systemPrompt"), prompt),
      field(t("temperature"), el("div", { class: "range-row" }, temp, tempOut)),
      el("div", { class: "field-row" },
        field(t("contextLength"), ctx),
        field(t("theme"), theme)),
      field(t("ollamaUrl"), url)),
    actions: [cancelBtn, saveBtn],
    onClose: () => { // undo live previews
      if (rebuilding) return; // reopened right away in the new language
      previewing = false;
      if (saved) return;
      applyTheme(state.settings?.theme);
      setLanguage(state.settings?.language);
    },
  });
  // Other features may append a section to the form (e.g. update) and add to the draft.
  emit("settings-open", { form: m.body.firstChild, draft: s, contribute: fn => contributors.push(fn) });
}

export default {
  name: "settings",
  init() { $("#openSettings").addEventListener("click", () => openSettings()); },
  start: loadSettings,
};
