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
- `index.json` — `{ version, embed_model, dim, chunks: [{ doc_id, i, text, vec: [f32] }] }` (fine for thousands of chunks; can move to a binary format later). `version` = `index::INDEX_VERSION` (2 since v2.1.1, the structural chunker); an index with another version (or none) is **stale**: `/search` answers 409 `reindex_required`, the editor shows a "re-index" banner and the chat offers to re-index.

Ids: same safe charset rule as conversations (`ConversationStore::valid_id`).

## Indexing
- Text extraction: plain text files as UTF-8 (lossy); **PDF via the `pdf-extract` crate** (run in `spawn_blocking`); reject other binary types with a clear error (in a multi-file upload only that file is skipped; the UI also filters by extension first).
- **Arabic PDFs:** many writers store Arabic as shaped glyphs in visual order, so `pdf-extract` returns presentation forms (U+FB50–U+FDFF, U+FE70–U+FEFF), each line reversed, plus U+FFFD. `extract::fix_arabic_pdf` reverses word order per line (keeping runs of numbers/Latin in their order), reverses the characters of Arabic words (mirroring brackets), maps presentation forms to base letters (NFKC; Farsi-yeh forms → ي), re-joins fragments using the glyph shapes (a medial/final first letter joins the previous word, an initial/medial last letter the next one; stray isolated one-letter fragments join the previous word; a lone و joins the next) and drops U+FFFD. Words split after non-joining letters (ر ز و…) stay split, so such documents get `DocMeta.warning = "arabic_pdf_quality"` and the UI suggests uploading a .txt/.md copy. `pdf-extract` also separates every line with a blank line; `extract::pdf_paragraphs` rebuilds paragraphs (new paragraph at short heading-like lines).
- Chunking (`chunk.rs`, structure first): Markdown headings start sections; in a file without headings every blank-line paragraph is its own section, so a small multi-topic file (FAQ, price list) is not one diluted chunk. Sections < 80 chars merge with the next. Inside a section paragraphs are packed up to 600 chars; a longer paragraph is split by lines (list items, PDF lines), a longer line by sentences (Arabic `.؟!` and Latin `.?!`, ~500 chars, ~100 overlap). Continuation chunks repeat the section heading, and chunks that continue a split paragraph start with ~100 chars of overlap. Chunks never cross sections.
- Embeddings: Ollama `POST /api/embed` `{ model, input: [chunks...] }` in batches of 32. Default embed model **`bge-m3`** (multilingual, strong Arabic, ~1.2 GB). If it isn't installed, the UI offers a one-click pull (reuse the models feature's pull/progress).
- Changing an assistant's embed model ⇒ full re-index.
- Limits, checked before embedding: 5 MB of extracted text per document (`MAX_DOC_TEXT_BYTES`) and 20,000 chunks per assistant (`MAX_CHUNKS`); exceeding either fails with a clear "too large" error (400 / `{error}` line).

## Retrieval (per question)
- Query = the last user message; when it is a short follow-up (< 60 chars, e.g. "and used products?") the UI prepends the previous user message and the first 200 chars of the last reply.
- Embed the query → cosine similarity vs all chunks + a cheap keyword-overlap boost (≤ 0.15) so exact names/numbers still match → keep results with score ≥ `index::MIN_SCORE` (**0.42**) and within `index::RELATIVE_WINDOW` (0.15) of the best → top-k (default 5).
- Threshold tuned with `cargo test retrieval_eval_live -- --ignored --nocapture` (real `bge-m3`, AR/EN questions vs an Arabic FAQ, an English handbook, a dense Arabic policy paragraph and an off-topic recipe):

  | question → knowledge | best relevant score | best other score |
  |---|---|---|
  | AR → AR FAQ («ما هي سياسة الإرجاع؟») | 0.71–0.81 | ≤ 0.56 |
  | EN → AR FAQ ("What is the return policy?") | 0.65–0.67 | ≤ 0.54 |
  | AR → EN handbook («كم يوم إجازة سنوية؟») | 0.61–0.69 | ≤ 0.67 |
  | EN → EN handbook | 0.80–0.82 | ≤ 0.57 |
  | EN → dense AR paragraph ("overtime pay rate?") | 0.50–0.60 | ≤ 0.64 |
  | unrelated (capital of France, World Cup, poem) | – | 0.27–0.38 |
  | near-miss (chocolate cake, dollar exchange rate) | – | 0.43–0.45 |

  With the old one-chunk-per-small-file index the same kind of questions scored 0.43–0.49 and the former threshold 0.5 rejected them. Near-misses may pass 0.42; in strict mode the model then answers "not in knowledge" and the UI hides the sources list for that reply.
- If strict mode and nothing passes the threshold → the backend still returns `[]`; the UI sends a system message that says there are **no** sources, forbids citations and using earlier messages, and requires the "not in my knowledge" sentence. Replies without sources have stray `[n]` citations removed when rendered; replies that are exactly the not-in-knowledge sentence don't show a sources list.

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

## Deep understanding (v2.2)

The user's words: *"they take the information from them and answer from them **after understanding them excellently**."*
Top-k chunks answer direct lookups but not questions that need the whole document (summaries, comparisons, conditions spread over sections). Per-assistant switch `deep` (on for new assistants, off for assistants from older versions until enabled; enabling it offers to digest the existing documents).

- **Digests** (`src/assistants/digest.rs`): after a document is indexed, the assistant's chat model (else the UI's model, else the app default) writes a structured digest in the document's language — overview, document structure, key facts/numbers (quoted exactly), conditions/exceptions, definitions — saved as `docs/{doc_id}.digest.md`. Long documents are map-reduced: ~7,000-char windows on paragraph boundaries → one digest each → merged in groups of ≤ 9,000 chars until one remains. Calls are streamed with `think: false`, temperature 0.2, `num_ctx` = the assistant's answer context (so Ollama doesn't reload the model), 300 s timeout each. The digest is chunked by the structural chunker (each piece prefixed with the document title) and embedded as index chunks with `kind: "digest"`. Progress: `{status: "digesting", done, total}`; closing the stream cancels. A missing model / timeout / cancel never blocks indexing: the document stays indexed and `DocMeta.digest = {status: "failed", error}` (a previous good digest is kept). The user can view, edit (`status: "edited"`, re-embedded) and regenerate it. Re-index re-embeds existing digests; `/search` ignores digest chunks.
- **Per turn** (`POST /context`, `src/assistants/deep.rs`):
  1. *Query rewriting*: if there is an earlier user turn and the last message is ≤ 12 words, opens like a follow-up («وماذا عن…», "and …") or contains a reference word (it/that/same/«هذه/نفس/عنه»…), the chat model rewrites it into a standalone query in the same language (non-streaming semantics, temperature 0.1, ≤ 60 tokens, thinking off, last two turns as context, replies cut to 300 chars; cached per conversation tail). Original + rewritten queries are both searched (best score per chunk). On failure the previous question is joined to the message instead.
  2. *Search*: same scoring as `/search` (cosine + keyword boost, `MIN_SCORE` 0.42, `RELATIVE_WINDOW` 0.15) over document and digest chunks, up to 10 hits.
  3. *Budget*: `num_ctx` = the assistant's `num_ctx` or 16,384 in deep mode; knowledge budget = min(70 % of `num_ctx`, `num_ctx` − history − 500 − answer room (3,000 with thinking, 1,500 without)), at least `num_ctx`/8. Tokens are estimated (Arabic ≈ 2.5 chars/token, other ≈ 3.5).
  4. *Packing*: full digests of the (≤ 3) best-matching documents first, together ≤ 35 % of the budget; then each hit with its neighbours (i−1, i+1) by score, falling back to the bare hit when the window doesn't fit. Touching chunks merge into one passage (repeated headings / overlap removed); passages are ordered by document (best first), digest first, then by position.
  5. *Answer*: the UI sends `options.num_ctx` and `think` (on in deep mode when the model supports it, overridable per assistant) and stronger rules (read all passages, reconcile, answer completely, copy numbers/names exactly, cite every claim, say what isn't covered). Strict mode is unchanged: no passages → the exact not-in-knowledge sentence.
- **Editor**: deep toggle + advanced (`num_ctx`, thinking auto/on/off; warning above 16k: an 8B model may not fit 8 GB VRAM), instruction templates (legal, tutor, support, research, HR, technical docs — ar/en, `GET /templates`), per-document "what did it read?" dialog (pages/chars/chunks/warnings, paged extracted text, digest view/edit/regenerate with progress + cancel), digest badges and "write digests" for missing ones, and a test panel (rewritten query, passages with scores and digest/expansion badges, streamed answer, timings; nothing saved).
- **Extraction**: PDFs record `pages`; a PDF with < 80 chars of text per page gets `warning: "scanned_pdf"` (no OCR; the UI suggests an OCR'd copy); one with no text at all fails with that hint.
- Compatibility: no index version bump — `IndexedChunk.kind` is optional, old assistants load with `deep: false`; `bge-m3` and `bge-m3:latest` count as the same embed model.

## Embedding model picker (v2.2)

User request: *"the embedding model must be a dropdown list where I choose from the models, showing whether each is compatible with my PC."* The editor's embed field is a `<select>` over `ui/features/assistants/embed-catalog.js` (exact Ollama tags + download sizes from ollama.com/library): `bge-m3` (★ recommended, strong Arabic), `qwen3-embedding:0.6b/4b/8b` (good Arabic), `snowflake-arctic-embed2`, `granite-embedding:278m`, `embeddinggemma` (fair Arabic), `nomic-embed-text`, `mxbai-embed-large`, `all-minilm` (weak Arabic). Each option shows name · size · Arabic support · verdict for this PC · installed; the verdict is the models feature's `rateModel()` (`/api/app/system`) applied to embed size **plus the assistant's chat model size**, since both run for every question. A card below explains the pick (what it's good at, the verdict reason). "Other…" reveals a free-text tag box (unknown tags are not rated). A missing model shows the Download button (pull with progress). Changing the embed model of an assistant with documents shows a warning and, on save, asks to re-index now (declining leaves the stale banner).

## Deep-mode eval (v2.2, RTX 2070 8 GB, qwen3:8b, bge-m3, strict)

6-page Arabic employee handbook PDF + 5-page English purifier manual PDF (printed by Edge), 15 questions (facts, numbers, comparison, overview, conditions, cross-lingual, a follow-up, 3 unanswerable). C/P/W = correct/partial/wrong; no hallucinated answer to an unanswerable question in any mode.

| mode | C | P | W | refusals on the 3 unanswerable | median answer time |
|---|---|---|---|---|---|
| deep off (k 5, num_ctx 8192, no thinking) | 5 | 3 | 7 | 3/3 | 5.2 s |
| deep on, thinking off (num_ctx 16384) | 9 | 3 | 3 | 3/3 | 17 s |
| deep on, thinking on (default) | 8 | 4 | 3 | 3/3 | 25 s |

Indexing: ~1 s per PDF without digests; with digests 94 s (Arabic, 4.7k chars) and 37 s (English, 5.2k chars). The remaining wrong answers come from the Arabic PDF's allowance/end-of-service tables, whose cells `pdf-extract` merges and reorders (the document carries the `arabic_pdf_quality` warning) — a .txt/.docx copy avoids it. Retrieval takes ~3.5 s per turn because Ollama swaps `bge-m3` and the chat model on an 8 GB card.
