// status/strings.js — UI strings owned by the status feature (registered in status.js).
// Keys are global: keep them unique across all strings.js files. `ar` is the
// source language and the fallback for keys missing in other languages.

export default {
  ar: {
    statusConnected: "متصل",
    statusConnecting: "جارٍ الاتصال…",
    ollamaDown: "Ollama غير شغّال",
    ollamaDownBanner: "خدمة Ollama غير شغّالة، لذلك لا يمكن المحادثة الآن.",
    startOllama: "تشغيل Ollama",
    startingOllama: "جارٍ التشغيل…",
    startFailed: "تعذّر تشغيل Ollama. تأكد من تثبيته.",
    backendDown: "تعذّر الاتصال بالخادم المحلي",
  },
  en: {
    statusConnected: "Connected",
    statusConnecting: "Connecting…",
    ollamaDown: "Ollama isn't running",
    ollamaDownBanner: "The Ollama service isn't running, so chatting isn't possible right now.",
    startOllama: "Start Ollama",
    startingOllama: "Starting…",
    startFailed: "Couldn't start Ollama. Make sure it is installed.",
    backendDown: "Can't reach the local server",
  },
};
