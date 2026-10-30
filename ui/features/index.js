// features/index.js — the ONLY list of UI features. main.js loads them in this order.
//
// Each entry is a feature's default export: { name, init?(), start?(), onEscape?() }
//   init()     sync wiring (event listeners, static rendering). Runs once, in list order.
//   start()    async initial data load. All start() hooks run in parallel after every init().
//   onEscape() return true if it handled Esc. The Esc chain asks features in list
//              order and stops at the first true, so ORDER MATTERS here:
//              model menu → modal → stop generation → preview → mobile sidebar.
// A feature's strings are registered when its module is imported; its CSS is
// linked in index.html.
//
// To add a feature: import it and add it to the list (place it after the
// existing Esc handlers unless it needs Esc priority).

import { modalsFeature } from "../core/dom.js";
import models from "./models/models.js";
import chat from "./chat/chat.js";
import preview from "./preview/preview.js";
import sidebar from "./sidebar/sidebar.js";
import settings from "./settings/settings.js";
import status from "./status/status.js";
import exporter from "./exporter/exporter.js";
import code from "./code/code.js";
import assistants from "./assistants/assistants.js";
import update from "./update/update.js";

export default [
  models,        // Esc 1: model dropdown
  modalsFeature, // Esc 2: top-most modal (core/dom.js)
  chat,          // Esc 3: stop generation
  preview,       // Esc 4: preview panel
  sidebar,       // Esc 5: mobile slide-over sidebar
  settings,
  status,
  exporter,
  code,
  assistants,
  update,
];
