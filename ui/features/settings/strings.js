// settings/strings.js — UI strings owned by the settings feature (registered in settings.js).
// Keys are global: keep them unique across all strings.js files. `ar` is the
// source language and the fallback for keys missing in other languages.

export default {
  ar: {
    settings: "الإعدادات",
    settingsTitle: "الإعدادات",
    language: "اللغة / Language",
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
    invalidOllamaUrl: "عنوان Ollama غير صالح؛ اكتبه كاملًا مثل http://127.0.0.1:11434",
    settingsLoadFailed: "تعذّر تحميل الإعدادات، فلم يُحفظ التغيير: {error}",
  },
  en: {
    settings: "Settings",
    settingsTitle: "Settings",
    language: "Language / اللغة",
    systemPrompt: "System prompt",
    temperature: "Creativity (temperature)",
    contextLength: "Context length (num_ctx)",
    theme: "Theme",
    themeDark: "Dark",
    themeLight: "Light",
    themeSystem: "System",
    ollamaUrl: "Ollama address",
    save: "Save",
    saved: "Settings saved",
    settingsSaveFailed: "Couldn't save settings: {error}",
    invalidOllamaUrl: "The Ollama address isn't valid; enter a full URL such as http://127.0.0.1:11434",
    settingsLoadFailed: "Couldn't load settings, so the change wasn't saved: {error}",
  },
};
