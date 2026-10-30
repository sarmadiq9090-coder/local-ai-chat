// i18n.js — string lookup for the whole UI. Strings are NOT defined here: each
// feature owns a `strings.js` ({ ar: {...}, en: {...} }) and registers it with
// registerStrings() when its module loads; shared/core strings live in
// core/strings.js. Keys are global (must be unique across features).
// `ar` is the source language: t() falls back to it when a key is missing in
// the active language. Static markup is translated through data-i18n /
// data-i18n-placeholder / data-i18n-title attributes via translateDom().

import coreStrings from "./strings.js";

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

/** Switch the active language (unknown codes fall back to Arabic). */
export function setLanguage(code) {
  lang = code in dictionaries ? code : FALLBACK;
}

/** Look up a string; `{name}` placeholders are filled from `vars`. */
export function t(key, vars) {
  const dict = dictionaries[lang];
  let s = key in dict ? dict[key] : (dictionaries[FALLBACK][key] ?? key);
  if (typeof s === "string" && vars) {
    s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  }
  return s;
}

/** Translate static markup that carries data-i18n* attributes. */
export function translateDom(root = document) {
  root.querySelectorAll("[data-i18n]").forEach(n => { n.textContent = t(n.dataset.i18n); });
  root.querySelectorAll("[data-i18n-placeholder]").forEach(n => { n.placeholder = t(n.dataset.i18nPlaceholder); });
  root.querySelectorAll("[data-i18n-title]").forEach(n => {
    n.title = t(n.dataset.i18nTitle);
    n.setAttribute("aria-label", n.title);
  });
}

registerStrings("core", coreStrings);
