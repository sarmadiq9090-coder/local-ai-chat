# Changelog / سجل التغييرات

All notable changes to this project. Format: [Keep a Changelog](https://keepachangelog.com/), versions: [SemVer](https://semver.org/).

## [2.1.1] — 2026-10-30

Bug-fix release after a full end-to-end test on a real Windows PC (5 testers, then 3 regression testers).

### العربية
**أهم الإصلاحات**
- 🧠 **المساعدون:** الأسئلة بلغة غير لغة المعرفة (سؤال عربي عن ملف إنجليزي والعكس) صارت تعمل؛ تقطيع الملفات حسب العناوين والفقرات؛ عتبة التشابه أُعيد ضبطها بقياس فعلي.
- 📄 **ملفات PDF العربية** صارت تُقرأ بشكل صحيح (كانت الحروف معكوسة).
- 💬 أسئلة المتابعة القصيرة («وبدل السكن؟») تفهم سياق السؤال السابق.
- 🤖 التثبيت الجديد لم يعد يختار موديل الفهرسة كموديل للمحادثة.
- ▶️ **المعاينة:** الصفحات المكتوبة كملفات منفصلة (HTML + CSS + JS) صارت تعمل؛ Esc يغلق اللوحة حتى أثناء اللعب؛ عرض اللوحة لا يخرج عن النافذة.
- 🪟 فتح البرنامج مرة ثانية يعيد إظهار النافذة المصغّرة.
- 🔁 فتح آخر محادثة تلقائيًا عند تشغيل البرنامج.
- 🗣️ المساعد يرد بلغة سؤالك الأخير حتى لو بدأت المحادثة بلغة أخرى.
- ⚙️ عنوان Ollama غير الصالح لم يعد يمنع حفظ بقية الإعدادات، وحقل خاطئ واحد في ملف الإعدادات لا يمسح الباقي.

**تحسينات أخرى:** رسائل أخطاء واضحة بلغة الواجهة · حالة Ollama تتحدّث تلقائيًا عند عودته · رسائل التنزيل بالعربي · إلغاء الإعدادات يتراجع عن كل التغييرات · لوحة المفاتيح تبقى داخل النوافذ المنبثقة · لا وميض داكن مع الثيم الفاتح · ألوان أوضح في الثيم الفاتح · الملفات التالفة تظهر ويمكن حذفها مع نسخة احتياطية · قائمة المحادثات أسرع · ترويسات أمان إضافية · أيقونة كبيرة في شريط المهام · إصلاحات في تنسيق Markdown والمرفقات (ومنها ملفات عربية بترميز Windows-1256).

> بعد التحديث قد يطلب منك البرنامج **إعادة فهرسة** مساعديك مرة واحدة (بضغطة زر) بسبب طريقة التقطيع الجديدة.

### English
**Highlights**
- 🧠 **Assistants:** cross-lingual retrieval works (Arabic questions about English files and vice versa); structural chunking by headings/paragraphs; similarity threshold re-tuned on a measured eval.
- 📄 **Arabic PDFs** now extract readable text (presentation forms + reversed visual order fixed).
- 💬 Short follow-up questions use the previous turn for retrieval.
- 🤖 Fresh installs no longer pick the embedding model as the chat model.
- ▶️ **Preview:** pages split into separate HTML/CSS/JS blocks (`<link href>`/`<script src>`) now run; Esc closes the panel even while a game has focus; panel width is clamped to the window.
- 🪟 Launching the app again restores the minimized window.
- 🔁 The last open conversation reopens on start.
- 🗣️ Assistants reply in the language of your latest question, even in a mixed-language chat.
- ⚙️ A bad stored Ollama URL no longer blocks other settings saves; one invalid field in settings.json no longer resets the rest.

**Also:** localized, friendly error messages · Ollama status recovers automatically · localized model-pull statuses · settings Cancel reverts everything · modal focus trap + accessible labels · no dark flash with the light theme · better light-theme contrast · corrupt conversation/settings/assistant files are shown or backed up instead of lost · faster conversation list (cached metadata) · JSON errors everywhere · CSP/nosniff/frame-ancestors headers · big taskbar icon · Markdown and attachment fixes (incl. Windows-1256 Arabic text files) · picking several files at once attaches all of them · preview no longer injects unrelated code blocks · Arabic PDF «ه» fixed · `LOCALAI_DATA_DIR` for portable/isolated data.

> After updating, existing assistants show a one-time **Re-index** prompt because of the new chunker.

## [2.1.0] — 2026-10-30

First public release: Rust desktop app (WebView2), Arabic & English UI, custom assistants with a knowledge base (RAG), live HTML/JS/SVG preview, offline syntax highlighting, hardware-aware model suggestions, auto-update from GitHub Releases, local-server request guard.

[2.1.1]: https://github.com/sarmadiq9090-coder/local-ai-chat/compare/v2.1.0...v2.1.1
[2.1.0]: https://github.com/sarmadiq9090-coder/local-ai-chat/releases/tag/v2.1.0
