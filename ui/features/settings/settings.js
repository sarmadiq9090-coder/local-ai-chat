// settings.js — app settings: load/save via /api/app/settings, theme application,
// and the settings modal (system prompt, temperature, context length, theme,
// Ollama URL). Emits "settings" after every change and "ollama-url-changed"
// when the Ollama address is edited. Owns the sidebar "settings" button.
// Other features wait for `settingsLoaded` before using state.settings at startup.

import { state, emit } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, el, openModal, toast } from "../../core/dom.js";
import { t, registerStrings } from "../../core/i18n.js";
import strings from "./strings.js";

registerStrings("settings", strings);

export const DEFAULT_SETTINGS = {
  ollama_url: "http://127.0.0.1:11434",
  system_prompt: "أنت مساعد ذكي ومفيد. أجب دائمًا بنفس لغة المستخدم؛ إذا كتب بالعربية فأجب بعربية فصحى واضحة. ضع أي كود داخل كتل ``` مع اسم اللغة.",
  temperature: 0.7,
  num_ctx: 8192,
  theme: "dark",
  default_model: null,
  think: false,
};

const CTX_OPTIONS = [2048, 4096, 8192, 16384, 32768];
const darkQuery = window.matchMedia?.("(prefers-color-scheme: dark)");

/** Resolve "system" to dark/light and set <html data-theme>. */
export function applyTheme(theme) {
  const resolved = theme === "system" ? (darkQuery?.matches ? "dark" : "light") : (theme === "light" ? "light" : "dark");
  document.documentElement.dataset.theme = resolved;
}

darkQuery?.addEventListener?.("change", () => {
  if (state.settings?.theme === "system") applyTheme("system");
});

let markLoaded;
/** Resolves once the first loadSettings() has finished (startup ordering). */
export const settingsLoaded = new Promise(resolve => { markLoaded = resolve; });

/** Fetch settings from the backend (defaults on failure) and apply the theme. */
export async function loadSettings() {
  let s = {};
  try { s = (await api.getSettings()) || {}; } catch (e) { console.warn("settings load failed", e); }
  state.settings = { ...DEFAULT_SETTINGS, ...s };
  applyTheme(state.settings.theme);
  emit("settings");
  markLoaded();
  return state.settings;
}

/** Merge `patch` into settings and persist. Returns the saved settings. */
let saveChain = Promise.resolve();

export async function saveSettings(patch) {
  state.settings = { ...state.settings, ...patch };
  // Serialize saves so a slower earlier response can't overwrite a newer one.
  const snapshot = { ...state.settings };
  const request = saveChain.then(() => api.saveSettings(snapshot));
  saveChain = request.catch(() => {});
  try {
    const saved = await request;
    if (saved && typeof saved === "object") state.settings = { ...DEFAULT_SETTINGS, ...saved };
  } catch (e) {
    toast(t("settingsSaveFailed", { error: e.message }), "error");
    throw e;
  } finally {
    applyTheme(state.settings.theme);
    emit("settings");
  }
  return state.settings;
}

const field = (label, control, extra) =>
  el("label", { class: "field" }, el("span", { class: "field-label" }, label, extra), control);

/** Open the settings dialog. */
export function openSettings() {
  const s = state.settings || DEFAULT_SETTINGS;

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

  let saved = false;
  const saveBtn = el("button", { class: "btn primary", type: "button" }, t("save"));
  const cancelBtn = el("button", { class: "btn", type: "button", onclick: () => m.close() }, t("cancel"));

  saveBtn.addEventListener("click", async () => {
    const oldUrl = state.settings?.ollama_url;
    saveBtn.disabled = true;
    try {
      await saveSettings({
        system_prompt: prompt.value,
        temperature: Number(temp.value),
        num_ctx: Number(ctx.value),
        theme: theme.value,
        ollama_url: url.value.trim() || DEFAULT_SETTINGS.ollama_url,
      });
      saved = true;
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
      field(t("systemPrompt"), prompt),
      field(t("temperature"), el("div", { class: "range-row" }, temp, tempOut)),
      el("div", { class: "field-row" },
        field(t("contextLength"), ctx),
        field(t("theme"), theme)),
      field(t("ollamaUrl"), url)),
    actions: [cancelBtn, saveBtn],
    onClose: () => { if (!saved) applyTheme(state.settings?.theme); }, // undo live preview
  });
}

export default {
  name: "settings",
  init() { $("#openSettings").addEventListener("click", openSettings); },
  start: loadSettings,
};
