// core/strings.js — strings used by core modules (dom.js, markdown.js) or shared by
// several features. Registered by core/i18n.js under the name "core".
// Keys are global: keep them unique across all strings.js files. `ar` is the
// source language and the fallback for keys missing in other languages.

export default {
  ar: {
    appTitle: "الذكاء المحلي",
    conversationsLabel: "المحادثات",
    untitled: "محادثة جديدة",
    delete: "حذف",
    copy: "نسخ",
    run: "تشغيل",
    runPreview: "تشغيل الكود في لوحة المعاينة",
    copied: "تم النسخ",
    copyFailed: "تعذّر النسخ",
    cancel: "إلغاء",
    busy: "انتظر حتى ينتهي الرد أو أوقفه أولًا",
    fileHeader: "ملف",
    confirm: "تأكيد",
  },
  en: {
    appTitle: "Local AI Chat",
    conversationsLabel: "Conversations",
    untitled: "New chat",
    delete: "Delete",
    copy: "Copy",
    run: "Run",
    runPreview: "Run the code in the preview panel",
    copied: "Copied",
    copyFailed: "Couldn't copy",
    cancel: "Cancel",
    busy: "Wait for the reply to finish, or stop it first",
    fileHeader: "File",
    confirm: "Confirm",
  },
};
