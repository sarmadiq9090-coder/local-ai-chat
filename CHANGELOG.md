# Changelog / سجل التغييرات

All notable changes to this project. Format: [Keep a Changelog](https://keepachangelog.com/), versions: [SemVer](https://semver.org/).

## [Unreleased]

### العربية
- 🧯 **سجل الأعطال:** عند توقّف Lemon بشكل غير متوقع يُحفظ السبب في `crash.txt` داخل مجلد البيانات، ويظهر تنبيه في التشغيل التالي مع زر لفتح المجلد.
- 🌐 **لغة التشغيل الأول تتبع ويندوز:** عربية إن كانت لغة واجهة ويندوز عربية، وإلا فالإنجليزية. اختيارك المحفوظ يبقى هو الأساس دائمًا.
- ⬇️ **زر «تنزيل Ollama»** في شريط «Ollama غير شغّال» يفتح صفحة التنزيل الرسمية (ollama.com/download).
- 🔤 **الأرقام:** أرقام 123 افتراضيًا في الواجهة العربية مع خيار في الإعدادات لعرض ١٢٣، وحقول الأرقام تقبل الشكلين.
- 🆘 **الدعم:** قسم «الدعم» في الإعدادات فيه بريد support@fastkiwi.app وصفحة الدعم، و«تراخيص البرامج مفتوحة المصدر».
- 📜 **إشعارات الطرف الثالث:** ملف `THIRD-PARTY-NOTICES.txt` يُولَّد من `Cargo.lock` (`tools/make_notices.py`) ومضمّن في البرنامج، ويتحقق CI من أنه محدَّث وأن كل التراخيص مسموحة.
- 🪛 علامة «…» في نصوص الحقول الفارغة صارت في نهاية الجملة العربية لا في بدايتها.
- 🧩 **يعمل دون Visual C++ Runtime:** صار البرنامج يعمل على أي ويندوز 10/11 دون تثبيت حزمة Microsoft Visual C++ (كان الإصدار 2.2.0 لا يفتح على الأجهزة التي تخلو منها). ويتحقق CI من أن الملف لا يعتمد إلا على مكتبات ويندوز نفسها.
- 🔏 **تحديثات موقّعة:** صار المحدِّث قادرًا على التحقق من توقيع minisign لكل تحديث (`local-ai-chat.exe.minisig`) بمفاتيح مضمّنة في البرنامج، ويجب أن يطابق التوقيع رقم الإصدار نفسه. عند إضافة المفاتيح يُرفض أي تحديث بلا توقيع صالح، وتظهر رسالة واضحة مع رابط للتنزيل اليدوي. ما دامت قائمة المفاتيح فارغة يبقى السلوك كما هو (التحقق بـ sha256 فقط).
- 🛡️ **إصدار أكثر أمانًا على GitHub:** تثبيت كل الإجراءات (actions) على commit محدد، وصلاحيات قراءة فقط افتراضيًا، والتوقيع يجري في بيئة `release` بعد موافقة المالك، مع شهادة مصدر البناء (attestation)، وإنشاء الإصدار كمسودة لا تُنشر إلا بعد التأكد من وجود الملفات الثلاثة. أُضيف Dependabot. دليل المالك: `RELEASING.md`.
- 🚂 **إصدار بلا وسوم يدوية:** يكفي دمج طلب يرفع رقم الإصدار في `Cargo.toml`؛ يبني GitHub البرنامج وينتظر موافقة المالك ثم ينشئ الوسم والإصدار بنفسه. الدمج بلا تغيير في الرقم لا ينشر شيئًا.
- 🔕 **رسالة هادئة عند تعذّر الوصول إلى خادم التحديثات** (HTTP 403 أو 404 أو 429): «تعذّر الوصول إلى خادم التحديثات الآن. برنامجك يعمل كالمعتاد، وسنحاول مرة أخرى لاحقًا.» بدل «rate limit… try again later»، وتظهر فقط عند الضغط على «التحقق من التحديثات»؛ الفحص التلقائي يبقى صامتًا.
- ☁️ **مصدر تحديثات لا يعتمد على GitHub:** يسأل Lemon أولاً ملف تحديث موقّعاً على `updates.fastkiwi.app/local-ai-chat/latest.json` (Cloudflare)، ويجرّب روابط التنزيل بالترتيب (نسخة Cloudflare أولاً ثم GitHub) مع التحقق من sha256 والتوقيع لكل نسخة، ويرجع إلى GitHub إن تعذّر الملف أو لم يُتحقَّق منه. ما دامت المفاتيح غير مضافة لا يُستعمل الملف إلا إذا طابقت بصمته ما ينشره GitHub. لا رجوع لإصدار أقدم. الأداة `tools/make-update-manifest` تكتب الملف في CI وتوقّعه حين يوجد السر. التفاصيل: `docs/specs/update-manifest.md`.

### English
- 🧯 **Crash log:** panics are recorded to `crash.txt` in the data folder; the next start shows a one-line notice with an "Open folder" button.
- 🌐 **First run follows the Windows display language:** Arabic on Arabic Windows, English otherwise. A saved choice always wins.
- ⬇️ **"Get Ollama"** button in the "Ollama isn't running" banner, opening ollama.com/download.
- 🔤 **Digits:** Western 123 by default in the Arabic UI, with a setting for Arabic-Indic ١٢٣; number fields accept both.
- 🆘 **Support** section in Settings: support@fastkiwi.app, the support page and the open-source licences.
- 🔕 **Calm "update server can't be reached" message** for HTTP 403, 404 and 429: "The update server can't be reached right now. Your app keeps working; we'll try again later." instead of a rate-limit error, shown only after "Check for updates"; automatic checks stay silent.
- 📜 **Third-party notices:** `THIRD-PARTY-NOTICES.txt`, generated from `Cargo.lock` by `tools/make_notices.py`, embedded in the exe; CI checks it is current and every licence is on the allowlist.
- 🪛 Empty-field placeholders in the Arabic UI keep their "…" at the end of the sentence.
- 🧩 **Runs without the Visual C++ runtime:** the exe now links the C runtime statically, so it starts on Windows 10/11 PCs without the Visual C++ Redistributable (2.2.0 needed VCRUNTIME140.dll). CI fails if the exe imports any DLL that is not part of Windows (`tools/check_imports.py`).
- 🔏 **Signed updates:** the updater can verify a minisign signature (`local-ai-chat.exe.minisig`) by compiled-in keys, with the trusted comment bound to the exact version (`local-ai-chat vX.Y.Z`). Once keys are compiled in, unsigned or badly signed updates are refused with a clear message and a manual-download link. With the key list still empty, behavior is unchanged (sha256 only). Spec: `docs/specs/signing.md`.
- 🛡️ **Hardened release workflow:** SHA-pinned actions, read-only default token, no credentials persisted and no build cache. Signing runs in the owner-approved `release` Environment. Releases get a build-provenance attestation and are published only after a draft holding all three assets is checked. Also: Dependabot for actions and cargo, and a `workflow_dispatch` dry run.
- 🚂 **Release on version bump:** merging a PR that bumps the `Cargo.toml` version releases it (after owner approval); CI creates the `v<version>` tag itself, with notes from `CHANGELOG.md`. Merges with an unchanged version only run a quick version check.
- ☁️ **Update feed that does not depend on GitHub:** the updater asks a signed manifest at `updates.fastkiwi.app/local-ai-chat/latest.json` (Cloudflare) first and tries the download addresses in order (the Cloudflare copy, then GitHub), checking sha256 and the signature of each copy; it falls back to GitHub when the manifest is unreachable or not trusted. Until signing keys are compiled in, the manifest is used only when GitHub reports the same sha256. Never a downgrade. `tools/make-update-manifest` writes it in CI and signs it when the secret exists (`docs/specs/update-manifest.md`).

### Security
- الخادم المحلي يرفض الطلبات القادمة من مواقع أخرى عبر ترويسة `Sec-Fetch-Site` (مثل `<img>` أو `<script>` من صفحة ويب)، ويضيف `Cross-Origin-Resource-Policy: same-origin` و`X-Content-Type-Options: nosniff` لكل الردود.
- The local server now rejects cross-site browser requests via `Sec-Fetch-Site` (e.g. an `<img>`/`<script>` GET from a web page, which carries no `Origin`), and adds `Cross-Origin-Resource-Policy: same-origin` and `X-Content-Type-Options: nosniff` to every response.

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
