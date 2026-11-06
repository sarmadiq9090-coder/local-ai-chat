// crash.js — one-line notice on start when crash.txt (written by the panic hook,
// src/crash_log.rs) changed since the notice was last shown, with "Open folder".
// Shown once per crash (marked seen right away). Cross-feature deps: reuses the
// status feature's `.banner` style and is placed right after its #banner.

import * as api from "../../core/api.js";
import { $, el } from "../../core/dom.js";
import { icon } from "../../core/icons.js";
import { registerStrings, translateDom } from "../../core/i18n.js";
import strings from "./strings.js";

registerStrings("crash", strings);

export default {
  name: "crash",
  async start() {
    const info = await api.getCrashInfo().catch(() => null);
    if (!info?.new) return;
    api.markCrashSeen().catch(() => {});
    const bar = el("div", { id: "crashNotice", class: "banner", role: "status" },
      el("span", { "data-icon": "alert", html: icon("alert") }),
      el("span", { "data-i18n": "crash_notice" }),
      el("button", { class: "btn", type: "button", "data-i18n": "crash_open",
        onclick: () => api.openCrashFolder().catch(() => {}) }),
      el("button", { class: "btn", type: "button", "data-i18n": "crash_close", style: { marginInlineStart: "0" }, onclick: () => bar.remove() }));
    translateDom(bar);
    $("#banner").after(bar);
  },
};
