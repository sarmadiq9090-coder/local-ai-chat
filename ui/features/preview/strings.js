// preview/strings.js — UI strings owned by the preview feature (registered in preview.js).
// Keys are global: keep them unique across all strings.js files. `ar` is the
// source language and the fallback for keys missing in other languages.

export default {
  ar: {
    preview: "المعاينة",
    reload: "إعادة التشغيل",
    maximize: "تكبير/تصغير",
    close: "إغلاق",
    previewErrors: "{count} خطأ في الكود",
    fixErrors: "اطلب الإصلاح",
    fixPrompt: "ظهرت هذه الأخطاء عند تشغيل الكود. صلّحها وأعطني الكود كاملًا بعد الإصلاح:\n\n```\n{errors}\n```",
    consoleTitle: "مخرجات الكونسول",
  },
  en: {
    preview: "Preview",
    reload: "Run again",
    maximize: "Maximize/restore",
    close: "Close",
    previewErrors: "{count} error(s) in the code",
    fixErrors: "Ask for a fix",
    fixPrompt: "These errors appeared when running the code. Fix them and give me the complete code after the fix:\n\n```\n{errors}\n```",
    consoleTitle: "Console output",
  },
};
