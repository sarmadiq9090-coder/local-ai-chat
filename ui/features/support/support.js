// support.js — "Support" section appended to the settings dialog (listens for the
// "settings-open" event from features/settings): the support e-mail, the support web
// page (Arabic or English by UI language) and the open-source licence notices
// (GET /api/app/notices, THIRD-PARTY-NOTICES.txt embedded in the exe).
// Links use fixed addresses only. The native window opens http(s) links in the default
// browser and allows exactly SUPPORT_EMAIL as a mailto: link (src/window.rs).
// Cross-feature deps: settings ("settings-open" event), core/api (getNotices).

import { on } from "../../core/state.js";
import * as api from "../../core/api.js";
import { el, openModal, toast } from "../../core/dom.js";
import { t, registerStrings, getLanguage } from "../../core/i18n.js";
import strings from "./strings.js";

registerStrings("support", strings);

const SUPPORT_EMAIL = "support@fastkiwi.app"; // must match SUPPORT_MAILTO in src/window.rs
const SUPPORT_PAGE = { ar: "https://fastkiwi.app/ar/support/", en: "https://fastkiwi.app/support/" };

async function openNotices(btn) {
  btn.disabled = true;
  try {
    const text = await api.getNotices();
    openModal({
      title: t("sup_licencesTitle"),
      className: "wide sup-notices",
      body: el("pre", { class: "sup-notices-text", dir: "ltr", tabindex: "0" }, text),
    });
  } catch (e) {
    toast(t("sup_licencesFailed", { error: e.message }), "error");
  } finally {
    btn.disabled = false;
  }
}

/** "Support" section for the settings form (see features/settings "settings-open"). */
function renderSettingsSection({ form } = {}) {
  if (!form) return;
  const page = SUPPORT_PAGE[getLanguage()] ?? SUPPORT_PAGE.en;
  const licences = el("button", { class: "btn small", type: "button" }, t("sup_licences"));
  licences.addEventListener("click", () => openNotices(licences));
  form.append(el("div", { class: "field sup-section" },
    el("span", { class: "field-label" }, t("sup_section")),
    el("p", { class: "sup-text" }, t("sup_text")),
    el("div", { class: "sup-links" },
      // No target: mailto goes through the window's navigation handler (src/window.rs).
      el("a", { class: "sup-link", href: `mailto:${SUPPORT_EMAIL}`, dir: "ltr" }, SUPPORT_EMAIL),
      el("a", { class: "sup-link", href: page, target: "_blank", rel: "noopener noreferrer" }, t("sup_page")),
      licences)));
}

export default {
  name: "support",
  init() {
    on("settings-open", renderSettingsSection);
  },
};
