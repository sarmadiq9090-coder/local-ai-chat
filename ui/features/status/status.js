// status.js — backend/Ollama connection status: the top-bar status text, the
// "Ollama is not running" banner with its "start Ollama" button, and polling
// while the backend auto-starts Ollama. Loads the installed models once Ollama
// is reachable, and re-checks when the Ollama URL changes in settings.
// Cross-feature deps: settings (settingsLoaded), models (loadModels).

import { state, on } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, setStatus, toast } from "../../core/dom.js";
import { t, registerStrings } from "../../core/i18n.js";
import { settingsLoaded } from "../settings/settings.js";
import { loadModels } from "../models/models.js";
import strings from "./strings.js";

registerStrings("status", strings);

let connecting = false; // a status request is in flight
let checked = false;    // at least one status request has finished

function showBanner(show) {
  $("#banner").hidden = !show;
}

/** Ask the backend whether Ollama is up; load models or show the start banner. */
export async function checkStatus() {
  setStatus(t("statusConnecting"));
  connecting = true;
  let status;
  try {
    status = await api.getStatus();
  } catch {
    connecting = false;
    checked = true;
    state.status = null;
    setStatus(t("backendDown"), "err");
    showBanner(false);
    return;
  }
  connecting = false;
  checked = true;
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

/** Re-render the status line from the last known status (after a language switch). */
function renderStatusText() {
  const s = state.status;
  if (connecting) setStatus(t("statusConnecting"));
  else if (!checked) { /* nothing shown yet */ }
  else if (!s) setStatus(t("backendDown"), "err");
  else if (!s.ollama) setStatus(t("ollamaDown"), "err");
  else if (!state.models.length) setStatus(t("noModels"), "err");
  else setStatus(t("statusConnected") + (s.version ? ` · Ollama ${s.version}` : ""), "ok");
  const btn = $("#startOllama");
  if (btn.disabled) btn.textContent = t("startingOllama");
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

export default {
  name: "status",
  init() {
    $("#startOllama").addEventListener("click", startOllama);
    on("ollama-url-changed", checkStatus);
    on("language", renderStatusText);
  },
  // Models depend on settings.default_model, so wait for settings first.
  async start() {
    await settingsLoaded;
    await checkStatus();
  },
};
