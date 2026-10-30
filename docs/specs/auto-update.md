# Spec: Auto-update from GitHub Releases

The user's words: *"Make an auto-update button."*

## Behavior
1. On startup (and at most once every 6 h while open) the app checks the latest GitHub release of `REPO` (const, e.g. `sarmadiq9090-coder/local-ai-chat`).
2. If `latest.tag_name` (semver, `v2.1.0`) > `CARGO_PKG_VERSION`, the sidebar footer shows a highlighted **"تحديث متاح v2.1.0 / Update available"** button. Settings also has "Check for updates" + an "auto-check" toggle (default on).
3. Clicking it opens a modal: version, release notes (rendered Markdown), **"تحديث الآن / Update now"**.
4. Update now → backend downloads the release asset `local-ai-chat.exe` with a progress bar (NDJSON stream), verifies it against the `local-ai-chat.exe.sha256` asset, then:
   - renames the running exe to `local-ai-chat.old.exe` (Windows allows renaming a running exe),
   - writes the new exe in its place,
   - spawns the new exe with `--after-update` and exits.
5. On start with `--after-update`, the app deletes `local-ai-chat.old.exe` (retry a few times — the old process may still be exiting; the single-instance mutex must be released first, so the new process waits up to ~5 s for it) and shows a toast "تم التحديث إلى v2.1.0".
6. Failures (no internet, rate limit, bad checksum) never break the app: show a toast and keep the current version. If writing the new exe fails, rename `.old` back.

## Backend
- `src/update.rs`: `check() -> Option<ReleaseInfo { version, notes, asset_url, sha_url }>`, `install(progress_tx)`.
- HTTPS needs TLS for reqwest: enable `rustls-tls` (or `default-tls`) feature — Ollama calls stay plain HTTP.
- GitHub API: `GET https://api.github.com/repos/{REPO}/releases/latest` with a `User-Agent` header; no token.
- Routes (`src/server/routes/update.rs`): `GET /api/app/update/check` → `{ current, latest?, available, notes?, published_at? }`; `POST /api/app/update/install` → NDJSON `{status, downloaded, total}` … `{status:"restarting"}`.

## Release side (CI)
GitHub Actions workflow on tag `v*`: `cargo build --release` on `windows-latest`, upload `local-ai-chat.exe` and `local-ai-chat.exe.sha256` to the release.

## UI
Feature folder `ui/features/update/` (button, modal, settings section) with ar + en strings.
