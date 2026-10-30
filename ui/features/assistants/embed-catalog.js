// embed-catalog.js — suggested Ollama embedding models for assistants' knowledge search,
// and a hardware verdict for each (reuses the models feature's rateModel()).
// Entries: name (exact Ollama tag), sizeGb (download size, ollama.com/library, Oct 2026),
// params, arabic (verygood | good | ok | weak), recommended, desc { ar, en }.
// Cross-feature deps: models/catalog.js (rateModel — pure, exported).

import { rateModel } from "../models/catalog.js";
import { getLanguage } from "../../core/i18n.js";

export const DEFAULT_EMBED = "bge-m3";

export const EMBED_CATALOG = [
  { name: "bge-m3", sizeGb: 1.2, params: "0.57B", arabic: "verygood", recommended: true,
    desc: {
      ar: "متعدد اللغات وقوي جدًا في العربية والبحث بين العربية والإنجليزية، ويقبل مقاطع طويلة. الخيار الموصى به لمعظم المساعدين.",
      en: "Multilingual and very strong in Arabic and Arabic↔English search, handles long passages. The recommended choice for most assistants." } },
  { name: "qwen3-embedding:0.6b", sizeGb: 0.639, params: "0.6B", arabic: "good",
    desc: {
      ar: "صغير وسريع ومتعدد اللغات (يدعم العربية جيدًا). بديل خفيف لـ bge-m3 على الأجهزة الضعيفة.",
      en: "Small, fast and multilingual (good Arabic). A light alternative to bge-m3 on weaker computers." } },
  { name: "qwen3-embedding:4b", sizeGb: 2.5, params: "4B", arabic: "good",
    desc: {
      ar: "أدق من 0.6b في البحث متعدد اللغات، لكنه أبطأ في الفهرسة ويأخذ من ذاكرة كرت الشاشة بجانب موديل المحادثة.",
      en: "More accurate than 0.6b for multilingual search, but slower to index and it shares GPU memory with the chat model." } },
  { name: "qwen3-embedding:8b", sizeGb: 4.7, params: "8B", arabic: "good",
    desc: {
      ar: "من أدق موديلات الفهرسة متعددة اللغات، لكنه كبير: مع موديل محادثة 8B لا يتسع الاثنان في كرت 8 GB فيتبادلان الذاكرة ويبطؤ كل سؤال.",
      en: "One of the most accurate multilingual embedding models, but large: next to an 8B chat model both don't fit an 8 GB card, so they swap and every question gets slower." } },
  { name: "snowflake-arctic-embed2", sizeGb: 1.2, params: "0.57B", arabic: "ok",
    desc: {
      ar: "متعدد اللغات وممتاز في الإنجليزية؛ عربيته مقبولة. مناسب إذا كانت معظم المستندات بالإنجليزية.",
      en: "Multilingual and excellent in English; fair Arabic. Good when most documents are in English." } },
  { name: "granite-embedding:278m", sizeGb: 0.563, params: "0.28B", arabic: "ok",
    desc: {
      ar: "من IBM، متعدد اللغات وخفيف؛ عربيته مقبولة. بديل صغير عندما تكون الذاكرة محدودة.",
      en: "From IBM, multilingual and light; fair Arabic. A small option when memory is tight." } },
  { name: "embeddinggemma", sizeGb: 0.622, params: "0.3B", arabic: "ok",
    desc: {
      ar: "من Google، صغير ومدرَّب على أكثر من 100 لغة؛ عربيته مقبولة.",
      en: "From Google, small and trained on 100+ languages; fair Arabic." } },
  { name: "nomic-embed-text", sizeGb: 0.274, params: "0.14B", arabic: "weak",
    desc: {
      ar: "سريع جدًا وجيد للنصوص الإنجليزية، لكنه ضعيف في العربية: لا ننصح به لمستندات عربية.",
      en: "Very fast and good for English text, but weak in Arabic: not recommended for Arabic documents." } },
  { name: "mxbai-embed-large", sizeGb: 0.67, params: "0.34B", arabic: "weak",
    desc: {
      ar: "دقيق جدًا في الإنجليزية فقط؛ ضعيف في العربية والبحث بين اللغتين.",
      en: "Very accurate in English only; weak in Arabic and in cross-language search." } },
  { name: "all-minilm", sizeGb: 0.046, params: "0.02B", arabic: "weak",
    desc: {
      ar: "صغير جدًا (45 MB) وسريع، للإنجليزية فقط؛ نتائجه في العربية ضعيفة.",
      en: "Tiny (45 MB) and fast, English only; poor results in Arabic." } },
];

/** Catalog entry for a tag (":latest" aware), or null. */
export function findEmbed(name) {
  const n = String(name || "").trim().replace(/:latest$/, "");
  return EMBED_CATALOG.find(m => m.name === n) || null;
}

/**
 * String key of a catalog entry's Arabic-support label: the models feature's `arabic_*`
 * (good / ok / weak), or this feature's `asst_arabic_verygood` (bge-m3, as its card says).
 */
export const arabicKey = m => (m.arabic === "verygood" ? "asst_arabic_verygood" : `arabic_${m.arabic}`);

/** A catalog entry's description in the UI language. */
export const describeEmbed = m => m.desc?.[getLanguage()] ?? m.desc?.ar ?? "";

/**
 * Verdict for an embedding model on this computer (null if `system` is unknown). The embedding
 * model runs next to the assistant's chat model for every question, so both sizes count:
 * rateModel() of the models feature is applied to their sum.
 */
export function rateEmbed(m, system, chatGb = 0) {
  return rateModel({ name: m.name, sizeGb: Math.round((m.sizeGb + chatGb) * 10) / 10, params: m.params, moe: false }, system);
}
