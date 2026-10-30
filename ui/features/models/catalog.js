// catalog.js — suggested Ollama models + a pure hardware rating for each.
// `CATALOG` entries: name (exact Ollama tag), sizeGb (download size), params,
// moe (+ activeParams), category (chat | code | reasoning | vision),
// arabic (good | ok | weak), desc (Arabic, what it's good at).
// rateModel(model, system) rates a model against GET /api/app/system.

import { t } from "./i18n.js";

export const CATEGORIES = ["chat", "code", "reasoning", "vision"];
export const LEVELS = ["best", "good", "slow", "no"];

export const CATALOG = [
  // ---------- chat ----------
  { name: "qwen3:4b", sizeGb: 2.5, params: "4B", moe: false, category: "chat", arabic: "ok",
    desc: "صغير وسريع جدًا، مناسب للأجهزة الضعيفة والأسئلة اليومية، ويدعم وضع التفكير." },
  { name: "qwen3:8b", sizeGb: 5.2, params: "8B", moe: false, category: "chat", arabic: "good",
    desc: "أفضل توازن بين الجودة والسرعة للاستخدام العام، عربيته جيدة ويدعم وضع التفكير." },
  { name: "qwen3:14b", sizeGb: 9.3, params: "14B", moe: false, category: "chat", arabic: "good",
    desc: "أذكى من 8b في الكتابة والتحليل والعربية، يحتاج كرت شاشة بذاكرة 12 GB أو أكثر ليكون سريعًا." },
  { name: "qwen3:30b", sizeGb: 19, params: "30B", moe: true, activeParams: "3B", category: "chat", arabic: "good",
    desc: "جودة نموذج كبير بسرعة نموذج صغير (MoE)، ممتاز للمحادثة العامة والعربية ويعمل جيدًا حتى لو لم يتسع في الكرت." },
  { name: "qwen3:32b", sizeGb: 20, params: "32B", moe: false, category: "chat", arabic: "good",
    desc: "من أقوى النماذج المحلية في الفهم والكتابة، لكنه كثيف (Dense) فيحتاج كرتًا بذاكرة 24 GB ليكون سريعًا." },
  { name: "llama3.2:3b", sizeGb: 2.0, params: "3B", moe: false, category: "chat", arabic: "weak",
    desc: "صغير وخفيف جدًا للمهام البسيطة والتلخيص بالإنجليزية؛ عربيته ضعيفة." },
  { name: "llama3.1:8b", sizeGb: 4.9, params: "8B", moe: false, category: "chat", arabic: "ok",
    desc: "نموذج عام مشهور ومستقر، جيد بالإنجليزية ومقبول بالعربية." },
  { name: "mistral-small3.2:24b", sizeGb: 15, params: "24B", moe: false, category: "chat", arabic: "ok",
    desc: "نموذج متوسط قوي في اتباع التعليمات واستدعاء الأدوات، ويقرأ الصور أيضًا." },

  // ---------- code ----------
  { name: "qwen2.5-coder:7b", sizeGb: 4.7, params: "7B", moe: false, category: "code", arabic: "ok",
    desc: "مساعد برمجة سريع لكتابة الأكواد وشرحها وإكمالها، مناسب لمعظم الأجهزة." },
  { name: "qwen2.5-coder:14b", sizeGb: 9.0, params: "14B", moe: false, category: "code", arabic: "ok",
    desc: "أدق من 7b في البرمجة وإصلاح الأخطاء، يحتاج كرتًا بذاكرة 12 GB ليكون سريعًا." },
  { name: "qwen2.5-coder:32b", sizeGb: 20, params: "32B", moe: false, category: "code", arabic: "ok",
    desc: "من أقوى نماذج البرمجة المحلية، لكنه كثيف وثقيل ويحتاج كرتًا بذاكرة 24 GB." },
  { name: "qwen3-coder:30b", sizeGb: 19, params: "30B", moe: true, activeParams: "3.3B", category: "code", arabic: "ok",
    desc: "أحدث نموذج برمجة من Qwen، قوي في المشاريع الكبيرة والوكلاء، وسريع لأنه MoE." },

  // ---------- reasoning ----------
  { name: "gpt-oss:20b", sizeGb: 14, params: "21B", moe: true, activeParams: "3.6B", category: "reasoning", arabic: "ok",
    desc: "نموذج OpenAI المفتوح، تفكير واستدلال قوي في الرياضيات والبرمجة، وسريع لأنه MoE." },
  { name: "gpt-oss:120b", sizeGb: 65, params: "117B", moe: true, activeParams: "5.1B", category: "reasoning", arabic: "good",
    desc: "النسخة الكبيرة من gpt-oss، قريب من النماذج السحابية لكنه يحتاج ذاكرة ضخمة تقارب 80 GB." },
  { name: "deepseek-r1:8b", sizeGb: 5.2, params: "8B", moe: false, category: "reasoning", arabic: "ok",
    desc: "يفكر خطوة بخطوة قبل الإجابة، جيد لمسائل الرياضيات والمنطق على الأجهزة المتوسطة." },
  { name: "deepseek-r1:14b", sizeGb: 9.0, params: "14B", moe: false, category: "reasoning", arabic: "ok",
    desc: "استدلال أعمق من 8b في المسائل الصعبة، لكنه أبطأ ويحتاج كرتًا أكبر." },
  { name: "phi4:14b", sizeGb: 9.1, params: "14B", moe: false, category: "reasoning", arabic: "weak",
    desc: "نموذج Microsoft، قوي في الرياضيات والمنطق بالإنجليزية؛ عربيته ضعيفة." },

  // ---------- vision ----------
  { name: "gemma3:4b", sizeGb: 3.3, params: "4B", moe: false, category: "vision", arabic: "ok",
    desc: "نموذج Google الصغير، يفهم الصور ويصفها ويدعم لغات كثيرة، خفيف على الجهاز." },
  { name: "gemma3:12b", sizeGb: 8.1, params: "12B", moe: false, category: "vision", arabic: "good",
    desc: "توازن ممتاز لقراءة الصور والمحادثة متعددة اللغات، وعربيته جيدة." },
  { name: "gemma3:27b", sizeGb: 17, params: "27B", moe: false, category: "vision", arabic: "good",
    desc: "أقوى نسخة من Gemma 3 في فهم الصور والكتابة، لكنه كثيف ويحتاج كرتًا بذاكرة 24 GB." },
  { name: "qwen2.5vl:7b", sizeGb: 6.0, params: "7B", moe: false, category: "vision", arabic: "ok",
    desc: "متخصص في قراءة الصور والمستندات والجداول واستخراج النصوص منها (OCR)." },
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
  const vars = { vram: ltr(`${fmtGb(vram)} GB`), ram: ltr(`${fmtGb(ram)} GB`), need: ltr(`${fmtGb(need)} GB`), active: ltr(model.activeParams || "") };
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
  return x >= 20 ? String(Math.round(x)) : String(Math.round(x * 10) / 10);
}
