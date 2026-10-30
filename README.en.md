# Local AI Chat

[العربية](README.md) · **English**

**Local AI Chat** (Arabic: «الذكاء المحلي») is a lightweight Windows desktop app
for chatting and coding with AI models that run **entirely on your own PC**
through [Ollama](https://ollama.com). It is a single `.exe` with its own
window, an Arabic-first (RTL) interface with an English translation, and no
cloud services, accounts or telemetry.

Under the hood it is a small Rust program: an [axum](https://github.com/tokio-rs/axum)
server on `127.0.0.1` serves an embedded HTML/JS UI to a native
[tao](https://github.com/tauri-apps/tao) + [wry](https://github.com/tauri-apps/wry)
(WebView2) window, and proxies requests to your local Ollama.

[![CI](https://github.com/sarmadiq9090-coder/local-ai-chat/actions/workflows/ci.yml/badge.svg)](https://github.com/sarmadiq9090-coder/local-ai-chat/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Features

Available now (v2.1):

- One portable `.exe`, its own window and icon; only one instance runs at a time.
- ChatGPT/Claude-style chat UI in **Arabic and English** — switch live in Settings,
  with correct right-to-left / left-to-right layout everywhere.
- **Custom assistants with a knowledge base (RAG)** — give an assistant your files
  (PDF, text, code) and notes; it retrieves the relevant passages, answers **only**
  from them in strict mode, and shows its sources under each answer.
  Uses a local multilingual embedding model (`bge-m3`).
- **Auto-update** from GitHub Releases with one click, verified with SHA-256
  (can be turned off in Settings).
- Streaming answers with stop, copy, regenerate and edit-and-resend.
- Code blocks with syntax highlighting (bundled highlight.js, works offline).
- Live preview: run HTML/JS/SVG code (pages, small games) in a sandboxed side panel;
  console output and runtime errors are captured, with an "ask for a fix" button.
- Conversations saved on disk, with search, rename and delete.
- Attach text and source-code files to a message (button or drag & drop).
- Export a conversation to Markdown.
- Settings: system prompt, temperature (creativity), context length, light/dark/system theme.
- Model manager: download (pull) and delete Ollama models from inside the app,
  with a suggested-models catalog rated against your GPU/RAM.
- Starts Ollama automatically if it is installed but not running.

- The local server only accepts requests from the app itself (Host/Origin check
  against other websites and DNS rebinding).

Planned (see [ROADMAP.md](ROADMAP.md), Arabic):

- Tool calling (read/write files and run commands in a project folder, with
  your approval for each step), image input for vision models, prompt
  templates, tray icon and quick-ask window, installer, GPU/memory monitor.

## Screenshots

_Screenshots coming soon._ They will live in [`docs/screenshots/`](docs/screenshots/).

<!--
![Chat](docs/screenshots/chat.png)
![Live preview](docs/screenshots/preview.png)
![Model manager](docs/screenshots/models.png)
-->

## Requirements

- **Windows 10 or 11** (64-bit).
- **Microsoft Edge WebView2 Runtime** — preinstalled on Windows 11 and on
  up-to-date Windows 10. If missing, get the
  [Evergreen runtime](https://developer.microsoft.com/microsoft-edge/webview2/).
- **[Ollama](https://ollama.com/download)** installed locally (the app talks to
  `http://127.0.0.1:11434` by default; changeable in Settings).
- **Recommended:** an NVIDIA/AMD GPU with 8 GB+ VRAM and 16 GB+ RAM. Small
  models (1–4B parameters) also run on CPU only, just slower. The in-app model
  catalog tells you which models fit your hardware.

## Install

1. Install [Ollama](https://ollama.com/download).
2. Download `local-ai-chat.exe` from the
   [latest release](https://github.com/sarmadiq9090-coder/local-ai-chat/releases/latest).
   Optionally verify it against `local-ai-chat.exe.sha256`:
   ```powershell
   (Get-FileHash .\local-ai-chat.exe -Algorithm SHA256).Hash
   ```
3. Run it. No installer is needed; put the exe wherever you like.
4. Open **Models** in the app and download a model (for example a small
   `qwen` or `gemma` model to start).

> Windows SmartScreen may warn about an unsigned app downloaded from the
> internet. Choose "More info → Run anyway", or build it yourself from source.

## Build from source

Prerequisites:

- [Rust](https://rustup.rs/) stable with the **MSVC** toolchain
  (`x86_64-pc-windows-msvc`, the rustup default on Windows).
- **Visual Studio Build Tools** with the "Desktop development with C++"
  workload (MSVC linker + Windows SDK).

```powershell
git clone https://github.com/sarmadiq9090-coder/local-ai-chat.git
cd local-ai-chat
cargo build --release
# -> target\release\local-ai-chat.exe
```

The UI in `ui/` is embedded into the exe at build time, so the release binary
is fully self-contained.

### Development mode

Run only the server, without the native window, and open the UI in any browser
(with its dev tools):

```powershell
cargo build
target\debug\local-ai-chat.exe --serve 8790
# then open http://127.0.0.1:8790/
```

In debug builds the UI files are read from disk, so after editing files in
`ui/` a browser refresh is enough.

```powershell
cargo test             # unit tests
cargo fmt              # formatting
cargo clippy           # lints
```

## Where your data lives

| What | Location |
|---|---|
| Conversations | `%APPDATA%\LocalAiChat\conversations\` (one JSON file each) |
| Settings | `%APPDATA%\LocalAiChat\settings.json` |
| Assistants + knowledge | `%APPDATA%\LocalAiChat\assistants\` |
| WebView2 profile / cache | `%LOCALAPPDATA%\LocalAiChat\webview\` |
| Exports | `Documents\LocalAiChat\` |
| Models | Managed by Ollama (default `%USERPROFILE%\.ollama\models`) |

To reset the app, close it and delete the `LocalAiChat` folders above. Your
models are not affected.

## Privacy

- Everything runs locally. Your prompts, files and conversations never leave
  your computer; they go only to the Ollama server on your machine.
- The app's server listens on `127.0.0.1` only, so it is not reachable from
  your network.
- **No telemetry, analytics, accounts or ads.**
- The only internet traffic is what you ask for: downloading models (done by
  Ollama from its registry) and the auto-update check against this
  repository's GitHub Releases (on start and every 6 hours; can be turned off
  in Settings).

## Project structure

See [CLAUDE.md](CLAUDE.md) for the architecture, the feature-folder layout
(`src/server/routes/<feature>.rs`, `ui/features/<feature>/`) and conventions,
and [docs/API.md](docs/API.md) for the backend ⇄ UI API contract.

## Contributing

Contributions, bug reports and translations are welcome. Please read
[CONTRIBUTING.md](CONTRIBUTING.md) first. Security issues: see
[SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © 2026 Sarmad.

Third-party components and their licenses are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Ollama and AI models are not
part of this project; each model has its own license, which you accept when
you download it.
