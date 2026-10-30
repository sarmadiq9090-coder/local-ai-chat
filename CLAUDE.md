# CLAUDE.md — Local AI Chat

Windows desktop chat client for local models via [Ollama](https://ollama.com). One `.exe`:
a Rust (axum) server on `127.0.0.1:<random port>` serves the UI (vanilla ES modules, embedded
with rust-embed) and a JSON API; a native window (tao + WebView2/wry) shows it. Arabic-first
(RTL) UI. API contract: `docs/API.md`. Plans: `ROADMAP.md`. User docs (Arabic): `README.md`.

## Build & run

```bash
cargo build --release                          # target\release\local-ai-chat.exe (UI embedded)
cargo build                                    # debug build
target\debug\local-ai-chat.exe --serve 8790    # dev mode: server only, open http://127.0.0.1:8790/
cargo check && cargo test
```

- In **debug** builds `--serve` reads `ui/` from disk: UI edits need only a browser refresh.
  Rust edits need `cargo build` + restart.
- The app holds a **single-instance mutex**: launching the exe twice just focuses the first
  window. Test with `--serve <port>` (no window, no mutex), never by launching the exe twice.
- Data: `%APPDATA%\LocalAiChat\` (settings.json, conversations\*.json). Exports: `Documents\LocalAiChat\`.

## Folder map

```
src/
  main.rs  config.rs  error.rs  single_instance.rs  window.rs  ollama.rs  storage.rs  system.rs
  server/
    mod.rs            AppState + router assembly (/api/app, /api/ollama, static UI)
    assets.rs         embedded ui/ files
    ollama_proxy.rs   /api/ollama/* streaming passthrough to Ollama
    routes/
      mod.rs          merges one router per feature (one `mod` + one `.merge` line each)
      status.rs settings.rs conversations.rs export.rs system.rs
ui/
  index.html          markup shell + <link> per feature stylesheet
  main.js             bootstrap: translateDom, icons, then init()/start() of every feature, Esc chain
  core/               shared, rarely edited: api.js state.js dom.js icons.js markdown.js i18n.js
                      strings.js (shared strings) base.css (tokens, reset, shell, buttons, forms, modal, toast)
  features/
    index.js          the ONLY list of features (order = init order and Esc priority)
    chat/ sidebar/ models/ settings/ preview/ status/ exporter/ code/
                      each: <name>.js (default export = feature object), <name>.css, strings.js
  vendor/             highlight.min.js + license (do not edit)
```

Feature object (default export of `features/<name>/<name>.js`):
`{ name, init?(), start?(), onEscape?() }` — `init` = sync wiring, run in list order;
`start` = async initial load, all run in parallel after every `init`; `onEscape` returns `true`
if it handled Esc (asked in list order: model menu → modal → stop generation → preview → mobile sidebar).
Features communicate through `core/state.js` (`state` + `on/emit` events) or by importing another
feature's exported functions; list such cross-feature deps in the file's header comment.

## Adding a feature

UI:
1. Create `ui/features/<name>/` with `<name>.js`, `strings.js`, and `<name>.css` if it has styles.
2. `strings.js`: `export default { ar: {...}, en: {...} }`. In `<name>.js`:
   `import strings from "./strings.js"; registerStrings("<name>", strings);` at module top.
   Keys are global — prefix new ones to stay unique (e.g. `asst_title`).
3. `export default { name: "<name>", init, start, onEscape }` (all optional except `name`).
4. Append the import + entry to `ui/features/index.js` (after existing Esc handlers unless it needs priority).
5. Append `<link rel="stylesheet" href="features/<name>/<name>.css">` in `ui/index.html`.
6. Static markup the feature needs goes in `index.html` only if unavoidable; prefer creating DOM in `init()`.

Backend:
1. Create `src/server/routes/<name>.rs` with `pub fn router() -> Router<AppState>` (paths relative to `/api/app`).
2. Add `mod <name>;` and `.merge(<name>::router())` in `src/server/routes/mod.rs`.
3. Document every endpoint (and types) in `docs/API.md`.
New shared state goes on `AppState` in `server/mod.rs`; new settings fields go in `config.rs` `Settings`
(with a default) and `DEFAULT_SETTINGS` in `features/settings/settings.js`.

## Rules for parallel agents

- Each agent **owns its feature folder(s)** (`ui/features/<name>/`, `src/server/routes/<name>.rs`,
  new `src/<name>/` modules). Do not edit another feature's files; call its exported functions instead.
  If you truly need a change there, keep it minimal and mention it in your report.
- Shared files get **minimal, append-style edits only**, and you **re-read them right before editing**
  (another agent may have just changed them): `ui/core/*`, `ui/features/index.js`, `ui/index.html`,
  `src/server/mod.rs`, `src/server/routes/mod.rs`, `src/main.rs`, `src/config.rs`, `Cargo.toml`,
  `docs/API.md`. Never reformat or reorder them.
- **Never hard-code UI strings.** Put them in the feature's `strings.js` with both `ar` and `en`;
  use `t("key", {vars})` in JS and `data-i18n`, `data-i18n-title`, `data-i18n-placeholder` in markup.
- Commit only your own files (`git add <paths>`, not `git add -A`) — others may have work in progress.
- Behavior/visual changes outside your feature need explicit approval.

## Arabic / RTL rules

- `<html dir="rtl">`; use logical CSS properties (`margin-inline-start`, `inset-inline-end`…), not left/right.
- Every user/model text block gets `dir="auto"` + `unicode-bidi: plaintext; text-align: start`
  so Arabic and English paragraphs each pick their own direction.
- Code is always LTR (`dir="ltr"` on code blocks/inputs for model names, URLs, paths).
- Use colors from theme tokens (`var(--text)`, `--accent`…) defined in `core/base.css`; add new tokens
  for both `:root` (dark) and `[data-theme="light"]`.

## Before reporting done

1. `cargo check` (no new warnings) and `cargo test`.
2. Run `target\debug\local-ai-chat.exe --serve <port>`, load `http://127.0.0.1:<port>/` in a browser,
   exercise the feature, and confirm there are no console errors. Stop the server afterwards.
3. Commit with a message ending in:
   `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
