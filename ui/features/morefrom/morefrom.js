// morefrom.js — "More from FastKiwi" section appended to the settings dialog (listens for
// the "settings-open" event from features/settings): the other FastKiwi apps, each with its
// page on fastkiwi.app and, when its listing is live, a Microsoft Store link.
// The list comes from GET /api/app/more-from, a copy of the site's apps.json compiled into
// the exe (src/more_from.rs, data in ./data/, refreshed by tools/sync_apps.py): nothing is
// fetched from the internet, and the icons are the vendored files in ./data/img/.
// Links are checked again here (only https://fastkiwi.app/ pages and
// ms-windows-store://pdp/?productid=<id>); the native window opens them outside the app
// (src/window.rs). Nothing is downloaded or installed.
// Cross-feature deps: settings ("settings-open" event), core/api (getMoreFrom).

import { on } from "../../core/state.js";
import * as api from "../../core/api.js";
import { el } from "../../core/dom.js";
import { t, registerStrings, getLanguage } from "../../core/i18n.js";
import strings from "./strings.js";

registerStrings("morefrom", strings);

const STORE_URI = /^ms-windows-store:\/\/pdp\/\?productid=[0-9A-Z]{12}$/;
const ICON_PATH = /^features\/morefrom\/data\/img\/[a-z0-9-]+\.(svg|png)$/;

/** An https://fastkiwi.app/... URL (same rule as more_from::is_site_url). */
function isSiteUrl(url) {
  try {
    const u = new URL(url);
    return u.protocol === "https:" && u.hostname === "fastkiwi.app" && !u.port && !u.username && !u.password;
  } catch {
    return false;
  }
}

let cached = null; // the list is static (compiled into the exe): load it once

function renderApp(app, lang) {
  const text = field => (app[field] && (app[field][lang] ?? app[field].en)) || "";
  const page = text("url");
  const links = [];
  if (isSiteUrl(page)) {
    links.push(el("a", { class: "mf-link", href: page, target: "_blank", rel: "noopener noreferrer" }, t("mf_page")));
  }
  // No target: the custom scheme goes through the window's navigation handler (src/window.rs).
  if (!app.coming_soon && typeof app.store === "string" && STORE_URI.test(app.store)) {
    links.push(el("a", { class: "mf-link", href: app.store }, t("mf_store")));
  }
  const icon = typeof app.icon === "string" && ICON_PATH.test(app.icon)
    ? el("img", { class: "mf-icon", src: app.icon, alt: "", width: 32, height: 32, loading: "lazy", onerror: e => e.target.remove() })
    : null;
  return el("li", { class: "mf-app" },
    icon,
    el("div", { class: "mf-body" },
      el("div", { class: "mf-head" },
        el("span", { class: "mf-name", dir: "auto" }, text("name")),
        el("span", { class: "mf-tagline", dir: "auto" }, text("tagline")),
        app.coming_soon ? el("span", { class: "mf-soon" }, t("mf_soon")) : null),
      el("p", { class: "mf-desc", dir: "auto" }, text("description")),
      links.length ? el("div", { class: "mf-links" }, links) : null));
}

/** "More from FastKiwi" section for the settings form (see features/settings "settings-open"). */
function renderSettingsSection({ form } = {}) {
  if (!form) return;
  const list = el("ul", { class: "mf-list" });
  const section = el("div", { class: "field mf-section" },
    el("span", { class: "field-label" }, t("mf_section")),
    el("p", { class: "mf-text" }, t("mf_text")),
    list);
  form.append(section);
  const lang = getLanguage() === "ar" ? "ar" : "en";
  (cached ||= api.getMoreFrom().catch(e => { cached = null; throw e; }))
    .then(apps => list.replaceChildren(...(Array.isArray(apps) ? apps : []).map(a => renderApp(a, lang))))
    .catch(e => list.replaceWith(el("p", { class: "mf-error" }, t("mf_failed", { error: e.message }))));
}

export default {
  name: "morefrom",
  init() {
    on("settings-open", renderSettingsSection);
  },
};
