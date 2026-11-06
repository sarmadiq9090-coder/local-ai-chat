// i18n.js — string lookup for the whole UI. Strings are NOT defined here: each
// feature owns a `strings.js` ({ ar: {...}, en: {...} }) and registers it with
// registerStrings() when its module loads; shared/core strings live in
// core/strings.js. Keys are global (must be unique across features).
// `ar` is the source language: t() falls back to it when a key is missing in
// the active language. Static markup is translated through data-i18n /
// data-i18n-placeholder / data-i18n-title attributes via translateDom().
// Digits (owner rule): the Arabic UI shows Western digits 123 unless the user picks
// Arabic-Indic ١٢٣ (settings.digits → setLanguage(lang, digits)). Show numbers with formatNumber /
// formatDate / localizeDigits (number vars of t() are localized automatically);
// read typed numbers with parseNumber, which accepts both.

import coreStrings from "./strings.js";
import { emit } from "./state.js";

const FALLBACK = "ar";
const dictionaries = { ar: {}, en: {} }; // lang -> merged { key: string }
const owners = {};                      // key -> feature name (duplicate detection)
let lang = FALLBACK;

/**
 * Merge a feature's strings into the dictionaries.
 * `strings` = { ar: {...}, en: {...}, <lang>: {...} }. Warns on keys already
 * registered by another feature (keys must stay globally unique).
 */
export function registerStrings(featureName, strings) {
  for (const [code, entries] of Object.entries(strings || {})) {
    const dict = (dictionaries[code] ||= {});
    for (const [key, value] of Object.entries(entries || {})) {
      const owner = owners[`${code}:${key}`];
      if (owner && owner !== featureName) console.warn(`i18n: key "${key}" (${code}) registered by both "${owner}" and "${featureName}"`);
      owners[`${code}:${key}`] = featureName;
      dict[key] = value;
    }
  }
}

const LANG_KEY = "ui.lang"; // last language, so startup can apply it before settings load

/** Set <html lang/dir> for `code` (rtl for Arabic, ltr otherwise). */
function applyDocumentLanguage(code) {
  document.documentElement.lang = code;
  document.documentElement.dir = code === "ar" ? "rtl" : "ltr";
}

/** The active language code ("ar" | "en"). */
export const getLanguage = () => lang;

/**
 * Switch the active language (unknown codes fall back to Arabic) and, when given,
 * the digits ("latn" | "arab", settings.digits): sets <html lang/dir>, re-translates
 * static markup and emits "language" so features re-render their dynamic text.
 * No-op if nothing changed.
 */
export function setLanguage(code, digits) {
  const next = code in dictionaries && code !== "" ? code : FALLBACK;
  try { localStorage.setItem(LANG_KEY, next); } catch { /* storage unavailable */ }
  const nextDigits = digits === undefined ? digitMode : digits === "arab" ? "arab" : "latn";
  if (next === lang && nextDigits === digitMode && document.documentElement.lang === next) return;
  digitMode = nextDigits;
  lang = next;
  applyDocumentLanguage(lang);
  translateDom();
  emit("language", lang);
}

/** Look up a string; `{name}` placeholders are filled from `vars` (numbers get the UI digits). */
export function t(key, vars) {
  const dict = dictionaries[lang];
  let s = key in dict ? dict[key] : (dictionaries[FALLBACK][key] ?? key);
  if (typeof s === "string" && vars) {
    s = s.replace(/\{(\w+)\}/g, (m, k) => (!(k in vars) ? m : typeof vars[k] === "number" ? localizeDigits(vars[k]) : String(vars[k])));
  }
  return s;
}

// ---------- digits ----------
let digitMode = "latn"; // "latn" (123) | "arab" (١٢٣); only used while the UI is Arabic
const ARABIC_INDIC = "٠١٢٣٤٥٦٧٨٩";

/** The applied digit setting ("latn" | "arab"); set with setLanguage(code, digits). */
export const getDigits = () => digitMode;

/** Locale for Intl formatters: Arabic with the chosen digits, else English. */
export const numberLocale = () => (lang === "ar" ? `ar-u-nu-${digitMode}` : "en");

/** A number in the UI language and digits (Intl.NumberFormat options). */
export const formatNumber = (n, options) => new Intl.NumberFormat(numberLocale(), options).format(n);

/** A date/time in the UI language and digits (toLocaleString options). */
export const formatDate = (date, options) => new Date(date).toLocaleString(numberLocale(), options);

/**
 * An already formatted number such as "4.7 GB" or "12%" in the UI digits:
 * unchanged with 123; with ١٢٣ every digit is swapped and a "." between digits
 * becomes the Arabic decimal mark "٫". Use only on numbers, not versions or model names.
 */
export function localizeDigits(text) {
  const s = String(text);
  if (lang !== "ar" || digitMode !== "arab") return s;
  return s.replace(/(\d)\.(?=\d)/g, "$1\u066B").replace(/\d/g, d => ARABIC_INDIC[d]);
}

/**
 * Parse a typed number: Western 123, Arabic-Indic ١٢٣ and Persian ۱۲۳ digits, the
 * Arabic decimal mark ٫ and thousands separators (٬ , and spaces). "" → NaN.
 */
export function parseNumber(text) {
  const s = String(text ?? "").trim()
    .replace(/[\u0660-\u0669]/g, c => String(c.charCodeAt(0) - 0x0660))
    .replace(/[\u06F0-\u06F9]/g, c => String(c.charCodeAt(0) - 0x06F0))
    .replace(/\u066B/g, ".")
    .replace(/[\u066C,\s]/g, "");
  return s === "" ? NaN : Number(s);
}

/** Translate static markup that carries data-i18n* attributes. */
export function translateDom(root = document) {
  root.querySelectorAll("[data-i18n]").forEach(n => { n.textContent = t(n.dataset.i18n); });
  root.querySelectorAll("[data-i18n-placeholder]").forEach(n => { n.placeholder = t(n.dataset.i18nPlaceholder); });
  root.querySelectorAll("[data-i18n-title]").forEach(n => {
    n.title = t(n.dataset.i18nTitle);
    n.setAttribute("aria-label", n.title);
  });
  root.querySelectorAll("[data-i18n-aria-label]").forEach(n => { n.setAttribute("aria-label", t(n.dataset.i18nAriaLabel)); });
}

registerStrings("core", coreStrings);

// Startup: use the last language before settings arrive (core/boot.js sets dir/lang
// even earlier, before first paint); settings.js calls setLanguage() once loaded.
// First run (nothing stored): guess from the WebView's language, which follows the
// Windows display language, as the backend's first-run default does (config.rs).
let storedAtStartup = null;
/** The language stored by an earlier run ("ar" | "en"), or null on a first run. */
export const previousLanguage = () => storedAtStartup;
try {
  const saved = localStorage.getItem(LANG_KEY);
  if (saved && saved in dictionaries) storedAtStartup = saved;
  const first = saved ? null : (/^ar\b/i.test(navigator.language || "") ? "ar" : "en");
  const start = saved && saved in dictionaries ? saved : first;
  if (start) { lang = start; applyDocumentLanguage(lang); }
} catch { /* storage unavailable */ }
