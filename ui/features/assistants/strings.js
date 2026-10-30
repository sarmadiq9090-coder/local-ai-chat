// assistants/strings.js — UI strings of the custom assistants (RAG) feature.
// Keys are global: all prefixed `asst_`. `ar` is the source language.
// The *_rules / not-in-knowledge strings are part of the prompt sent to the model.

export default {
  ar: {
    asst_section: "المساعدون",
    asst_new: "مساعد جديد",
    asst_none: "لا يوجد مساعدون بعد",
    asst_edit: "تعديل المساعد",
    asst_chat: "محادثة مع {name}",
    asst_editorNew: "مساعد جديد",
    asst_editorEdit: "تعديل: {name}",
    asst_name: "الاسم",
    asst_namePh: "مثال: مساعد المطعم",
    asst_icon: "الأيقونة",
    asst_description: "الوصف",
    asst_descriptionPh: "وصف قصير يظهر في القائمة",
    asst_instructions: "التعليمات",
    asst_instructionsPh: "كيف يتصرف المساعد؟ مثال: أنت موظف استقبال ودود في مطعم…",
    asst_model: "الموديل",
    asst_modelDefault: "الموديل الحالي في التطبيق",
    asst_strict: "الوضع الصارم: يجيب فقط من المعرفة المعطاة",
    asst_embedModel: "موديل الفهرسة (embeddings)",
    asst_embedInstalled: "مثبّت",
    asst_embedMissing: "غير مثبّت",
    asst_pull: "تنزيل",
    asst_pulling: "جارٍ التنزيل…",
    asst_pullDone: "تم تنزيل {name}",
    asst_pullFailed: "فشل التنزيل: {error}",
    asst_knowledge: "المعرفة",
    asst_saveFirst: "احفظ المساعد أولًا لتضيف له معرفة.",
    asst_dropFiles: "اسحب الملفات وأفلتها هنا أو",
    asst_chooseFiles: "اختر ملفات",
    asst_fileTypes: "نص، Markdown، CSV، JSON، أكواد، PDF",
    asst_addNote: "إضافة ملاحظة",
    asst_editNote: "تعديل الملاحظة",
    asst_noteName: "عنوان الملاحظة",
    asst_noteNamePh: "مثال: قائمة الطعام",
    asst_noteText: "النص",
    asst_noteTextPh: "اكتب المعلومات أو الأسئلة والأجوبة هنا…",
    asst_noDocs: "لا توجد معرفة بعد.",
    asst_chunks_zero: "لا مقاطع",
    asst_chunks_one: "مقطع واحد",
    asst_chunks_two: "مقطعان",
    asst_chunks_few: "{n} مقاطع",
    asst_chunks_many: "{n} مقطعًا",
    asst_chunks_other: "{n} مقطع",
    asst_kindNote: "ملاحظة",
    asst_kindFile: "ملف",
    asst_deleteDoc: "حذف المستند",
    asst_deleteDocConfirm: "حذف «{name}» من معرفة المساعد؟",
    asst_reindex: "إعادة الفهرسة",
    asst_stale: "فهرس المعرفة قديم (تغيّر موديل الفهرسة أو طريقة الفهرسة في التطبيق). أعد الفهرسة.",
    asst_reindexNow: "أعد الفهرسة الآن",
    asst_reindexAsk: "فهرس معرفة «{name}» قديم ويجب إعادة بنائه قبل البحث فيه (تحديث التطبيق حسّن الفهرسة). أعيد الفهرسة الآن؟",
    asst_reindexing: "جارٍ إعادة فهرسة «{name}»…",
    asst_status_extracting: "استخراج النص…",
    asst_status_embedding: "فهرسة المقاطع…",
    asst_status_saving: "حفظ…",
    asst_indexed: "تمت فهرسة «{name}»",
    asst_reindexed: "تمت إعادة الفهرسة",
    asst_docFailed: "تعذّرت إضافة «{name}»: {error}",
    asst_failed: "فشلت العملية: {error}",
    asst_modelMissing: "موديل الفهرسة {name} غير مثبّت. افتح محرر المساعد (اضغط اسمه في الأعلى) ونزّله.",
    asst_modelMissingHere: "موديل الفهرسة {name} غير مثبّت. نزّله بزر «تنزيل» بجانب «موديل الفهرسة» أعلاه.",
    asst_modelNotInstalled: "الموديل {name} غير مثبّت؛ سيُستخدم موديل التطبيق الحالي",
    asst_skipped: "تم تخطي ملفات غير مدعومة: {names}",
    asst_corrupt: "تعذّرت قراءة {n} من ملفات المساعدين؛ حُفظت نسخة منها باسم assistant.json.corrupt-… وأُخفيت من القائمة.",
    asst_warn_arabic_pdf_quality: "قد يكون النص العربي المستخرج من ملف PDF هذا غير دقيق؛ يُفضّل رفع نسخة ‎.txt أو ‎.md منه.",
    asst_suggestTitle: "اسأل",
    asst_suggest: ["ما أهم ما في «{doc}»؟", "لخّص لي «{doc}»", "ما الذي يجب أن أعرفه عن «{doc}»؟"],
    asst_welcomeNoDesc: "اسألني عن المعرفة التي أُعطيت لي.",
    asst_welcomeAddDocs: "لا توجد معرفة بعد — أضف ملفات أو ملاحظات من محرر المساعد.",
    asst_save: "حفظ",
    asst_create: "إنشاء",
    asst_saved: "تم الحفظ",
    asst_delete: "حذف المساعد",
    asst_deleteTitle: "حذف المساعد",
    asst_deleteConfirm: "حذف المساعد «{name}» مع كل معرفته؟ لا يمكن التراجع.",
    asst_nameRequired: "الاسم مطلوب",
    asst_busy: "انتظر حتى تنتهي الفهرسة",
    asst_chipTitle: "هذه المحادثة مع مساعد — اضغط للتعديل",
    asst_sources: "المصادر",
    asst_searchFailed: "تعذّر البحث في المعرفة: {error}",
    asst_unavailable: "المساعد غير موجود؛ سيتم الرد كمحادثة عادية.",
    // prompt parts
    asst_srcLabel: "المصدر",
    asst_rulesHead: "قواعد الإجابة",
    asst_sourcesHead: "المصادر",
    asst_strictNoSources:
      "- لا توجد أي مصادر من المعرفة تطابق هذا السؤال.\n" +
      "- لا تستخدم أي معرفة خارجية ولا معلومات من الرسائل السابقة، ولا تخمّن.\n" +
      "- لا تذكر أي أرقام مصادر مثل [1].\n" +
      "- أجب بهذه الجملة حرفيًا ولا شيء غيرها:\n{sentence}",
    asst_softNoSources:
      "- لم يُعثر على مصادر من المعرفة تطابق هذا السؤال.\n" +
      "- لا تذكر أي أرقام مصادر مثل [1].\n" +
      "- يمكنك الإجابة من معرفتك العامة، مع التنبيه إلى أنها ليست من المعرفة المعطاة.\n" +
      "- أجب بلغة سؤال المستخدم.",
    asst_strictRules:
      "- أجب فقط من المصادر أدناه، ولا تستخدم أي معرفة خارجية أبدًا ولا تخمّن.\n" +
      "- اذكر رقم المصدر بين قوسين مربعين بعد المعلومة، مثل [1].\n" +
      "- إذا لم تكن الإجابة موجودة نصًّا في المصادر (حتى لو كنت تعرفها من مكان آخر)، فأجب بهذه الجملة حرفيًا ولا شيء غيرها:\n{sentence}\n" +
      "- أجب بلغة سؤال المستخدم.",
    asst_softRules:
      "- اعتمد على المصادر أدناه أولًا، واذكر رقم المصدر بين قوسين مربعين مثل [1].\n" +
      "- إذا لم تكن الإجابة في المصادر فيمكنك الإجابة من معرفتك العامة، مع التنبيه إلى أنها ليست من المعرفة المعطاة.\n" +
      "- أجب بلغة سؤال المستخدم.",
  },
  en: {
    asst_section: "Assistants",
    asst_new: "New assistant",
    asst_none: "No assistants yet",
    asst_edit: "Edit assistant",
    asst_chat: "Chat with {name}",
    asst_editorNew: "New assistant",
    asst_editorEdit: "Edit: {name}",
    asst_name: "Name",
    asst_namePh: "e.g. Restaurant helper",
    asst_icon: "Icon",
    asst_description: "Description",
    asst_descriptionPh: "Short description shown in the list",
    asst_instructions: "Instructions",
    asst_instructionsPh: "How should it behave? e.g. You are a friendly restaurant host…",
    asst_model: "Model",
    asst_modelDefault: "The app's current model",
    asst_strict: "Strict mode: answer only from the given knowledge",
    asst_embedModel: "Embedding model",
    asst_embedInstalled: "Installed",
    asst_embedMissing: "Not installed",
    asst_pull: "Download",
    asst_pulling: "Downloading…",
    asst_pullDone: "Downloaded {name}",
    asst_pullFailed: "Download failed: {error}",
    asst_knowledge: "Knowledge",
    asst_saveFirst: "Save the assistant first to add knowledge.",
    asst_dropFiles: "Drag and drop files here or",
    asst_chooseFiles: "choose files",
    asst_fileTypes: "Text, Markdown, CSV, JSON, code, PDF",
    asst_addNote: "Add note",
    asst_editNote: "Edit note",
    asst_noteName: "Note title",
    asst_noteNamePh: "e.g. Menu",
    asst_noteText: "Text",
    asst_noteTextPh: "Write facts or Q&A here…",
    asst_noDocs: "No knowledge yet.",
    asst_chunks_one: "{n} chunk",
    asst_chunks_other: "{n} chunks",
    asst_kindNote: "Note",
    asst_kindFile: "File",
    asst_deleteDoc: "Delete document",
    asst_deleteDocConfirm: "Remove “{name}” from this assistant's knowledge?",
    asst_reindex: "Re-index",
    asst_stale: "The knowledge index is out of date (the embedding model or the app's indexing changed). Re-index it.",
    asst_reindexNow: "Re-index now",
    asst_reindexAsk: "The knowledge index of “{name}” is out of date and must be rebuilt before it can be searched (an app update improved indexing). Re-index now?",
    asst_reindexing: "Re-indexing “{name}”…",
    asst_status_extracting: "Extracting text…",
    asst_status_embedding: "Indexing chunks…",
    asst_status_saving: "Saving…",
    asst_indexed: "Indexed “{name}”",
    asst_reindexed: "Re-indexed",
    asst_docFailed: "Couldn't add “{name}”: {error}",
    asst_failed: "Failed: {error}",
    asst_modelMissing: "Embedding model {name} isn't installed. Open the assistant editor (click its name at the top) to download it.",
    asst_modelMissingHere: "Embedding model {name} isn't installed. Download it with the “Download” button next to “Embedding model” above.",
    asst_modelNotInstalled: "Model {name} isn't installed; the app's current model is used",
    asst_skipped: "Skipped unsupported files: {names}",
    asst_corrupt: "{n} assistant file(s) couldn't be read; a copy was kept as assistant.json.corrupt-… and they were hidden from the list.",
    asst_warn_arabic_pdf_quality: "The Arabic text extracted from this PDF may be inaccurate; uploading a .txt or .md copy works better.",
    asst_suggestTitle: "Ask",
    asst_suggest: ["What are the key points of “{doc}”?", "Summarize “{doc}”", "What should I know about “{doc}”?"],
    asst_welcomeNoDesc: "Ask me about the knowledge I was given.",
    asst_welcomeAddDocs: "No knowledge yet — add files or notes in the assistant editor.",
    asst_save: "Save",
    asst_create: "Create",
    asst_saved: "Saved",
    asst_delete: "Delete assistant",
    asst_deleteTitle: "Delete assistant",
    asst_deleteConfirm: "Delete the assistant “{name}” and all its knowledge? This can't be undone.",
    asst_nameRequired: "Name is required",
    asst_busy: "Wait until indexing finishes",
    asst_chipTitle: "This chat is with an assistant — click to edit",
    asst_sources: "Sources",
    asst_searchFailed: "Knowledge search failed: {error}",
    asst_unavailable: "The assistant no longer exists; replying as a normal chat.",
    asst_srcLabel: "Source",
    asst_rulesHead: "Answer rules",
    asst_sourcesHead: "Sources",
    asst_strictNoSources:
      "- There are NO sources from the knowledge for this question.\n" +
      "- Do not use outside knowledge or information from earlier messages, and never guess.\n" +
      "- Do not cite any source numbers like [1].\n" +
      "- Reply with exactly this sentence and nothing else:\n{sentence}",
    asst_softNoSources:
      "- No sources from the knowledge match this question.\n" +
      "- Do not cite any source numbers like [1].\n" +
      "- You may answer from general knowledge, but say it isn't from the given knowledge.\n" +
      "- Reply in the language of the user's question.",
    asst_strictRules:
      "- Answer ONLY from the sources below. Never use outside knowledge and never guess.\n" +
      "- Cite the source number in square brackets after each fact, like [1].\n" +
      "- If the sources don't state the answer (even if you know it from elsewhere), reply with exactly this sentence and nothing else:\n{sentence}\n" +
      "- Reply in the language of the user's question.",
    asst_softRules:
      "- Rely on the sources below first and cite the source number in square brackets, like [1].\n" +
      "- If the answer isn't in the sources you may answer from general knowledge, but say it isn't from the given knowledge.\n" +
      "- Reply in the language of the user's question.",
  },
};

/** The exact "not in knowledge" sentences (picked by the question's language, not the UI's). */
export const NOT_IN_KNOWLEDGE = {
  ar: "لا توجد هذه المعلومة في المعرفة التي أُعطيت لي.",
  en: "That information isn't in the knowledge I was given.",
};
