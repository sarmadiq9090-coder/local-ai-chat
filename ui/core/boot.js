// boot.js — runs synchronously in <head> before first paint: applies the last UI
// language (no RTL flash for English users) and the last theme (no dark flash for
// light-theme users; "system" follows prefers-color-scheme). features/settings keeps
// localStorage "ui.theme" in sync. External file so the CSP needs no inline script.
// First run (no "ui.lang" yet): English unless the WebView language (= Windows display language) is Arabic.
try {
  var bootLang = localStorage.getItem("ui.lang") || (/^ar\b/i.test(navigator.language || "") ? "ar" : "en");
  if (bootLang === "en") { document.documentElement.lang = "en"; document.documentElement.dir = "ltr"; document.title = "Lemon"; }
} catch (e) {}
try {
  var bootTheme = localStorage.getItem("ui.theme");
  if (bootTheme === "system") bootTheme = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  if (bootTheme === "light" || bootTheme === "dark") document.documentElement.setAttribute("data-theme", bootTheme);
} catch (e) {}
