// status.js — backend/Ollama connection status: the top-bar status text, the
// "Ollama is not running" banner with its "start Ollama" button, and polling
// (with backoff, 2.5 s → 15 s, for as long as it stays down) until Ollama answers.
// Loads the installed models once Ollama is reachable, clears/disables the model
// picker while it is down, and re-checks when the Ollama URL changes in settings
// or when another feature emits "ollama-maybe-down" (e.g. a chat request failed).
// Cross-feature deps: settings (settingsLoaded), models (loadModels, clearModels, chatModels).

import { state, on } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, setStatus, toast } from "../../core/dom.js";
import { t, registerStrings } from "../../core/i18n.js";
import { settingsLoaded } from "../settings/settings.js";
import { loadModels, clearModels, chatModels } from "../models/models.js";
import strings from "./strings.js";

registerStrings("status", strings);

const POLL_FIRST_MS = 2500;
const POLL_MAX_MS = 15000;
const START_SLOW_MS = 20000; // "Start Ollama": give the button back after this long

let connecting = false; // a status request is in flight
let checked = false;    // at least one status request has finished

function showBanner(show) {
  $("#banner").hidden = !show;
}

const sleep = ms => new Promise(r => setTimeout(r, ms));

/** Ask the backend whether Ollama is up; load models, or show the banner and keep polling. */
let recheck = false; // a check was requested while one was in flight (e.g. the Ollama URL changed)

export async function checkStatus() {
  if (connecting) { recheck = true; return; }
  setStatus(t("statusConnecting"));
  connecting = true;
  let status;
  do {
    recheck = false;
    status = await api.getStatus().catch(() => null);
  } while (recheck);
  connecting = false;
  await applyStatus(status);
}

/** Render `status` (null = backend unreachable); start polling while Ollama is down. */
async function applyStatus(status) {
  checked = true;
  state.status = status;
  if (status?.ollama) {
    showBanner(false);
    setStatus(t("statusConnected") + (status.version ? ` · Ollama ${status.version}` : ""), "ok");
    await loadModels(); // also sets the "no chat model" status when needed
    return;
  }
  showBanner(!!status);
  setStatus(status ? t("ollamaDown") : t("backendDown"), "err");
  clearModels();
  pollUntilUp();
}

let polling = false;

/** Keep checking while Ollama (or the backend) is down, backing off from 2.5 s to 15 s. */
async function pollUntilUp() {
  if (polling) return;
  polling = true;
  let delay = POLL_FIRST_MS;
  try {
    for (;;) {
      await sleep(delay);
      delay = Math.min(POLL_MAX_MS, Math.round(delay * 1.5));
      if (state.status?.ollama) return; // someone else already saw it come back
      if (connecting) continue;
      const status = await api.getStatus().catch(() => null);
      if (status?.ollama) {
        polling = false;
        await applyStatus(status);
        return;
      }
      if (!!status !== !!state.status) { // backend came back (or went away) while Ollama is still down
        state.status = status;
        showBanner(!!status);
        setStatus(status ? t("ollamaDown") : t("backendDown"), "err");
      }
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
  else if (!chatModels().length) setStatus(t("noChatModels"), "err");
  else setStatus(t("statusConnected") + (s.version ? ` · Ollama ${s.version}` : ""), "ok");
  const btn = $("#startOllama");
  if (btn.disabled) btn.textContent = t("startingOllama");
}

let startRun = 0; // id of the latest "Start Ollama" attempt

async function startOllama() {
  const btn = $("#startOllama");
  const run = ++startRun;
  let gaveUp = false;
  btn.disabled = true;
  btn.textContent = t("startingOllama");
  const restoreBtn = () => { btn.disabled = false; btn.textContent = t("startOllama"); };
  // Don't leave the button stuck on "Starting…" (e.g. an unreachable/remote URL): give it back
  // after 20 s; the request keeps going and the poller picks Ollama up if it does start.
  const slow = setTimeout(() => {
    gaveUp = true;
    if (run === startRun) restoreBtn();
    toast(t("startSlow"), "error", 8000);
  }, START_SLOW_MS);
  try {
    const res = await api.startOllama();
    if (!res?.ok && !gaveUp) toast(t("startFailed"), "error");
  } catch (e) {
    if (!gaveUp) toast(`${t("startFailed")} (${e.message})`, "error");
  } finally {
    clearTimeout(slow);
    if (run === startRun) restoreBtn();
    await checkStatus();
  }
}

export default {
  name: "status",
  init() {
    $("#startOllama").addEventListener("click", startOllama);
    on("ollama-url-changed", checkStatus);
    // Emitted by other features (chat) on network/502 errors: re-check right away.
    on("ollama-maybe-down", () => { if (!polling) checkStatus(); });
    on("language", renderStatusText);
    on("models", renderStatusText); // e.g. a chat model was just downloaded / deleted
  },
  // Models depend on settings.default_model, so wait for settings first.
  async start() {
    await settingsLoaded;
    await checkStatus();
  },
};
