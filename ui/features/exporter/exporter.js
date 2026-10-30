// exporter.js — turn a conversation into a Markdown document and hand it to the
// backend (POST /api/app/export), which writes it under Documents\LocalAiChat\.

import * as api from "./api.js";
import { toast } from "./dom.js";
import { t } from "./i18n.js";

/** Longest run of backticks in `s` + 1 (min 3) so fences never break. Pure. */
export function fenceFor(s) {
  const runs = String(s).match(/`+/g) || [];
  return "`".repeat(Math.max(3, ...runs.map(r => r.length + 1)));
}

/** Conversation → Markdown string. Pure. */
export function conversationToMarkdown(conv) {
  const out = [`# ${conv.title || t("untitled")}`, ""];
  if (conv.created_at) out.push(`*${new Date(conv.created_at).toLocaleString("ar")}*`, "");
  for (const m of conv.messages || []) {
    if (m.role === "user") {
      out.push(`## ${t("you")}`, "", m.content || "", "");
      for (const a of m.attachments || []) {
        const f = fenceFor(a.content);
        out.push(`**${t("fileHeader")}: ${a.name}**`, "", f, a.content, f, "");
      }
    } else {
      out.push(`## ${m.model || "AI"}`, "");
      if (m.content) out.push(m.content, "");
      if (m.error) out.push(`> ${m.error}`, "");
    }
  }
  return out.join("\n");
}

/** Safe file name from a title. Pure. */
export const fileNameFor = title =>
  (String(title || "chat").replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_").trim().slice(0, 80) || "chat") + ".md";

/** Export the conversation and toast the saved path. */
export async function exportConversation(conv) {
  if (!conv?.messages?.length) { toast(t("exportEmpty")); return; }
  try {
    const res = await api.exportMarkdown(fileNameFor(conv.title), conversationToMarkdown(conv));
    toast(t("exported", { path: res?.path ?? "" }), "success", 7000);
  } catch (e) {
    toast(t("exportFailed", { error: e.message }), "error");
  }
}
