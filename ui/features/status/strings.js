// status/strings.js — UI strings owned by the status feature (registered in status.js).
// Keys are global: keep them unique across all strings.js files. `ar` is the
// source language and the fallback for keys missing in other languages.

export default {
  ar: {
    statusConnected: "متصل",
    statusConnecting: "جارٍ الاتصال…",
    ollamaDown: "Ollama غير شغّال",
    ollamaDownBanner: "خدمة Ollama غير شغّالة، لذلك لا يمكن المحادثة الآن. إن لم يكن مثبّتًا على جهازك فنزّله أولًا (مجاني).",
    startOllama: "تشغيل Ollama",
    getOllama: "تنزيل Ollama",
    startingOllama: "جارٍ التشغيل…",
    startFailed: "تعذّر تشغيل Ollama. إن لم يكن مثبّتًا فنزّله من زر «تنزيل Ollama».",
    backendDown: "تعذّر الاتصال بالخادم المحلي",
    startSlow: "لم يستجب Ollama بعد. تأكد من أنه مثبّت (زر «تنزيل Ollama») ومن صحة عنوانه في الإعدادات؛ سنواصل المحاولة في الخلفية.",
  },
  en: {
    statusConnected: "Connected",
    statusConnecting: "Connecting…",
    ollamaDown: "Ollama isn't running",
    ollamaDownBanner: "The Ollama service isn't running, so chatting isn't possible right now. If it isn't installed on this computer, download it first (free).",
    startOllama: "Start Ollama",
    getOllama: "Get Ollama",
    startingOllama: "Starting…",
    startFailed: "Couldn't start Ollama. If it isn't installed, use \"Get Ollama\" to download it.",
    backendDown: "Can't reach the local server",
    startSlow: "Ollama hasn't responded yet. Make sure it is installed (\"Get Ollama\") and its address in Settings is correct; we'll keep checking in the background.",
  },
};
