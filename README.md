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
    mod.rs             إنشاء الخادم والمسارات والحالة المشتركة
    app_routes.rs      /api/app/*   الإعدادات، المحادثات، التصدير، الحالة، مواصفات الجهاز
    ollama_proxy.rs    /api/ollama/* تمرير مباشر (مع البث) إلى Ollama
    assets.rs          ملفات الواجهة المضمّنة
ui/
  index.html, css/app.css
  js/                  وحدات ES: api, state, chat, sidebar, settings, models, catalog, markdown, i18n …
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

- **Endpoint جديد**: أضف دالة ومسارًا في `src/server/app_routes.rs` ووثّقه في `docs/API.md`.
- **حقل جديد في المحادثة/الرسالة**: من الواجهة فقط؛ الخادم يحفظ أي حقول إضافية كما هي.
- **إعداد جديد**: أضف الحقل في `Settings` (`src/config.rs`) مع قيمته الافتراضية، ثم في `ui/js/settings.js`.
- **نص جديد في الواجهة**: في `ui/js/i18n.js`.
- **استدعاء Ollama جديد**: متاح مباشرة عبر `/api/ollama/<endpoint>` من `ui/js/api.js`.
