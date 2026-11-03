# Changelog / سجل التغييرات

All notable changes to this project. Format: [Keep a Changelog](https://keepachangelog.com/), versions: [SemVer](https://semver.org/).

## [Unreleased]

### العربية
- 🔏 **تحديثات موقّعة:** صار المحدِّث قادرًا على التحقق من توقيع minisign لكل تحديث (`local-ai-chat.exe.minisig`) بمفاتيح مضمّنة في البرنامج، ويجب أن يطابق التوقيع رقم الإصدار نفسه. عند إضافة المفاتيح يُرفض أي تحديث بلا توقيع صالح، وتظهر رسالة واضحة مع رابط للتنزيل اليدوي. ما دامت قائمة المفاتيح فارغة يبقى السلوك كما هو (التحقق بـ sha256 فقط).
- 🛡️ **إصدار أكثر أمانًا على GitHub:** تثبيت كل الإجراءات (actions) على commit محدد، وصلاحيات قراءة فقط افتراضيًا، والتوقيع يجري في بيئة `release` بعد موافقة المالك، مع شهادة مصدر البناء (attestation)، وإنشاء الإصدار كمسودة لا تُنشر إلا بعد التأكد من وجود الملفات الثلاثة. أُضيف Dependabot. دليل المالك: `RELEASING.md`.
- 🚂 **إصدار بلا وسوم يدوية:** يكفي دمج طلب يرفع رقم الإصدار في `Cargo.toml`؛ يبني GitHub البرنامج وينتظر موافقة المالك ثم ينشئ الوسم والإصدار بنفسه. الدمج بلا تغيير في الرقم لا ينشر شيئًا.

### English
- 🔏 **Signed updates:** the updater can verify a minisign signature (`local-ai-chat.exe.minisig`) by compiled-in keys, with the trusted comment bound to the exact version (`local-ai-chat vX.Y.Z`). Once keys are compiled in, unsigned or badly signed updates are refused with a clear message and a manual-download link. With the key list still empty, behavior is unchanged (sha256 only). Spec: `docs/specs/signing.md`.
- 🛡️ **Hardened release workflow:** SHA-pinned actions, read-only default token, no credentials persisted and no build cache. Signing runs in the owner-approved `release` Environment. Releases get a build-provenance attestation and are published only after a draft holding all three assets is checked. Also: Dependabot for actions and cargo, and a `workflow_dispatch` dry run.
- 🚂 **Release on version bump:** merging a PR that bumps the `Cargo.toml` version releases it (after owner approval); CI creates the `v<version>` tag itself, with notes from `CHANGELOG.md`. Merges with an unchanged version only run a quick version check.

## [2.2.0] — 2026-10-30

### العربية
- 🍋 **الاسم الجديد: Lemon** مع أيقونة جديدة (ليمونة بالكود على خلفية داكنة).
- 🧠 **الفهم العميق للمساعدين:** ملخّص لكل ملف عند رفعه، سياق محيط بكل مقطع، إعادة صياغة أسئلة المتابعة، ذاكرة أكبر وتفكير قبل الإجابة. في اختبارنا ارتفعت الإجابات الصحيحة من 5/15 إلى 9/15 دون أي إجابة مختلَقة.
- 📚 **اختيار موديل الفهرسة من قائمة** مع حجمه ودعمه للعربية ومدى توافقه مع جهازك، وزر تنزيل.
- 👁️ «ماذا قرأ المساعد؟»: عرض النص المستخرج من كل ملف وملخّصه وتعديله، ولوحة اختبار للمساعد.
- 📝 قوالب تعليمات جاهزة (قانوني، معلّم، دعم فني، باحث، موارد بشرية، توثيق تقني).
- 📄 تحذير لملفات PDF الممسوحة ضوئيًا، وإصلاح الأرقام المفصولة في PDF العربية.

### English
- 🍋 **New name: Lemon**, with a new code-lemon icon.
- 🧠 **Deep understanding for assistants:** per-document digests, neighbouring-chunk context, follow-up query rewriting, larger context and thinking. In our eval correct answers rose from 5/15 to 9/15 with no hallucinations.
- 📚 **Embedding-model picker** with size, Arabic support, PC-compatibility verdict and download.
- 👁️ "What did it read?" (extracted text + editable digest) and an assistant test panel.
- 📝 Instruction templates (legal, tutor, support, research, HR, technical docs).
- 📄 Scanned-PDF warning; split digits in Arabic PDFs re-joined.

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

[2.2.0]: https://github.com/sarmadiq9090-coder/local-ai-chat/compare/v2.1.1...v2.2.0
[2.1.1]: https://github.com/sarmadiq9090-coder/local-ai-chat/compare/v2.1.0...v2.1.1
[2.1.0]: https://github.com/sarmadiq9090-coder/local-ai-chat/releases/tag/v2.1.0
