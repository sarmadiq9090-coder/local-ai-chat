# API contract (backend ⇄ UI)

All endpoints are served by the Rust backend on the same origin as the UI (`http://127.0.0.1:<random port>`).
JSON in / JSON out unless noted. Errors: non-2xx status with body `{"error": "message"}`.

## App endpoints (`/api/app/...`)

Implemented in `src/server/routes/<feature>.rs` (one file per feature, merged in `routes/mod.rs`).

| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/app/status` | – | `{ "ollama": bool, "version": string \| null, "app_version": string, "data_dir": string }` |
| POST | `/api/app/ollama/start` | – | `{ "ok": bool }` — tries to launch Ollama, waits up to ~15s |
| GET | `/api/app/system` | – | `SystemInfo` — hardware summary for model suggestions; probed once, then cached |
| GET | `/api/app/settings` | – | `Settings` |
| PUT | `/api/app/settings` | `Settings` | `Settings` (saved) |
| GET | `/api/app/conversations` | – | `ConversationMeta[]`, newest `updated_at` first |
| GET | `/api/app/conversations/{id}` | – | `Conversation` (404 if missing) |
| PUT | `/api/app/conversations/{id}` | `Conversation` | `{ "ok": true }` — create or replace |
| DELETE | `/api/app/conversations/{id}` | – | `{ "ok": true }` |
| POST | `/api/app/export` | `{ "filename": string, "content": string }` | `{ "path": string }` — saved to `Documents\LocalAiChat\` and revealed in Explorer |

## Ollama passthrough (`/api/ollama/...`)

`/api/ollama/<anything>` is forwarded verbatim to `<settings.ollama_url>/api/<anything>` (same method, body, query).
Streaming responses (NDJSON from `/chat`, `/pull`) are streamed through unchanged.
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
};

// GPUs: NVIDIA VRAM from nvidia-smi; other adapters from WMI + the driver's registry
// qwMemorySize (null when unknown, e.g. integrated GPUs). ram_gb = total physical RAM.
type SystemInfo = {
  gpus: { name: string; vram_gb: number | null }[];
  ram_gb: number;
  cpu: string | null;        // e.g. "Intel(R) Core(TM) i9-10900 CPU @ 2.80GHz"
};

type ConversationMeta = { id: string; title: string; model: string | null; updated_at: number /* unix ms */ };

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
