# Spec: Custom assistants with a knowledge base (RAG)

Roadmap item 3.0. The user's words: *"I want to train an AI the way I see fit; it retrieves and answers only from what I give it."*

## User stories
1. I create an **assistant**: name, emoji icon, description, instructions, preferred model.
2. I give it **knowledge**: upload files (txt, md, csv, json, code files, **pdf**) and/or write notes / Q&A directly in the app.
3. I chat with it from the sidebar. On every question it **retrieves** the most relevant passages from its knowledge and answers from them.
4. **Strict mode** (on by default): it answers **only** from the knowledge. If the answer isn't there it says so (Arabic: «لا توجد هذه المعلومة في المعرفة التي أُعطيت لي.» / English: "That information isn't in the knowledge I was given.") — no guessing.
5. Under each answer I see the **sources** (file name + snippet) it used, collapsible.
6. I can add/remove files, edit notes, and re-index; I see indexing progress and chunk counts.

## Storage (backend)
`%APPDATA%\LocalAiChat\assistants\{id}\`
- `assistant.json` — `{ id, name, icon, description, instructions, model, strict, embed_model, created_at, updated_at, docs: [{ id, name, kind: "file"|"note", size, chunks, added_at }] }`
- `docs/{doc_id}.txt` — extracted plain text of each document (notes stored the same way)
- `index.json` — `{ embed_model, dim, chunks: [{ doc_id, i, text, vec: [f32] }] }` (fine for thousands of chunks; can move to a binary format later)

Ids: same safe charset rule as conversations (`ConversationStore::valid_id`).

## Indexing
- Text extraction: plain text files as UTF-8 (lossy); **PDF via the `pdf-extract` crate** (run in `spawn_blocking`); reject other binary types with a clear error.
- Chunking: ~800 characters with ~150 overlap, split preferably on paragraph/sentence boundaries (Arabic `.؟!` and Latin `.?!` and newlines).
- Embeddings: Ollama `POST /api/embed` `{ model, input: [chunks...] }` in batches of 32. Default embed model **`bge-m3`** (multilingual, strong Arabic, ~1.2 GB). If it isn't installed, the UI offers a one-click pull (reuse the models feature's pull/progress).
- Changing an assistant's embed model ⇒ full re-index.
- Limits, checked before embedding: 5 MB of extracted text per document (`MAX_DOC_TEXT_BYTES`) and 20,000 chunks per assistant (`MAX_CHUNKS`); exceeding either fails with a clear "too large" error (400 / `{error}` line).

## Retrieval (per question)
- Embed the user's question → cosine similarity vs all chunks → top-k (default 5) with a minimum score (`index::MIN_SCORE` = 0.5, tuned on `bge-m3`); also a cheap keyword-overlap boost so exact names/numbers still match.
- If strict mode and nothing passes the threshold → the backend still returns `[]`; the UI sends the prompt with an empty context so the model replies with the "not in my knowledge" sentence (instructions enforce it).

## Prompt assembly (UI side, like normal chat)
System message = assistant instructions + (strict ? STRICT_RULES : SOFT_RULES) + the retrieved passages:
```
[المصدر 1: file.pdf]
...text...
[المصدر 2: notes]
...
```
STRICT_RULES (in the language of the UI, but tell the model to reply in the user's language): answer only from the sources; quote/cite the source number like [1]; if the answer isn't in the sources, reply exactly with the "not in knowledge" sentence; never use outside knowledge.
Assistant reply messages store `sources: [{ n, doc_name, snippet, score }]` (extra message field — the backend preserves unknown fields).

## API (to add to docs/API.md)
| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/app/assistants` | – | `Assistant[]` (without index) |
| POST | `/api/app/assistants` | `{ name, icon?, description?, instructions?, model?, strict?, embed_model? }` | `Assistant` |
| GET/PUT/DELETE | `/api/app/assistants/{id}` | PUT: partial fields | `Assistant` / `{ok}` |
| POST | `/api/app/assistants/{id}/docs` | multipart file(s) **or** JSON `{ name, text }` for a note | NDJSON progress stream: `{status, done, total}` … final `{doc}` |
| DELETE | `/api/app/assistants/{id}/docs/{doc_id}` | – | `{ok}` (removes its chunks) |
| POST | `/api/app/assistants/{id}/reindex` | – | NDJSON progress |
| POST | `/api/app/assistants/{id}/search` | `{ query, k? }` | `{ results: [{ doc_id, doc_name, text, score }] }` |

(axum multipart needs the `multipart` feature; the 64 MB body limit already applies.)

## UI (feature folder `ui/features/assistants/`)
- Sidebar: an "المساعدون / Assistants" section above conversations (list with icon + name, "+ new").
- Assistant editor modal/page: fields above, knowledge list (drag-and-drop files, "add note" textarea), per-doc chunk count + delete, re-index button with progress, embed-model status (installed? → pull button).
- Chatting: a conversation can have `assistant_id`; new chat from an assistant sets it; the top bar shows the assistant chip; the chat feature calls `/search` before sending and builds the system message; sources render as a collapsible list under the reply.
- All strings in `strings.js` with **ar + en**.

## Done when
- Create assistant → add a PDF + a note → ask a question answered in them → correct answer with sources.
- Ask something not in them (strict) → the exact "not in knowledge" sentence.
- Works in Arabic and English; restart the app → assistants and indexes persist.
- `cargo check`/`cargo test` pass; unit tests for chunking and cosine top-k.
