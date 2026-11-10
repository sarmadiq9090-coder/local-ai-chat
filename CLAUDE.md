# CLAUDE.md — Lemon (Local AI Chat)

## Owner & workflow (shared rules, same in every repo)

Owner: Sarmad (`sarmadiq9090-coder`). Writes Arabic, is not technical in English, and works only
through Claude sessions, with no fixed computer.

- Reply to the owner in Arabic. Keep code, identifiers, commit messages and PR titles in English,
  and give PR bodies an Arabic summary first.
- Never push to the default branch. Work on `claude/<topic>`, open a PR, and let the owner merge
  unless they explicitly say "ادمج" (merge).
- One Claude session per repository at a time. Before starting, check open PRs and recently pushed
  `claude/*` branches. If another session is active, coordinate through it instead of editing the
  same files.
- The owner has no fixed computer: never ask them to keep files, keys or tools on a PC. Use GitHub
  (Environments/secrets, Codespaces) or their password manager.
- **Daily release train (D-035, D-047).** Each app ships once a week on its day at 09:00 Asia/Baghdad:
  Sat InstaFind, Sun Remember Me, Mon Lemon, Tue InstaWords, Wed InstaConvert (only when complete),
  Thu QA, Fri rest. ALL of an app's finished, green features merge together on its day (D-032/D-034);
  nothing is merged on other days. An app not ready on its day rolls to the same day next week; its
  Store update ships the same day as the release. The site (fastkiwi.app) is exempt: it publishes
  when green (D-046). A critical security fix may go early with the owner's explicit approval.
- **One GitHub writer (D-040, core/GITHUB-SAFETY.md in the brain).** Only the manager session pushes,
  opens PRs, comments, merges or dispatches workflows; other sessions work locally and report to the
  manager. Serial writes >=2 s apart, <=15 per hour, <=150 per day, <=3 new PRs per repo per day, <=2
  workflow_dispatch per repo per day, 1 rerun max. On any 403/429/abuse/CAPTCHA signal: stop all
  writes and tell the owner.
- **Closed source (D-051, 2026-11-09).** FastKiwi apps are proprietary (free to use; pricing
  undecided). Don't add "open source"/MIT wording; keep third-party notices. Code published before
  2026-11-09 stays under its MIT licence.
- **Mirror.** `.github/workflows/mirror.yml` pushes main, tags and claude/* daily to GitLab
  (`fastkiwi1/local-ai-chat`), no force, no prune; it skips until the owner adds the `GITLAB_DEPLOY_KEY` secret.
- Things only the owner can do: repo settings, deleting branches or repos, money decisions, secrets,
  merging. Ask, and give exact click-by-click steps in Arabic.
- Don't enable features that write to third-party systems, or anything security-sensitive, without
  the owner's explicit approval.
- Commit only your own files (`git add <paths>`).
- Commit trailer: `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`
  and `Claude-Session: https://claude.ai/code/session_01H59LCK6b45r38j6yGz7oef`.

Windows desktop chat client for local models via [Ollama](https://ollama.com). One `.exe`:
a Rust (axum) server on `127.0.0.1:<random port>` serves the UI (vanilla ES modules, embedded
with rust-embed) and a JSON API; a native window (tao + WebView2/wry) shows it. Arabic-first
(RTL) UI. API contract: `docs/API.md`. Plans: `ROADMAP.md`. User docs (Arabic): `README.md`.

**Naming:** the product name is **Lemon** (window title, UI, docs). For compatibility with existing
installs and auto-update, these keep the old technical names — do NOT rename them: the exe /
release asset `local-ai-chat.exe`, the crate `local-ai-chat`, the GitHub repo
`sarmadiq9090-coder/local-ai-chat` (`update::REPO`), and the data folders `LocalAiChat`.

## Build & run

```bash
cargo build --release                          # target\release\local-ai-chat.exe (UI embedded)
cargo build                                    # debug build
target\debug\local-ai-chat.exe --serve 8790    # dev mode: server only, open http://127.0.0.1:8790/
cargo check && cargo test
```

- In **debug** builds `--serve` reads `ui/` from disk: UI edits need only a browser refresh.
  Rust edits need `cargo build` + restart.
- **Parallel agents:** a running exe locks `target\debug\local-ai-chat.exe` and breaks everyone's
  `cargo build`. So never run it in place: copy it first and run the copy on your own port, e.g.
  `cp target/debug/local-ai-chat.exe target/dev-<you>.exe && target/dev-<you>.exe --serve 879x`
  (the copy still reads `ui/` from disk). Stop it when done.
- The server rejects requests whose `Host`/`Origin` isn't `127.0.0.1:<port>` / `localhost:<port>`
  (`server/guard.rs`), so open the UI via one of those exact hosts.
- The app holds a **single-instance mutex**: launching the exe twice just focuses the first
  window. Test with `--serve <port>` (no window, no mutex), never by launching the exe twice.
- `LOCALAI_DATA_DIR=<folder>` puts all data (settings, conversations, assistants, WebView2
  profile, exports) in that folder — use it for tests/screenshots so the user's real data is
  never touched or shown.
- Data: `%APPDATA%\LocalAiChat\` (settings.json, conversations\*.json, assistants\{id}\ with
  assistant.json, docs\*.txt, index.json). Exports: `Documents\LocalAiChat\`.

## Folder map

```
src/
  main.rs  config.rs  error.rs  single_instance.rs  window.rs  ollama.rs  storage.rs  system.rs
  update.rs           auto-update from GitHub Releases (check, download + sha256 verify, exe swap)
  assistants/         custom assistants + knowledge base (RAG): extract, chunk, embed, index, types
  server/
    mod.rs            AppState + router assembly (/api/app, /api/ollama, static UI)
    assets.rs         embedded ui/ files
    guard.rs          rejects requests whose Host/Origin isn't this server (CSRF / DNS rebinding)
    ollama_proxy.rs   /api/ollama/* streaming passthrough to Ollama
    routes/
      mod.rs          merges one router per feature (one `mod` + one `.merge` line each)
      status.rs settings.rs conversations.rs export.rs system.rs assistants.rs update.rs
ui/
  index.html          markup shell + <link> per feature stylesheet
  main.js             bootstrap: translateDom, icons, then init()/start() of every feature, Esc chain
  core/               shared, rarely edited: api.js state.js dom.js icons.js markdown.js i18n.js
                      strings.js (shared strings) base.css (tokens, reset, shell, buttons, forms, modal, toast)
  features/
    index.js          the ONLY list of features (order = init order and Esc priority)
    chat/ sidebar/ models/ settings/ preview/ status/ exporter/ code/ assistants/ update/
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

## Releasing

A release happens by **merging a PR into `main` that bumps `version` in `Cargo.toml`**
(on the app's train day only), with a matching `## [x.y.z]` section in `CHANGELOG.md` (it becomes the
release notes). No tag push is needed: `.github/workflows/release.yml` sees that tag
`v<version>` doesn't exist, builds, waits for the owner's approval in the `release` Environment,
signs, and `gh release create v<version> --target <sha>` makes the tag. A merge with an unchanged
version only runs a quick version check. Never bump the version outside a release PR, and never
push tags. Owner steps (Arabic): `RELEASING.md`; details: `docs/specs/signing.md`.

## Before reporting done

1. `cargo check` (no new warnings) and `cargo test`.
2. Run `target\debug\local-ai-chat.exe --serve <port>`, load `http://127.0.0.1:<port>/` in a browser,
   exercise the feature, and confirm there are no console errors. Stop the server afterwards.
3. Commit with a message ending in:
   `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
