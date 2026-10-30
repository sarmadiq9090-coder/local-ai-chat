# Contributing to Lemon — Local AI Chat

<div dir="rtl">

**ملخّص بالعربية:** شكرًا لاهتمامك بالمساهمة! البرنامج مكتوب بـ Rust (الخادم والنافذة) وواجهته HTML/JS بلا أدوات بناء.
للبناء: `cargo build`، وللتطوير شغّل `target\debug\local-ai-chat.exe --serve 8790` وافتح `http://127.0.0.1:8790/` في المتصفح.
كل ميزة في مجلد خاص: الواجهة في `ui/features/<الاسم>/` (مع `strings.js` بالعربية والإنجليزية)، والخادم في `src/server/routes/<الاسم>.rs`.
لا تكتب نصوص الواجهة مباشرة في الكود، وجرّب التغيير باللغتين (عربي/إنجليزي) وبالثيمين (فاتح/داكن) قبل فتح طلب الدمج (PR).
يمكنك كتابة الـ Issues والـ PRs بالعربية أو بالإنجليزية.

</div>

---

Thanks for your interest! Issues and pull requests are welcome in **Arabic or
English**. For anything larger than a small fix, please open an issue first so
we can agree on the approach.

## Getting started

Requirements: Windows 10/11, [Rust](https://rustup.rs/) stable with the MSVC
toolchain, Visual Studio Build Tools ("Desktop development with C++"), the
WebView2 Runtime, and [Ollama](https://ollama.com) for manual testing.

```powershell
git clone https://github.com/sarmadiq9090-coder/local-ai-chat.git
cd local-ai-chat
cargo build
target\debug\local-ai-chat.exe              # full app with its own window
target\debug\local-ai-chat.exe --serve 8790 # server only; open http://127.0.0.1:8790/
```

In debug builds the files in `ui/` are served from disk, so UI changes only
need a browser refresh in `--serve` mode. Rust changes need a rebuild.

## Project layout

The authoritative guide to the architecture and conventions is
[CLAUDE.md](CLAUDE.md) (it is written for both humans and AI coding
assistants). The API between backend and UI is documented in
[docs/API.md](docs/API.md); feature designs live in [docs/specs/](docs/specs/).

In short, the code is organized **by feature**:

| Part | Where | Notes |
|---|---|---|
| UI feature | `ui/features/<name>/` | `<name>.js` (logic), `strings.js` (`{ ar: {...}, en: {...} }`), optional `<name>.css`. Registered in `ui/features/index.js`. |
| Shared UI core | `ui/core/` | `api.js`, `i18n.js`, `state.js`, `dom.js`, `icons.js`, `markdown.js`, `base.css`. No feature-specific code here. |
| Backend route | `src/server/routes/<name>.rs` | Exposes `pub fn router() -> Router<AppState>`; add one `mod` and one `.merge(...)` line in `routes/mod.rs`. Document new endpoints in `docs/API.md`. |
| Backend logic | `src/<module>.rs` | Storage, config, Ollama process, window, etc. |

A new feature should normally touch its own folder/file plus one registration
line on each side, and nothing else.

## Coding style

### Rust
- Format with `cargo fmt`; keep `cargo clippy` clean.
- Small, focused modules with a `//!` header explaining what the module owns.
- Return `anyhow::Result` internally and map errors to JSON `{ "error": ... }`
  at the HTTP boundary (see `src/error.rs`).
- Never block the async runtime: use `tokio::fs`, or `spawn_blocking` for heavy
  CPU/IO work.
- The server must keep listening on `127.0.0.1` only.
- New settings fields need a default (settings are `#[serde(default)]`) so old
  settings files keep loading.

### JavaScript / CSS / HTML
- Plain ES modules, no build step, no new runtime dependencies without
  discussion (the app must work fully offline; any vendored library goes in
  `ui/vendor/` with its license file and an entry in
  [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)).
- 2-space indentation (see [.editorconfig](.editorconfig)).
- **No hard-coded UI text.** Every user-visible string goes in the feature's
  `strings.js` with both `ar` and `en` entries and is read through `t("key")`.
  Arabic is the source language and the fallback. Keys are global, so keep
  them unique.
- **RTL first.** Use CSS logical properties (`margin-inline-start`,
  `padding-inline-end`, `inset-inline-start`, `text-align: start`) instead of
  `left`/`right`. Code blocks and other LTR content get `dir="ltr"`
  explicitly. Check that icons with a direction (arrows, chevrons) flip
  correctly.
- Use the theme's CSS variables for colors; never hard-code colors that break
  the light or dark theme.
- Treat model output as untrusted: render Markdown through `ui/core/markdown.js`
  (which escapes HTML) and run previews only in the sandboxed iframe.

## Commit messages

- Imperative mood, short subject (≤ 72 chars), optionally prefixed with the
  feature: `preview: add reload button`, `models: show pull speed`,
  `docs: fix API example`.
- Explain the *why* in the body when it is not obvious.
- One logical change per commit; keep formatting-only changes separate.

## Pull request checklist

Before opening a PR, please make sure that:

- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy` shows no new warnings.
- [ ] `cargo test` passes.
- [ ] `cargo build --release` succeeds.
- [ ] You tested the change manually in dev mode (`--serve 8790`) and/or the
      real window, with Ollama running.
- [ ] The UI works in **both languages** (Arabic RTL and English LTR).
- [ ] The UI works in **both themes** (light and dark).
- [ ] New UI strings exist in both `ar` and `en`.
- [ ] New or changed endpoints are documented in `docs/API.md`.
- [ ] Screenshots or a short GIF are attached for visible UI changes.

## License

By contributing, you agree that your contributions are licensed under the
project's [MIT License](LICENSE).
