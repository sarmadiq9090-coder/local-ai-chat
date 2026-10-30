# الذكاء المحلي (Local AI Chat)

برنامج ويندوز للمحادثة مع نماذج الذكاء الصناعي المحلية عبر [Ollama](https://ollama.com).
ملف `exe` واحد، يعمل بدون إنترنت، ويدعم العربية بالكامل.

## البناء

```bash
cargo build --release
```

الناتج: `target\release\local-ai-chat.exe` (الواجهة مضمّنة داخله).

المتطلبات: Rust (MSVC)، WebView2 (موجود في ويندوز 11)، و Ollama.

## هيكل المشروع

```
src/
  main.rs              نقطة البداية: تشغيل الخادم في خيط + النافذة في الخيط الرئيسي
  config.rs            الإعدادات ومسارات الملفات
  error.rs             نوع أخطاء الـ API (يُرجع {"error": ...})
  storage.rs           حفظ المحادثات (ملف JSON لكل محادثة)
  ollama.rs            فحص Ollama وتشغيله تلقائيًا
  window.rs            النافذة (tao) + WebView2 (wry)
  system.rs            مواصفات الجهاز (كرت الشاشة وذاكرته، الرام، المعالج) لاقتراح الموديلات
  server/
    mod.rs             إنشاء الخادم والحالة المشتركة (AppState) وتجميع المسارات
    routes/            /api/app/*   ملف لكل ميزة: status, settings, conversations, export, system
      mod.rs           يدمج موجّهات الميزات (سطر واحد لكل ميزة)
    ollama_proxy.rs    /api/ollama/* تمرير مباشر (مع البث) إلى Ollama
    assets.rs          ملفات الواجهة المضمّنة
ui/
  index.html           هيكل الصفحة + رابط CSS لكل ميزة
  main.js              الإقلاع: يشغّل كل الميزات المذكورة في features/index.js
  core/                المشترك: api, state, dom, icons, markdown, i18n, strings + base.css (الألوان والتخطيط والمكوّنات العامة)
  features/
    index.js           القائمة الوحيدة للميزات (ترتيبها = ترتيب التشغيل وأولوية زر Esc)
    chat/ sidebar/ models/ settings/ preview/ status/ exporter/ code/
                       لكل ميزة: ملف js وملف css وملف strings.js (النصوص بالعربية والإنجليزية)
  vendor/              highlight.js لتلوين الكود
docs/API.md            عقد الـ API بين الخادم والواجهة
ROADMAP.md             قائمة التحسينات المنجزة والقادمة
```

## مكان البيانات

| البيان | المسار |
|---|---|
| المحادثات | `%APPDATA%\LocalAiChat\conversations\*.json` |
| الإعدادات | `%APPDATA%\LocalAiChat\settings.json` |
| التصدير | `Documents\LocalAiChat\` |

## كيف تضيف ميزة

- **ميزة جديدة في الواجهة**: مجلد `ui/features/<name>/` فيه `<name>.js` و `strings.js` و `<name>.css`، ثم سطر في `ui/features/index.js` وسطر `<link>` في `ui/index.html`. التفاصيل في `CLAUDE.md`.
- **Endpoint جديد**: ملف `src/server/routes/<name>.rs` فيه `router()`، ثم سطر في `src/server/routes/mod.rs`، ووثّقه في `docs/API.md`.
- **حقل جديد في المحادثة/الرسالة**: من الواجهة فقط؛ الخادم يحفظ أي حقول إضافية كما هي.
- **إعداد جديد**: أضف الحقل في `Settings` (`src/config.rs`) مع قيمته الافتراضية، ثم في `ui/features/settings/settings.js`.
- **نص جديد في الواجهة**: في ملف `strings.js` الخاص بالميزة (العربية `ar` والإنجليزية `en`)؛ لا تكتب النصوص مباشرة في الكود.
- **استدعاء Ollama جديد**: متاح مباشرة عبر `/api/ollama/<endpoint>` من `ui/core/api.js`.
