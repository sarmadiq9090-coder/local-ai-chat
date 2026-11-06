// catalog.js — suggested Ollama models + a pure hardware rating for each.
// `CATALOG` entries: name (exact Ollama tag), sizeGb (download size), params,
// moe (+ activeParams), category (chat | code | reasoning | vision),
// arabic (good | ok | weak), desc ({ ar, en }: what it's good at; read it via describe()).
// rateModel(model, system) rates a model against GET /api/app/system.

import { t, getLanguage, localizeDigits } from "../../core/i18n.js";

export const CATEGORIES = ["chat", "code", "reasoning", "vision"];
export const LEVELS = ["best", "good", "slow", "no"];

/** A catalog model's description in the active UI language (Arabic fallback). */
export const describe = model => model.desc?.[getLanguage()] ?? model.desc?.ar ?? "";

export const CATALOG = [
  // ---------- chat ----------
  { name: "qwen3:4b", sizeGb: 2.5, params: "4B", moe: false, category: "chat", arabic: "ok",
    desc: {
      ar: "صغير وسريع جدًا، مناسب للأجهزة الضعيفة والأسئلة اليومية، ويدعم وضع التفكير.",
      en: "Small and very fast, good for weak computers and everyday questions; supports thinking mode." } },
  { name: "qwen3:8b", sizeGb: 5.2, params: "8B", moe: false, category: "chat", arabic: "good",
    desc: {
      ar: "أفضل توازن بين الجودة والسرعة للاستخدام العام، عربيته جيدة ويدعم وضع التفكير.",
      en: "The best balance of quality and speed for general use; good Arabic and supports thinking mode." } },
  { name: "qwen3:14b", sizeGb: 9.3, params: "14B", moe: false, category: "chat", arabic: "good",
    desc: {
      ar: "أذكى من 8b في الكتابة والتحليل والعربية، يحتاج كرت شاشة بذاكرة 12 GB أو أكثر ليكون سريعًا.",
      en: "Smarter than 8b at writing, analysis and Arabic; needs a graphics card with 12 GB or more to be fast." } },
  { name: "qwen3:30b", sizeGb: 19, params: "30B", moe: true, activeParams: "3B", category: "chat", arabic: "good",
    desc: {
      ar: "جودة نموذج كبير بسرعة نموذج صغير (MoE)، ممتاز للمحادثة العامة والعربية ويعمل جيدًا حتى لو لم يتسع في الكرت.",
      en: "Big-model quality at small-model speed (MoE); excellent for general chat and Arabic, and runs well even when it doesn't fit on the card." } },
  { name: "qwen3:32b", sizeGb: 20, params: "32B", moe: false, category: "chat", arabic: "good",
    desc: {
      ar: "من أقوى النماذج المحلية في الفهم والكتابة، لكنه كثيف (Dense) فيحتاج كرتًا بذاكرة 24 GB ليكون سريعًا.",
      en: "One of the strongest local models for understanding and writing, but it is dense, so it needs a 24 GB card to be fast." } },
  { name: "llama3.2:3b", sizeGb: 2.0, params: "3B", moe: false, category: "chat", arabic: "weak",
    desc: {
      ar: "صغير وخفيف جدًا للمهام البسيطة والتلخيص بالإنجليزية؛ عربيته ضعيفة.",
      en: "Very small and light, for simple tasks and summaries in English; weak Arabic." } },
  { name: "llama3.1:8b", sizeGb: 4.9, params: "8B", moe: false, category: "chat", arabic: "ok",
    desc: {
      ar: "نموذج عام مشهور ومستقر، جيد بالإنجليزية ومقبول بالعربية.",
      en: "A popular, stable general model; good in English and fair in Arabic." } },
  { name: "mistral-small3.2:24b", sizeGb: 15, params: "24B", moe: false, category: "chat", arabic: "ok",
    desc: {
      ar: "نموذج متوسط قوي في اتباع التعليمات واستدعاء الأدوات، ويقرأ الصور أيضًا.",
      en: "A strong mid-size model at following instructions and calling tools; it can read images too." } },

  // ---------- code ----------
  { name: "qwen2.5-coder:7b", sizeGb: 4.7, params: "7B", moe: false, category: "code", arabic: "ok",
    desc: {
      ar: "مساعد برمجة سريع لكتابة الأكواد وشرحها وإكمالها، مناسب لمعظم الأجهزة.",
      en: "A fast coding assistant for writing, explaining and completing code; suits most computers." } },
  { name: "qwen2.5-coder:14b", sizeGb: 9.0, params: "14B", moe: false, category: "code", arabic: "ok",
    desc: {
      ar: "أدق من 7b في البرمجة وإصلاح الأخطاء، يحتاج كرتًا بذاكرة 12 GB ليكون سريعًا.",
      en: "More accurate than 7b at coding and fixing bugs; needs a 12 GB card to be fast." } },
  { name: "qwen2.5-coder:32b", sizeGb: 20, params: "32B", moe: false, category: "code", arabic: "ok",
    desc: {
      ar: "من أقوى نماذج البرمجة المحلية، لكنه كثيف وثقيل ويحتاج كرتًا بذاكرة 24 GB.",
      en: "One of the strongest local coding models, but dense and heavy; needs a 24 GB card." } },
  { name: "qwen3-coder:30b", sizeGb: 19, params: "30B", moe: true, activeParams: "3.3B", category: "code", arabic: "ok",
    desc: {
      ar: "أحدث نموذج برمجة من Qwen، قوي في المشاريع الكبيرة والوكلاء، وسريع لأنه MoE.",
      en: "Qwen's newest coding model, strong on large projects and agents, and fast because it is MoE." } },

  // ---------- reasoning ----------
  { name: "gpt-oss:20b", sizeGb: 14, params: "21B", moe: true, activeParams: "3.6B", category: "reasoning", arabic: "ok",
    desc: {
      ar: "نموذج OpenAI المفتوح، تفكير واستدلال قوي في الرياضيات والبرمجة، وسريع لأنه MoE.",
      en: "OpenAI's open model with strong reasoning in math and coding, and fast because it is MoE." } },
  { name: "gpt-oss:120b", sizeGb: 65, params: "117B", moe: true, activeParams: "5.1B", category: "reasoning", arabic: "good",
    desc: {
      ar: "النسخة الكبيرة من gpt-oss، قريب من النماذج السحابية لكنه يحتاج ذاكرة ضخمة تقارب 80 GB.",
      en: "The large gpt-oss, close to cloud models but needs a huge amount of memory (about 80 GB)." } },
  { name: "deepseek-r1:8b", sizeGb: 5.2, params: "8B", moe: false, category: "reasoning", arabic: "ok",
    desc: {
      ar: "يفكر خطوة بخطوة قبل الإجابة، جيد لمسائل الرياضيات والمنطق على الأجهزة المتوسطة.",
      en: "Thinks step by step before answering; good for math and logic problems on mid-range computers." } },
  { name: "deepseek-r1:14b", sizeGb: 9.0, params: "14B", moe: false, category: "reasoning", arabic: "ok",
    desc: {
      ar: "استدلال أعمق من 8b في المسائل الصعبة، لكنه أبطأ ويحتاج كرتًا أكبر.",
      en: "Deeper reasoning than 8b on hard problems, but slower and needs a bigger card." } },
  { name: "phi4:14b", sizeGb: 9.1, params: "14B", moe: false, category: "reasoning", arabic: "weak",
    desc: {
      ar: "نموذج Microsoft، قوي في الرياضيات والمنطق بالإنجليزية؛ عربيته ضعيفة.",
      en: "Microsoft's model, strong at math and logic in English; weak Arabic." } },

  // ---------- vision ----------
  { name: "gemma3:4b", sizeGb: 3.3, params: "4B", moe: false, category: "vision", arabic: "ok",
    desc: {
      ar: "نموذج Google الصغير، يفهم الصور ويصفها ويدعم لغات كثيرة، خفيف على الجهاز.",
      en: "Google's small model; understands and describes images, supports many languages, and is light on your computer." } },
  { name: "gemma3:12b", sizeGb: 8.1, params: "12B", moe: false, category: "vision", arabic: "good",
    desc: {
      ar: "توازن ممتاز لقراءة الصور والمحادثة متعددة اللغات، وعربيته جيدة.",
      en: "An excellent balance for reading images and multilingual chat, with good Arabic." } },
  { name: "gemma3:27b", sizeGb: 17, params: "27B", moe: false, category: "vision", arabic: "good",
    desc: {
      ar: "أقوى نسخة من Gemma 3 في فهم الصور والكتابة، لكنه كثيف ويحتاج كرتًا بذاكرة 24 GB.",
      en: "The strongest Gemma 3 at understanding images and writing, but dense; needs a 24 GB card." } },
  { name: "qwen2.5vl:7b", sizeGb: 6.0, params: "7B", moe: false, category: "vision", arabic: "ok",
    desc: {
      ar: "متخصص في قراءة الصور والمستندات والجداول واستخراج النصوص منها (OCR).",
      en: "Specialized in reading images, documents and tables and extracting their text (OCR)." } },
];

// Integrated GPUs share system RAM and don't speed up Ollama much: ignore them.
const INTEGRATED = /intel\(r\) (uhd|hd|iris)|intel.*(uhd|hd) graphics|radeon\(tm\) graphics|^amd radeon graphics|radeon \d{3}m|microsoft basic|virtual|parsec|remote/i;
const MIN_VRAM = 2; // GB; below this a GPU can't hold any useful model

/** Largest VRAM (GB) among discrete GPUs, or 0 when unknown/none. Pure. */
export function usableVram(system) {
  return Math.max(0, ...(system?.gpus || [])
    .filter(g => g.vram_gb >= MIN_VRAM && !INTEGRATED.test(g.name))
    .map(g => g.vram_gb));
}

/** Discrete GPUs (for the hardware summary line). Pure. */
export const discreteGpus = system => (system?.gpus || []).filter(g => !INTEGRATED.test(g.name));

const CONTEXT_GB = 1.5;   // KV cache + runtime overhead for a normal context
const RAM_SHARE = 0.6;    // part of RAM we'd let a model use (OS + apps need the rest)

/**
 * Rate how well `model` runs on `system` (from /api/app/system). Pure.
 * Returns { level, label, short, reason } or null if `system` is unknown.
 *
 * With a discrete GPU (vram = largest discrete VRAM):
 *  - best: sizeGb + 1.5 ≤ vram               → fully on the GPU, fast.
 *  - good: MoE and sizeGb ≤ vram + 0.6·RAM   → split GPU/RAM, still quick because
 *          only a few billion params are active per token.
 *  - slow: dense and sizeGb ≤ vram + 0.6·RAM → split GPU/RAM, every token touches
 *          all weights in RAM, so it crawls.
 *  - no:   otherwise.
 * Without VRAM info (no NVIDIA/discrete GPU detected) we rate by RAM only (CPU):
 *  - good: fits in 0.6·RAM and is small (≤ 4B) or MoE; slow: fits but dense; no: doesn't fit.
 */
export function rateModel(model, system) {
  if (!system) return null;
  const ram = Number(system.ram_gb) || 0;
  const vram = usableVram(system);
  const need = model.sizeGb + CONTEXT_GB;
  const fitsShared = model.sizeGb <= vram + RAM_SHARE * ram;
  // LTR isolates keep "8 GB" in order inside Arabic sentences.
  const ltr = x => `\u2066${x}\u2069`;
  const budget = vram + RAM_SHARE * ram; // what fitsShared compares against (shown in "no" reasons)
  const vars = { vram: ltr(`${fmtGb(vram)} GB`), ram: ltr(`${fmtGb(ram)} GB`), need: ltr(`${fmtGb(need)} GB`),
    budget: ltr(`${fmtGb(budget)} GB`), active: ltr(model.activeParams || "") };
  const make = (level, reasonKey) => ({
    level,
    label: t(`rate_${level}`),
    short: t(`rateShort_${level}`),
    reason: t(reasonKey, vars),
  });

  if (vram > 0) {
    if (need <= vram) return make("best", "rateReason_best");
    if (fitsShared) return model.moe ? make("good", "rateReason_good") : make("slow", "rateReason_slow");
    return make("no", "rateReason_no");
  }
  // CPU / RAM only
  if (!fitsShared) return make("no", "rateReasonCpu_no");
  const small = model.moe || parseFloat(model.params) <= 4;
  return small ? make("good", "rateReasonCpu_good") : make("slow", "rateReasonCpu_slow");
}

/** 8 → "8", 63.8 → "64", 9.3 → "9.3" (GB, for messages). */
function fmtGb(x) {
  return localizeDigits(x >= 20 ? Math.round(x) : Math.round(x * 10) / 10);
}
