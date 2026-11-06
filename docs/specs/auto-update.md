# Spec: Auto-update from GitHub Releases

The user's words: *"Make an auto-update button."*

## Behavior
1. On startup (and at most once every 6 h while open) the app checks the latest GitHub release of `REPO` (const, e.g. `sarmadiq9090-coder/local-ai-chat`).
2. If `latest.tag_name` (semver, `v2.1.0`) > `CARGO_PKG_VERSION`, the sidebar footer shows a highlighted **"تحديث متاح v2.1.0 / Update available"** button. Settings also has "Check for updates" + an "auto-check" toggle (default on).
3. Clicking it opens a modal: version, release notes (rendered Markdown), **"تحديث الآن / Update now"**.
4. Update now → backend downloads the release asset `local-ai-chat.exe` with a progress bar (NDJSON stream), verifies it against the `local-ai-chat.exe.sha256` asset and — once release-signing keys are compiled in — against the minisign signature `local-ai-chat.exe.minisig` (see **Signature** below), then:
   - renames the running exe to `local-ai-chat.old.exe` (Windows allows renaming a running exe),
   - writes the new exe in its place,
   - spawns the new exe with `--after-update` and exits.
5. On start with `--after-update`, the app deletes `local-ai-chat.old.exe` (retried for ~10 s in the background — the old process may still be exiting). The window build waits up to **30 s** for the old instance's single-instance mutex (`single_instance::acquire_wait`); if it is still held, a native error box (Arabic + English) explains that the previous version is still running instead of exiting silently. In `--serve` dev mode the new process waits up to ~10 s for the old one to free the port (`update::wait_for_port`) and shows a toast "تم التحديث إلى v2.1.0".
6. Failures (no internet, rate limit, bad checksum, missing/invalid signature) never break the app: show a toast and keep the current version. If writing the new exe fails, rename `.old` back.

## Signature (spec: `docs/specs/signing.md`)
- `update::TRUSTED_PUBLIC_KEYS` (compile-time constant; primary + backup minisign public keys). **Empty** (as shipped until the owner's keys are pasted in): sha256 only, exactly as 2.2.0, and the log says signature checking is not configured.
- **Not empty:** after the sha256 check, download `local-ai-chat.exe.minisig` (≤ 4 KB, same URL rules) and require a pre-hashed minisign signature over the exe bytes by one of the keys, with the trusted comment **exactly** `local-ai-chat v<version>` (the release tag). Missing, unreadable, other-key, legacy or wrong-version signatures are refused: the exe is never written, the install stream ends with `{ "error", "code": "signature" }` and the UI shows "couldn't be verified" (ar/en) with links to the releases page and https://fastkiwi.app/. No fallback, no runtime or server switch; `LOCALAI_UPDATE_FEED` follows the same rules.

## Backend
- `src/update.rs`: `fetch_latest() -> Result<Option<ReleaseInfo { version, notes, published_at, asset_url, sha_url, sig_url? }>>` (+ `parse_release`, `is_newer`), `install(&release, progress)` (download → verify → `swap_exe` → relaunch, `rollback` on failure), `after_update()`, `take_just_updated()`, `request_exit()`.
- Download hardening: asset/checksum URLs and every redirect must be `https://` (`check_download_url`; plain `http://` to `127.0.0.1`/`localhost` only while `LOCALAI_UPDATE_FEED` is set); the exe is capped at 200 MB and the `.sha256` at 1 KB (`read_capped`). Everything is verified in memory before the running exe is touched.
- HTTPS needs TLS for reqwest: the `rustls` feature is enabled (`default-features = false`) — Ollama calls stay plain HTTP.
- GitHub API: `GET https://api.github.com/repos/{REPO}/releases/latest` with a `User-Agent` header; no token.
- Routes (`src/server/routes/update.rs`): `GET /api/app/update/check` → `{ current, latest?, available, notes?, published_at? }`; `POST /api/app/update/install` → NDJSON `{status, downloaded, total}` … `{status:"restarting"}`.

## Release side (CI)
`.github/workflows/release.yml` on tag `v*`: `build` (windows, no secrets) → `sign` (`release` Environment, owner approval, minisign) → `publish` (attestation, draft release with `local-ai-chat.exe` + `local-ai-chat.exe.sha256` + `local-ai-chat.exe.minisig`, all three checked, then published). The first two asset names must never change: 2.2.0 and older clients require both. Details: `docs/specs/signing.md`.

## UI
Feature folder `ui/features/update/` (button, modal, settings section) with ar + en strings.
