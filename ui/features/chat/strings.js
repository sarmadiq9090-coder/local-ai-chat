// chat/strings.js — UI strings owned by the chat feature (registered in chat.js).
// Keys are global: keep them unique across all strings.js files. `ar` is the
// source language and the fallback for keys missing in other languages.

export default {
  ar: {
    noModelSelected: "اختر موديلًا أولًا (أو نزّل واحدًا من إدارة الموديلات)",
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
    thinking: "التفكير",
    thinkingLive: "يفكّر…",
    regenerate: "إعادة التوليد",
    edit: "تعديل",
    editing: "تعديل الرسالة — الإرسال سيحذف ما بعدها",
    stopped: "تم الإيقاف",
    errorPrefix: "خطأ: ",
    fileTooBig: "الملف «{name}» أكبر من {limit} ك.ب",
    fileNotText: "الملف «{name}» ليس ملفًا نصيًا",
    fileReadFailed: "تعذّر قراءة الملف «{name}»",
  },
  en: {},
};
