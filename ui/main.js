// main.js — bootstrap only: translate the static markup, render static icons,
// then run every feature listed in features/index.js (init → start), and route
// the Esc key through the features' onEscape() hooks in list order.
// Feature-specific wiring belongs in the feature, not here.

import { emit } from "./core/state.js";
import { toast } from "./core/dom.js";
import { translateDom } from "./core/i18n.js";
import { icon } from "./core/icons.js";
import features from "./features/index.js";

/** Fill every <span data-icon="name"> placeholder in the static markup. */
function renderStaticIcons() {
  document.querySelectorAll("[data-icon]").forEach(n => { n.innerHTML = icon(n.dataset.icon, Number(n.dataset.size) || 18); });
}

function installEscapeChain() {
  document.addEventListener("keydown", e => {
    if (e.key !== "Escape") return;
    if (features.some(f => f.onEscape?.())) e.preventDefault();
  });
}

async function main() {
  translateDom();
  renderStaticIcons();
  installEscapeChain();
  for (const f of features) f.init?.();
  await Promise.all(features.map(f => f.start?.()));
  emit("focus-composer");
}

main().catch(e => {
  console.error(e);
  toast(String(e?.message || e), "error", 8000);
});
