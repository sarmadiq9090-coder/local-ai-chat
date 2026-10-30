# API contract (backend ⇄ UI)

All endpoints are served by the Rust backend on the same origin as the UI (`http://127.0.0.1:<random port>`).
JSON in / JSON out unless noted. Errors: non-2xx status with body `{"error": "message"}` — including
request rejections under `/api/app` (malformed JSON → 400, wrong field types → 422, missing
`Content-Type` → 415, bad path/query → 400, body over 64 MB → 413, wrong method → 405, unknown
`/api/...` route → 404), the request guard's 403 (foreign `Host`/`Origin`), and a 413 for an oversized
body on `/api/ollama/*`. Some errors add a machine-readable `"code"` (and a technical `"detail"`) so the
UI can show a localized message: `ollama_unreachable` (502), `corrupt` (500), `invalid_ollama_url` (400).

## App endpoints (`/api/app/...`)

Implemented in `src/server/routes/<feature>.rs` (one file per feature, merged in `routes/mod.rs`).

| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/app/status` | – | `{ "ollama": bool, "version": string \| null, "app_version": string, "data_dir": string }` |
| POST | `/api/app/ollama/start` | – | `{ "ok": bool }` — tries to launch Ollama, only for a local `ollama_url` (127.0.0.1 / localhost / ::1; a remote URL just reports whether it answers), waits up to ~20 s after spawning |
| GET | `/api/app/system` | – | `SystemInfo` — hardware summary for model suggestions; probed once, then cached |
| GET | `/api/app/settings` | – | `Settings` |
| PUT | `/api/app/settings` | `Partial<Settings>` | `Settings` (saved) — given fields are merged into the current settings, missing ones keep their value (a full object replaces everything). Not an object → 400; wrong field type → 422; `ollama_url` not an absolute http(s) URL → 400 `{ "error", "code": "invalid_ollama_url" }` (empty → default URL) |
| GET | `/api/app/conversations` | – | `ConversationMeta[]`, newest `updated_at` first. A file that can't be parsed is still listed (`corrupt: true`, see type) so it can be deleted |
| GET | `/api/app/conversations/{id}` | – | `Conversation` (404 if missing; corrupt file → 500 `{ "error": string, "code": "corrupt", "detail": string /* parser message */ }`) |
| PUT | `/api/app/conversations/{id}` | `Conversation` | `{ "ok": true }` — create or replace |
| DELETE | `/api/app/conversations/{id}` | – | `{ "ok": true }` |
| POST | `/api/app/export` | `{ "filename": string, "content": string }` | `{ "path": string }` — saved to `Documents\LocalAiChat\` and revealed in Explorer; an existing file is never overwritten (`name (2).md`, `name (3).md`…) |

## Ollama passthrough (`/api/ollama/...`)

`/api/ollama/<anything>` is forwarded verbatim to `<settings.ollama_url>/api/<anything>` (same method, body, query).
Streaming responses (NDJSON from `/chat`, `/pull`) are streamed through unchanged.
Ollama's own responses (including its errors) pass through unchanged. If Ollama can't be reached:
**502** `{ "error": "Ollama is not reachable", "code": "ollama_unreachable", "detail": string /* reqwest error */ }`.
Useful ones: `GET tags`, `POST show`, `POST chat`, `POST pull`, `DELETE delete`, `GET ps`.

## Types

```ts
type Settings = {
  ollama_url: string;        // default "http://127.0.0.1:11434"
  system_prompt: string;     // default Arabic "answer in user's language" prompt
  temperature: number;       // 0..2, default 0.7
  num_ctx: number;           // context length, default 8192
  theme: "dark" | "light" | "system"; // default "dark"
  default_model: string | null;
  think: boolean;            // default false
  language: "ar" | "en";     // UI language, default "ar" (unknown → "ar"). While system_prompt
                             // equals the other language's built-in default it is swapped to this
                             // language's default on save; a custom prompt is never changed.
  auto_update: boolean;      // check GitHub for a newer release on start + every 6 h, default true
};
// settings.json that can't be parsed is renamed to `settings.json.corrupt-<unix seconds>` (and
// logged) before the defaults are used, so it is never silently overwritten.

// GPUs: NVIDIA VRAM from nvidia-smi; other adapters from WMI + the driver's registry
// qwMemorySize (null when unknown, e.g. integrated GPUs). ram_gb = total physical RAM.
type SystemInfo = {
  gpus: { name: string; vram_gb: number | null }[];
  ram_gb: number;
  cpu: string | null;        // e.g. "Intel(R) Core(TM) i9-10900 CPU @ 2.80GHz"
};

type ConversationMeta = {
  id: string; title: string; model: string | null; updated_at: number /* unix ms */;
  // Only present (true) for a conversation file that isn't valid JSON: then id = file name,
  // title = "", model = null, updated_at = the file's modified time (ms). GET → 500 code "corrupt";
  // DELETE works.
  corrupt?: true;
};

type Conversation = ConversationMeta & {
  created_at: number;        // unix ms
  messages: Message[];
};

type Message = {
  role: "user" | "assistant";
  content: string;
  thinking?: string;
  model?: string;            // assistant only
  stats?: string;            // e.g. "63.5 tokens/s"
  error?: string;
  attachments?: { name: string; content: string }[]; // user only: attached text files
};
```

Unknown extra fields on `Conversation` / `Message` must be preserved by the backend (it stores them as-is),
so the UI can add new fields without backend changes.

## Custom assistants (RAG) — `/api/app/assistants/...`

Implemented in `src/server/routes/assistants.rs` on top of `src/assistants/` (spec: `docs/specs/assistants.md`).
Stored under `%APPDATA%\LocalAiChat\assistants\{id}\` (`assistant.json`, `docs/{doc_id}.txt`, `index.json`).
Errors: unknown assistant/document → **404**; invalid id / empty name / bad upload → **400**;
embedding model not installed in Ollama → **424** `{ "error": string, "missing_model": string }`
(`/docs` and `/reindex` check this via Ollama's `/api/tags` *before* streaming; if Ollama can't be
asked, the stream reports it as an `{error, missing_model}` line instead);
stale knowledge index on `/search` → **409** `{ "error": string, "code": "reindex_required" }`
(built with another embed model or by an older chunker — call `/reindex`).
Limits (checked before anything is embedded): at most **5 MB of extracted text per document** and
**20,000 chunks per assistant**. An oversized note is rejected with **400** before streaming; an
oversized file (known only after extraction) or a full knowledge base ends that document's stream
with `{ error: "document too large: …" | "knowledge base too large: …", name }`.

| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/app/assistants` | – | `Assistant[]`, newest `updated_at` first (no index). A corrupt `assistant.json` is renamed to `assistant.json.corrupt-<unix ms>`, left out, and its id listed in the `X-Corrupt-Assistants` response header (comma-separated) |
| POST | `/api/app/assistants` | `NewAssistant` | `Assistant` |
| GET | `/api/app/assistants/{id}` | – | `Assistant & { index: IndexStatus }` |
| PUT | `/api/app/assistants/{id}` | `AssistantPatch` (only present fields change; `model: null` clears it) | `Assistant`. Changing `embed_model` makes the index stale → call `/reindex` |
| DELETE | `/api/app/assistants/{id}` | – | `{ "ok": true }` — removes all its documents and index (missing is fine) |
| POST | `/api/app/assistants/{id}/docs` | `multipart/form-data` with one or more file fields **or** JSON `{ name, text }` (a note) | NDJSON stream (see below); per document a final `{ doc: DocMeta }` or `{ error, name, missing_model? }` |
| GET | `/api/app/assistants/{id}/docs/{doc_id}` | – | `{ text: string }` — extracted plain text (e.g. to edit a note) |
| DELETE | `/api/app/assistants/{id}/docs/{doc_id}` | – | `{ "ok": true }` (its chunks are removed) |
| POST | `/api/app/assistants/{id}/reindex` | – | NDJSON stream; final `{ assistant: Assistant }` or `{ error, missing_model? }` |
| POST | `/api/app/assistants/{id}/search` | `{ query: string, k?: number }` (k default 5, max 50) | `{ results: SearchResult[] }`, best first — `[]` when nothing passes the score threshold (0.42) or is within 0.15 of the best score; **409** `reindex_required` on a stale index |

File uploads accept `txt md csv json xml yml yaml toml html css js ts py rs go java c cpp h cs php rb sh ps1 sql ini log pdf`.
Unsupported files in a multi-file upload are skipped with an `{ error: "unsupported file type: …", name }`
line while the others are indexed; if no file is supported → 400 before streaming. Files in one upload
are indexed one after another.

NDJSON progress lines: `{ "status": "extracting" | "embedding" | "saving", "done": number, "total": number }`.
Final lines carry no `status`. HTTP status is 200 once streaming starts; failures arrive as `{error}` lines.

```ts
type Assistant = {
  id: string; name: string; icon: string /* emoji, default "🤖" */; description: string;
  instructions: string;
  model: string | null;      // preferred chat model; null = the app's current model
  strict: boolean;           // default true: answer only from the knowledge
  embed_model: string;       // default "bge-m3"
  created_at: number; updated_at: number; // unix ms
  docs: DocMeta[];
};
type DocMeta = { id: string; name: string; kind: "file" | "note"; size: number /* bytes */; chunks: number; added_at: number;
                 warning?: "arabic_pdf_quality" /* Arabic PDF text had to be repaired and may be inaccurate */ };
type NewAssistant = { name: string; icon?: string; description?: string; instructions?: string;
                      model?: string; strict?: boolean; embed_model?: string };
type AssistantPatch = Partial<Omit<NewAssistant, "model">> & { model?: string | null };
type IndexStatus = { embed_model: string; dim: number; chunks: number;
                     stale: boolean /* re-index needed: other embed model or older chunker (index.json "version" < 2) */ };
type SearchResult = { doc_id: string; doc_name: string; text: string; score: number };
```

Conversation/message fields used by assistants (stored as unknown extra fields):
`Conversation.assistant_id?: string` (the chat belongs to an assistant) and
`Message.sources?: { n: number; doc_name: string; snippet: string; score: number }[]` (assistant replies:
the numbered passages given to the model, `n` matches the `[n]` citations).

## Auto-update — `/api/app/update/...`

Implemented in `src/server/routes/update.rs` + `src/update.rs` (spec: `docs/specs/auto-update.md`).
Source: GitHub `releases/latest` of `sarmadiq9090-coder/local-ai-chat` (const `update::REPO`), assets
`local-ai-chat.exe` + `local-ai-chat.exe.sha256` (`<hex>  local-ai-chat.exe`). Dev/test override: env var
`LOCALAI_UPDATE_FEED=<url>` returning JSON shaped like a GitHub release. Asset and checksum URLs
(and redirects) must be `https://`; plain `http://` to `127.0.0.1` / `localhost` is accepted only
while that override is set. Downloads are capped at 200 MB (exe) and 1 KB (`.sha256`).

| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/app/update/check` | – | `UpdateCheck`. Query: `force=1` bypasses the 10-min cache; `local=1` skips the network (cached result or just `current`). Network problems never give a 5xx: `available: false` + `error`. No release / repo not found (404) → `available: false`, no `error`. |
| POST | `/api/app/update/install` | – | NDJSON stream: `{ "status": "downloading", "downloaded": number, "total": number \| null }` … `{ "status": "verifying" }` … `{ "status": "restarting" }`, or a final `{ "error": string }` (no update, insecure URL, download failed or too large, checksum mismatch, swap failed — the current exe is left in place). After `restarting` the exe was swapped (old one → `local-ai-chat.old.exe`), the new exe was started with `--after-update` (plus `--serve <port>` in dev mode) and this process exits ~1 s later. |

```ts
type UpdateCheck = {
  current: string;            // this build, e.g. "2.1.0"
  latest?: string;            // latest release version without "v"
  available: boolean;         // latest > current (semver)
  notes?: string;             // release notes (Markdown)
  published_at?: string;      // ISO 8601
  error?: string;             // check failed (offline, rate limit, bad release)
  just_updated?: string;      // set once after a restart into a new version (from --after-update)
};
```

## Static UI (everything else)

Embedded `ui/` files. Every response has `X-Content-Type-Options: nosniff`. `/` (`index.html`) also
gets `Content-Security-Policy` = the policy of its own `<meta http-equiv="Content-Security-Policy">`
(read from the file, so the two never diverge) plus `frame-ancestors 'none'`.
`features/preview/frame.html` gets only `frame-ancestors 'self'` — never the page policy, because
previews run inline and CDN scripts inside it.
