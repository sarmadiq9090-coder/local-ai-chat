// main.js — bootstrap: translate static markup, load settings + theme, wire all
// modules, check backend/Ollama status (banner + "start Ollama"), load models
// and conversations, and install global keyboard shortcuts.

import { state, on } from "./state.js";
import * as api from "./api.js";
import { $, setStatus, toast, closeTopModal, hasOpenModal } from "./dom.js";
import { translateDom, t } from "./i18n.js";
import { icon } from "./icons.js";
import { installCodeCopy } from "./markdown.js";
import { loadSettings, openSettings } from "./settings.js";
import { initModels, loadModels, closeModelMenu } from "./models.js";
import { initSidebar, refreshConversations, newChat, closeMobileSidebar } from "./sidebar.js";
import { initChat, stopGeneration } from "./chat.js";
import { exportConversation } from "./exporter.js";
import { initPreview, closePreview } from "./preview.js";

/** Fill every <span data-icon="name"> placeholder in the static markup. */
function renderStaticIcons() {
  document.querySelectorAll("[data-icon]").forEach(n => { n.innerHTML = icon(n.dataset.icon, Number(n.dataset.size) || 18); });
}

function showBanner(show) {
  $("#banner").hidden = !show;
}

/** Ask the backend whether Ollama is up; load models or show the start banner. */
async function checkStatus() {
  setStatus(t("statusConnecting"));
  let status;
  try {
    status = await api.getStatus();
  } catch {
    state.status = null;
    setStatus(t("backendDown"), "err");
    showBanner(false);
    return;
  }
  state.status = status;
  if (status?.ollama) {
    showBanner(false);
    setStatus(t("statusConnected") + (status.version ? ` · Ollama ${status.version}` : ""), "ok");
    await loadModels();
  } else {
    showBanner(true);
    setStatus(t("ollamaDown"), "err");
    pollUntilUp();
  }
}

let polling = false;

/** The backend auto-starts Ollama at launch; keep checking for ~30s so the UI picks it up. */
async function pollUntilUp() {
  if (polling) return;
  polling = true;
  try {
    for (let i = 0; i < 12; i++) {
      await new Promise(r => setTimeout(r, 2500));
      const status = await api.getStatus().catch(() => null);
      if (status?.ollama) { await checkStatus(); return; }
    }
  } finally {
    polling = false;
  }
}

async function startOllama() {
  const btn = $("#startOllama");
  btn.disabled = true;
  btn.textContent = t("startingOllama");
  try {
    const res = await api.startOllama();
    if (!res?.ok) toast(t("startFailed"), "error");
  } catch (e) {
    toast(`${t("startFailed")} (${e.message})`, "error");
  } finally {
    btn.disabled = false;
    btn.textContent = t("startOllama");
    await checkStatus();
  }
}

function installShortcuts() {
  document.addEventListener("keydown", e => {
    if (e.key === "Escape") {
      if (closeModelMenu() || closeTopModal() || stopGeneration() || closePreview() || closeMobileSidebar()) e.preventDefault();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "n") {
      e.preventDefault(); // also blocks WebView2's "new window"
      if (!hasOpenModal()) newChat();
    }
  });
}

async function main() {
  translateDom();
  renderStaticIcons();
  installCodeCopy();
  installShortcuts();

  initSidebar();
  initModels();
  initChat();
  initPreview();

  $("#openSettings").addEventListener("click", openSettings);
  $("#startOllama").addEventListener("click", startOllama);
  $("#exportBtn").addEventListener("click", () => exportConversation(state.current));
  on("ollama-url-changed", checkStatus);

  await loadSettings();
  await Promise.all([checkStatus(), refreshConversations()]);
  $("#input").focus();
}

main().catch(e => {
  console.error(e);
  toast(String(e?.message || e), "error", 8000);
});
