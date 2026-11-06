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

// Tags and download sizes checked on ollama.com/library on 2026-11-06 (default Q4 builds).
// The Arabic level is our own rough rating for the UI, not a benchmark.
export const CATALOG = [
  // ---------- chat ----------
  { name: "qwen3.5:2b", sizeGb: 2.7, params: "2B", moe: false, category: "chat", arabic: "weak",
    desc: {
      ar: "صغير جدًا وسريع، للأجهزة الضعيفة والأسئلة البسيطة؛ عربيته محدودة.",
      en: "Very small and fast, for weak computers and simple questions; limited Arabic." } },
  { name: "qwen3.5:4b", sizeGb: 3.3, params: "4B", moe: false, category: "chat", arabic: "ok",
    desc: {
      ar: "صغير وسريع للأسئلة اليومية، يقرأ الصور ويدعم وضع التفكير، ويعمل حتى بدون كرت شاشة.",
      en: "Small and fast for everyday questions; reads images, supports thinking mode and runs even without a graphics card." } },
  { name: "qwen3.5:9b", sizeGb: 6.6, params: "9B", moe: false, category: "chat", arabic: "good",
    desc: {
      ar: "أفضل توازن بين الجودة والسرعة للاستخدام العام، عربيته جيدة ويقرأ الصور ويدعم وضع التفكير.",
      en: "The best balance of quality and speed for general use; good Arabic, reads images and supports thinking mode." } },
  { name: "ministral-3:8b", sizeGb: 6.0, params: "8B", moe: false, category: "chat", arabic: "ok",
    desc: {
      ar: "نموذج Mistral للأجهزة العادية، جيد في اتباع التعليمات واستدعاء الأدوات، ويقرأ الصور.",
      en: "Mistral's model for ordinary computers; good at following instructions and calling tools, and reads images." } },
  { name: "granite4.2:8b", sizeGb: 5.3, params: "8B", moe: false, category: "chat", arabic: "ok",
    desc: {
      ar: "نموذج IBM المفتوح (Apache 2.0)، مستقر في التلخيص والإجابة من المستندات والأدوات، ويدعم العربية.",
      en: "IBM's open model (Apache 2.0), steady at summaries, answering from documents and tools; supports Arabic." } },
  { name: "gemma4:26b", sizeGb: 16, params: "26B", moe: true, activeParams: "3.8B", category: "chat", arabic: "good",
    desc: {
      ar: "نموذج Google بجودة نموذج كبير وسرعة نموذج صغير (MoE)، ممتاز للمحادثة والعربية ويقرأ الصور، ويعمل جيدًا حتى لو لم يتسع في الكرت.",
      en: "Google's model with big-model quality at small-model speed (MoE); excellent for chat and Arabic, reads images, and runs well even when it doesn't fit on the card." } },
  { name: "qwen3.5:35b", sizeGb: 22, params: "35B", moe: true, activeParams: "3B", category: "chat", arabic: "good",
    desc: {
      ar: "نسخة MoE كبيرة من Qwen 3.5، قوية في الكتابة والتحليل والعربية وسريعة نسبيًا، لكنها تحتاج ذاكرة كثيرة (32 GB أو أكثر).",
      en: "A large MoE Qwen 3.5, strong at writing, analysis and Arabic and fairly quick, but it needs a lot of memory (32 GB or more)." } },
  { name: "qwen3.8:27b", sizeGb: 18, params: "27B", moe: false, category: "chat", arabic: "good",
    desc: {
      ar: "أحدث نموذج من Qwen، من أقوى النماذج المحلية في الفهم والكتابة، لكنه كثيف (Dense) فيحتاج كرتًا بذاكرة 24 GB ليكون سريعًا.",
      en: "Qwen's newest model, one of the strongest local models for understanding and writing, but dense, so it needs a 24 GB card to be fast." } },
  { name: "gemma4:31b", sizeGb: 19, params: "31B", moe: false, category: "chat", arabic: "good",
    desc: {
      ar: "أقوى نسخة من Gemma 4 في الكتابة والعربية وفهم الصور، لكنه كثيف ويحتاج كرتًا بذاكرة 24 GB.",
      en: "The strongest Gemma 4 for writing, Arabic and understanding images, but dense; needs a 24 GB card." } },

  // ---------- code ----------
  { name: "qwen2.5-coder:7b", sizeGb: 4.7, params: "7B", moe: false, category: "code", arabic: "ok",
    desc: {
      ar: "مساعد برمجة سريع لكتابة الأكواد وشرحها وإكمالها، مناسب لمعظم الأجهزة.",
      en: "A fast coding assistant for writing, explaining and completing code; suits most computers." } },
  { name: "qwen2.5-coder:14b", sizeGb: 9.0, params: "14B", moe: false, category: "code", arabic: "ok",
    desc: {
      ar: "أدق من 7b في البرمجة وإصلاح الأخطاء، يحتاج كرتًا بذاكرة 12 GB ليكون سريعًا.",
      en: "More accurate than 7b at coding and fixing bugs; needs a 12 GB card to be fast." } },
  { name: "devstral-small-2:24b", sizeGb: 15, params: "24B", moe: false, category: "code", arabic: "ok",
    desc: {
      ar: "نموذج برمجة من Mistral للمشاريع والوكلاء، يقرأ الصور أيضًا، لكنه كثيف ويحتاج كرتًا بذاكرة 16 GB أو أكثر.",
      en: "Mistral's coding model for projects and agents, reads images too, but dense; needs a 16 GB card or more." } },
  { name: "qwen3-coder:30b", sizeGb: 19, params: "30B", moe: true, activeParams: "3.3B", category: "code", arabic: "ok",
    desc: {
      ar: "نموذج برمجة من Qwen، قوي في المشاريع الكبيرة والوكلاء، وسريع لأنه MoE.",
      en: "Qwen's coding model, strong on large projects and agents, and fast because it is MoE." } },
  { name: "qwen3.6:35b", sizeGb: 23, params: "35B", moe: true, activeParams: "3B", category: "code", arabic: "good",
    desc: {
      ar: "Qwen 3.6 مُحسَّن للبرمجة والوكلاء، ويصلح للمحادثة أيضًا، وسريع لأنه MoE، لكنه يحتاج ذاكرة كثيرة.",
      en: "Qwen 3.6, tuned for coding and agents and fine for chat too; fast because it is MoE, but needs a lot of memory." } },

  // ---------- reasoning ----------
  { name: "gpt-oss:20b", sizeGb: 14, params: "21B", moe: true, activeParams: "3.6B", category: "reasoning", arabic: "ok",
    desc: {
      ar: "نموذج OpenAI المفتوح، تفكير واستدلال قوي في الرياضيات والبرمجة، وسريع لأنه MoE.",
      en: "OpenAI's open model with strong reasoning in math and coding, and fast because it is MoE." } },
  { name: "gpt-oss:120b", sizeGb: 65, params: "117B", moe: true, activeParams: "5.1B", category: "reasoning", arabic: "good",
    desc: {
      ar: "النسخة الكبيرة من gpt-oss، قريب من النماذج السحابية لكنه يحتاج ذاكرة ضخمة (80 GB تقريبًا بين الكرت والذاكرة).",
      en: "The large gpt-oss, close to cloud models but needs a huge amount of memory (about 80 GB across card and RAM)." } },
  { name: "deepseek-r1:8b", sizeGb: 5.2, params: "8B", moe: false, category: "reasoning", arabic: "ok",
    desc: {
      ar: "يفكر خطوة بخطوة قبل الإجابة، جيد لمسائل الرياضيات والمنطق على الأجهزة المتوسطة.",
      en: "Thinks step by step before answering; good for math and logic problems on mid-range computers." } },
  { name: "deepseek-r1:14b", sizeGb: 9.0, params: "14B", moe: false, category: "reasoning", arabic: "ok",
    desc: {
      ar: "استدلال أعمق من 8b في المسائل الصعبة، لكنه أبطأ ويحتاج كرتًا بذاكرة 12 GB.",
      en: "Deeper reasoning than 8b on hard problems, but slower and needs a 12 GB card." } },

  // ---------- vision ----------
  { name: "gemma4:e2b", sizeGb: 4.6, params: "2B (E2B)", moe: false, category: "vision", arabic: "ok",
    desc: {
      ar: "أصغر Gemma 4، مصمم للأجهزة الضعيفة: يفهم الصور ويصفها، ويدعم لغات كثيرة.",
      en: "The smallest Gemma 4, made for weak computers: understands and describes images, in many languages." } },
  { name: "gemma4:e4b", sizeGb: 6.6, params: "4B (E4B)", moe: false, category: "vision", arabic: "ok",
    desc: {
      ar: "نموذج Google الصغير، يفهم الصور ويصفها، أقوى من e2b ويبقى خفيفًا.",
      en: "Google's small model; understands and describes images; stronger than e2b and still light." } },
  { name: "gemma4:12b", sizeGb: 7.7, params: "12B", moe: false, category: "vision", arabic: "good",
    desc: {
      ar: "توازن ممتاز لقراءة الصور والمحادثة بلغات كثيرة، مع عربية جيدة، ويناسب كرتًا بذاكرة 8–12 GB.",
      en: "An excellent balance for reading images and multilingual chat, with good Arabic; suits an 8–12 GB card." } },
  { name: "qwen3-vl:8b", sizeGb: 6.2, params: "8B", moe: false, category: "vision", arabic: "ok",
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
