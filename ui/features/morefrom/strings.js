// morefrom/strings.js — UI strings owned by the morefrom feature (registered in morefrom.js).
// Keys are global: keep them unique across all strings.js files. `ar` is the
// source language and the fallback for keys missing in other languages.
// App names, taglines and descriptions are not here: they come in ar and en from the
// vendored apps.json (GET /api/app/more-from).

export default {
  ar: {
    mf_section: "المزيد من FastKiwi",
    mf_text: "برامج أخرى من FastKiwi. الروابط تُفتح في المتصفح أو في متجر Microsoft.",
    mf_page: "صفحة البرنامج",
    mf_store: "متجر Microsoft",
    mf_soon: "قريباً",
    mf_failed: "تعذّر عرض القائمة: {error}",
  },
  en: {
    mf_section: "More from FastKiwi",
    mf_text: "Other apps by FastKiwi. Links open in your browser or in the Microsoft Store.",
    mf_page: "App page",
    mf_store: "Microsoft Store",
    mf_soon: "Coming soon",
    mf_failed: "Couldn't show the list: {error}",
  },
};
