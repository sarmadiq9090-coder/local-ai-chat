// boot.js — runs synchronously in <head> before first paint: applies the last UI
// language (no RTL flash for English users). External file so the CSP needs no inline script.
try { if (localStorage.getItem("ui.lang") === "en") { document.documentElement.lang = "en"; document.documentElement.dir = "ltr"; document.title = "Local AI Chat"; } } catch (e) {}
