// update.js — auto-update from GitHub Releases (spec: docs/specs/auto-update.md).
// - start(): if settings.auto_update, checks /api/app/update/check (and every 6 h);
//   shows a "just updated" toast once after a restart into a new version.
// - Inserts a highlighted "Update available v…" button at the top of the sidebar
//   footer (.side-foot) when a newer release exists; it opens the update modal
//   (version, date, release notes, "Update now" with an NDJSON progress bar).
// - Settings integration: listens for the "settings-open" event (emitted by
//   features/settings with the settings form) and appends an "Updates" section
//   (auto-check toggle — part of the dialog draft: saved on Save, dropped on Cancel;
//   "Check for updates" button whose result stays visible across dialog rebuilds).
// Cross-feature deps: settings (settingsLoaded, "settings-open" draft protocol), core/api (getUpdateInfo, installUpdate).

import { state, on } from "../../core/state.js";
import * as api from "../../core/api.js";
import { $, el, openModal, toast } from "../../core/dom.js";
import { t, registerStrings, formatDate, localizeDigits } from "../../core/i18n.js";
import { icon } from "../../core/icons.js";
import { renderMarkdown } from "../../core/markdown.js";
import { settingsLoaded } from "../settings/settings.js";
import strings from "./strings.js";

registerStrings("update", strings);

const RECHECK_MS = 6 * 60 * 60 * 1000;

let info = null;        // last /update/check response
let button = null;      // sidebar footer button
let installing = false;
let manualChecked = false; // the user pressed "Check for updates": keep showing the result

/** Ask the backend (it caches for 10 min unless `force`). Never throws. */
export async function checkForUpdates({ force = false } = {}) {
  try {
    info = await api.getUpdateInfo(force ? { force: 1 } : {});
  } catch (e) {
    info = { available: false, error: e.message };
  }
  handleJustUpdated(info);
  renderButton();
  return info;
}

function handleJustUpdated(res) {
  if (res?.just_updated) toast(t("upd_done_toast", { version: res.just_updated }), "success", 6000);
}

function renderButton() {
  if (!button) return;
  const show = !!info?.available && !!info.latest;
  button.hidden = !show;
  if (show) button.querySelector(".upd-label").textContent = t("upd_available", { version: info.latest });
}

const fmtBytes = n => localizeDigits(n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(0, Math.round(n / 1024))} KB`);

function fmtDate(iso) {
  const d = iso ? new Date(iso) : null;
  if (!d || isNaN(d)) return null;
  return formatDate(d, { year: "numeric", month: "long", day: "numeric" });
}

/** The update dialog: notes + "Update now" with progress. */
export function openUpdateModal() {
  if (!info?.available) return;
  const date = fmtDate(info.published_at);
  const notes = el("div", { class: "content upd-notes", dir: "auto" });
  if (info.notes?.trim()) notes.innerHTML = renderMarkdown(info.notes); // renderMarkdown escapes all text
  else notes.append(el("p", { class: "muted" }, t("upd_no_notes")));

  const bar = el("progress", { class: "upd-progress", max: 1, value: 0, hidden: true });
  const statusLine = el("div", { class: "upd-status muted", dir: "auto", hidden: true });
  const laterBtn = el("button", { class: "btn", type: "button", onclick: () => m.close() }, t("upd_later"));
  const nowBtn = el("button", { class: "btn primary", type: "button" }, t("upd_now"));

  const setStatus = (text, kind = "") => {
    statusLine.hidden = false;
    statusLine.textContent = text;
    statusLine.className = `upd-status ${kind || "muted"}`;
  };

  nowBtn.addEventListener("click", async () => {
    installing = true;
    nowBtn.disabled = laterBtn.disabled = true;
    bar.hidden = false;
    bar.removeAttribute("value"); // indeterminate until sizes arrive
    let restarting = false;
    try {
      for await (const p of api.installUpdate()) {
        if (p.status === "downloading") {
          if (p.total) {
            bar.max = p.total;
            bar.value = p.downloaded;
            setStatus(t("upd_downloading", { done: fmtBytes(p.downloaded), total: fmtBytes(p.total) }));
          } else {
            setStatus(t("upd_downloading_nototal", { done: fmtBytes(p.downloaded) }));
          }
        } else if (p.status === "verifying") {
          bar.removeAttribute("value");
          setStatus(t("upd_verifying"));
        } else if (p.status === "restarting") {
          restarting = true;
          bar.max = 1; bar.value = 1;
          setStatus(t("upd_restarting"));
        }
      }
      if (!restarting) throw new Error("connection closed");
      reloadWhenBack(); // dev (--serve) mode: the server restarts on the same port
    } catch (e) {
      installing = false;
      bar.hidden = true;
      nowBtn.disabled = laterBtn.disabled = false;
      setStatus(t("upd_failed", { error: e.message }), "err");
      toast(t("upd_failed", { error: e.message }), "error", 8000);
    }
  });

  const m = openModal({
    title: t("upd_title"),
    className: "upd-modal",
    body: el("div", { class: "upd-body" },
      el("p", { class: "upd-version", dir: "auto" }, t("upd_version", { latest: info.latest, current: info.current })),
      date && el("p", { class: "upd-date muted", dir: "auto" }, t("upd_published", { date })),
      notes, bar, statusLine),
    actions: [laterBtn, nowBtn],
    beforeClose: () => !installing,
  });
}

/** After "restarting": in the desktop app the window closes; with --serve, reload once the new server answers. */
async function reloadWhenBack() {
  await new Promise(r => setTimeout(r, 2000));
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch("/api/app/status", { cache: "no-store" });
      if (res.ok) { location.reload(); return; }
    } catch { /* server still restarting */ }
    await new Promise(r => setTimeout(r, 1000));
  }
}

/** "Updates" section appended to the settings form (see features/settings "settings-open"). */
export function renderSettingsSection({ form, draft, contribute } = {}) {
  if (!form) return;
  const auto = el("input", { type: "checkbox", id: "upd-auto", "aria-label": t("upd_auto") });
  auto.checked = (draft ?? state.settings)?.auto_update !== false;
  const wasOn = state.settings?.auto_update !== false;
  // Saved with the dialog's Save button; a newly enabled toggle triggers a check after the save.
  contribute?.(() => {
    if (auto.checked && !wasOn) setTimeout(() => { if (state.settings?.auto_update !== false) checkForUpdates(); }, 0);
    return { auto_update: auto.checked };
  });

  const result = el("span", { class: "upd-result muted", dir: "auto" });
  const view = el("button", { class: "btn small primary", type: "button", hidden: true, onclick: openUpdateModal }, t("upd_view"));
  const showResult = () => {
    view.hidden = !info?.available;
    if (info?.available) result.textContent = t("upd_available", { version: info.latest });
    else if (info?.error) result.textContent = t("upd_check_failed", { error: info.error });
    else if (info) result.textContent = t("upd_current", { version: info.current });
  };
  const checkBtn = el("button", { class: "btn small", type: "button" }, el("span", { html: icon("refresh", 15) }), t("upd_check"));
  checkBtn.addEventListener("click", async () => {
    checkBtn.disabled = true;
    result.textContent = t("upd_checking");
    view.hidden = true;
    await checkForUpdates({ force: true });
    manualChecked = true;
    checkBtn.disabled = false;
    showResult();
  });
  if (info?.available || manualChecked) showResult();

  form.append(el("div", { class: "field upd-section" },
    el("span", { class: "field-label" }, t("upd_section")),
    el("label", { class: "upd-toggle", for: "upd-auto" }, auto, el("span", {}, t("upd_auto"))),
    el("div", { class: "upd-actions" }, checkBtn, view, result)));
}

export default {
  name: "update",
  init() {
    const foot = $(".side-foot");
    if (foot) {
      button = el("button", { class: "side-btn upd-btn", type: "button", hidden: true, onclick: openUpdateModal },
        el("span", { class: "upd-icon", html: icon("download") }),
        el("span", { class: "upd-label", dir: "auto" }));
      foot.prepend(button);
    }
    on("language", renderButton);
    on("settings-open", renderSettingsSection);
  },
  async start() {
    await settingsLoaded;
    if (state.settings?.auto_update !== false) {
      checkForUpdates(); // not awaited: never delay startup on the network
    } else {
      try { handleJustUpdated(await api.getUpdateInfo({ local: 1 })); } catch { /* ignore */ }
    }
    setInterval(() => { if (state.settings?.auto_update !== false) checkForUpdates(); }, RECHECK_MS);
  },
};
