// i18n.js — every user-facing UI string lives here, keyed by name.
// To add a language: add a dictionary to `dictionaries` and call setLanguage().
// Static markup is translated through data-i18n / data-i18n-placeholder /
// data-i18n-title / data-i18n-aria attributes via translateDom().

const ar = {
  appTitle: "الذكاء المحلي",
  newChat: "محادثة جديدة",
  untitled: "محادثة جديدة",
  searchPlaceholder: "ابحث في المحادثات…",
  noConversations: "لا توجد محادثات بعد",
  noMatches: "لا نتائج مطابقة",
  toggleSidebar: "إظهار/إخفاء الشريط الجانبي",
  rename: "إعادة تسمية (انقر مرتين)",
  delete: "حذف",
  deleteConvTitle: "حذف المحادثة",
  deleteConvConfirm: "هل تريد حذف «{title}» نهائيًا؟",
  groupToday: "اليوم",
  groupYesterday: "أمس",
  groupWeek: "آخر 7 أيام",
  groupMonth: "آخر 30 يومًا",
  groupOlder: "أقدم",

  settings: "الإعدادات",
  models: "الموديلات",
  export: "تصدير المحادثة",
  exported: "تم الحفظ في: {path}",
  exportEmpty: "لا توجد رسائل لتصديرها",
  exportFailed: "فشل التصدير: {error}",

  modelPicker: "اختيار الموديل",
  noModels: "لا توجد موديلات مثبتة",
  noModelSelected: "اختر موديلًا أولًا (أو نزّل واحدًا من إدارة الموديلات)",
  manageModels: "إدارة الموديلات…",
  deepThinking: "تفكير عميق",
  thinkUnsupported: "هذا الموديل لا يدعم التفكير العميق",

  statusConnected: "متصل",
  statusConnecting: "جارٍ الاتصال…",
  ollamaDown: "Ollama غير شغّال",
  ollamaDownBanner: "خدمة Ollama غير شغّالة، لذلك لا يمكن المحادثة الآن.",
  startOllama: "تشغيل Ollama",
  startingOllama: "جارٍ التشغيل…",
  startFailed: "تعذّر تشغيل Ollama. تأكد من تثبيته.",
  backendDown: "تعذّر الاتصال بالخادم المحلي",

  greeting: "كيف أقدر أساعدك اليوم؟",
  greetingSub: "كل شيء يعمل محليًا على جهازك",
  inputPlaceholder: "اسأل أي شيء…",
  hint: "Enter للإرسال · Shift+Enter لسطر جديد · Ctrl+N محادثة جديدة · Esc للإيقاف",
  send: "إرسال",
  stop: "إيقاف",
  attach: "إرفاق ملفات نصية أو برمجية",
  dropHere: "أفلت الملفات هنا",
  removeAttachment: "إزالة",
  scrollToBottom: "الانتقال إلى الأسفل",
  examples: [
    { title: "برمجة", text: "اكتب دالة بلغة Python تتحقق إن كانت الكلمة متناظرة (palindrome) مع شرح بسيط." },
    { title: "فكرة لعبة", text: "اقترح فكرة لعبة ألغاز صغيرة يمكن برمجتها في أسبوع، مع وصف آلية اللعب." },
    { title: "شرح مفهوم", text: "اشرح لي الفرق بين الذكاء الاصطناعي وتعلّم الآلة بأسلوب مبسّط." },
    { title: "كتابة", text: "ساعدني في كتابة رسالة بريد إلكتروني مهذبة أطلب فيها إجازة ليوم واحد." },
  ],

  you: "أنت",
  thinking: "التفكير",
  thinkingLive: "يفكّر…",
  copy: "نسخ",
  run: "تشغيل",
  runPreview: "تشغيل الكود في لوحة المعاينة",
  preview: "المعاينة",
  reload: "إعادة التشغيل",
  maximize: "تكبير/تصغير",
  close: "إغلاق",
  previewErrors: "{count} خطأ في الكود",
  fixErrors: "اطلب الإصلاح",
  fixPrompt: "ظهرت هذه الأخطاء عند تشغيل الكود. صلّحها وأعطني الكود كاملًا بعد الإصلاح:\n\n```\n{errors}\n```",
  consoleTitle: "مخرجات الكونسول",
  copied: "تم النسخ",
  copyFailed: "تعذّر النسخ",
  regenerate: "إعادة التوليد",
  edit: "تعديل",
  editing: "تعديل الرسالة — الإرسال سيحذف ما بعدها",
  cancel: "إلغاء",
  stopped: "تم الإيقاف",
  errorPrefix: "خطأ: ",
  busy: "انتظر حتى ينتهي الرد أو أوقفه أولًا",
  saveFailed: "تعذّر حفظ المحادثة: {error}",
  loadFailed: "تعذّر تحميل المحادثة: {error}",

  fileTooBig: "الملف «{name}» أكبر من {limit} ك.ب",
  fileNotText: "الملف «{name}» ليس ملفًا نصيًا",
  fileReadFailed: "تعذّر قراءة الملف «{name}»",
  fileHeader: "ملف",

  settingsTitle: "الإعدادات",
  systemPrompt: "تعليمات النظام (System prompt)",
  temperature: "درجة الإبداع (Temperature)",
  contextLength: "طول السياق (num_ctx)",
  theme: "المظهر",
  themeDark: "داكن",
  themeLight: "فاتح",
  themeSystem: "حسب النظام",
  ollamaUrl: "عنوان Ollama",
  save: "حفظ",
  saved: "تم حفظ الإعدادات",
  settingsSaveFailed: "تعذّر حفظ الإعدادات: {error}",

  modelsTitle: "إدارة الموديلات",
  installedModels: "الموديلات المثبتة",
  refresh: "تحديث",
  deleteModelTitle: "حذف الموديل",
  deleteModelConfirm: "حذف الموديل «{name}» من الجهاز؟",
  modelDeleted: "تم حذف {name}",
  modelDeleteFailed: "تعذّر الحذف: {error}",
  downloadModel: "تنزيل موديل",
  downloadPlaceholder: "مثال: qwen2.5:7b",
  download: "تنزيل",
  downloading: "جارٍ التنزيل…",
  downloadDone: "اكتمل تنزيل {name}",
  downloadFailed: "فشل التنزيل: {error}",
  downloadCanceled: "تم إلغاء التنزيل",
  cancelDownloadConfirm: "إلغاء التنزيل الجاري؟",
  confirm: "تأكيد",
  gb: "ج.ب",

  // model suggestions (catalog.js + model manager)
  hwYour: "جهازك:",
  hwRam: "{ram} GB رام",
  hwNoGpu: "بدون كرت شاشة مخصص",
  hwLoading: "جارٍ فحص مواصفات جهازك…",
  hwUnknown: "تعذّر فحص مواصفات الجهاز؛ التقييمات غير متاحة.",
  suggestExplain: "اختر من القائمة موديلًا مقترحًا، وقد قيّمناه حسب قوة جهازك: ✅ يعمل بالكامل على كرت الشاشة وسريع، 👍 أكبر من الكرت لكنه من نوع MoE فيبقى سريعًا، ⚠️ يعمل لكن ببطء لأن جزءًا منه على الرام، ❌ أكبر من قدرة جهازك. ويمكنك أيضًا كتابة اسم أي موديل آخر في الخانة.",
  suggestPick: "— اختر موديلًا مقترحًا —",
  suggestInstalled: "مثبّت",
  cat_chat: "محادثة",
  cat_code: "برمجة",
  cat_reasoning: "تفكير واستدلال",
  cat_vision: "رؤية وصور",
  arabic_good: "جيدة",
  arabic_ok: "مقبولة",
  arabic_weak: "ضعيفة",
  suggestSize: "حجم التنزيل",
  suggestParams: "المعاملات",
  suggestActive: "النشط منه {active} فقط",
  suggestArabic: "العربية",
  rate_best: "✅ ينصح به",
  rate_good: "👍 ينصح به",
  rate_slow: "⚠️ يعمل لكن ببطء",
  rate_no: "❌ لا ينصح",
  rateShort_best: "✅ ينصح",
  rateShort_good: "👍 ينصح",
  rateShort_slow: "⚠️ بطيء",
  rateShort_no: "❌ لا ينصح",
  rateReason_best: "يعمل بالكامل على كرت الشاشة ({vram})، سريع.",
  rateReason_good: "أكبر من ذاكرة الكرت ({vram}) فيتوزع بين الكرت والرام، لكن سرعته جيدة لأنه MoE (يعمل منه {active} فقط في كل خطوة).",
  rateReason_slow: "أكبر من ذاكرة الكرت ({vram}) فيتوزع بين الكرت والرام؛ يعمل لكن ببطء لأنه نموذج كثيف (Dense).",
  rateReason_no: "يحتاج نحو {need}، وهذا أكبر من قدرة جهازك (كرت {vram} + رام {ram}).",
  rateReasonCpu_good: "لم نتعرف على ذاكرة كرت الشاشة، فالتقييم حسب الرام فقط ({ram}): خفيف بما يكفي ليعمل على المعالج بسرعة مقبولة.",
  rateReasonCpu_slow: "لم نتعرف على ذاكرة كرت الشاشة، فالتقييم حسب الرام فقط ({ram}): يتسع في الرام لكنه سيعمل ببطء على المعالج.",
  rateReasonCpu_no: "لم نتعرف على ذاكرة كرت الشاشة، فالتقييم حسب الرام فقط: يحتاج نحو {need} وهذا أكبر من رام جهازك ({ram}).",
};

const dictionaries = { ar };
let dict = ar;

/** Switch the active dictionary (falls back to Arabic). */
export function setLanguage(lang) {
  dict = dictionaries[lang] || ar;
}

/** Look up a string; `{name}` placeholders are filled from `vars`. */
export function t(key, vars) {
  let s = key in dict ? dict[key] : (ar[key] ?? key);
  if (typeof s === "string" && vars) {
    s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  }
  return s;
}

/** Translate static markup that carries data-i18n* attributes. */
export function translateDom(root = document) {
  root.querySelectorAll("[data-i18n]").forEach(n => { n.textContent = t(n.dataset.i18n); });
  root.querySelectorAll("[data-i18n-placeholder]").forEach(n => { n.placeholder = t(n.dataset.i18nPlaceholder); });
  root.querySelectorAll("[data-i18n-title]").forEach(n => {
    n.title = t(n.dataset.i18nTitle);
    n.setAttribute("aria-label", n.title);
  });
}
