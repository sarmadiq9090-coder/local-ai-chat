//! Custom assistants with a knowledge base (RAG).
//!
//! Layout under `%APPDATA%\LocalAiChat\assistants\{id}\`:
//! - `assistant.json` — settings + document list ([`Assistant`])
//! - `docs/{doc_id}.txt` — extracted plain text of each document/note
//! - `index.json` — embedded chunks ([`Index`]), loaded lazily and cached
//!
//! Pipeline: [`extract`] → [`chunk`] → [`embed`] (Ollama `/api/embed`, batches
//! of 32, unit vectors) → [`index`] (cosine top-k + keyword boost).
//!
//! Deep understanding (v2.2, per-assistant `deep`): [`digest`] writes a
//! structured digest per document with the chat model (`docs/{doc_id}.digest.md`,
//! embedded as `kind: "digest"` chunks) and [`deep`] builds each turn's
//! context (query rewriting, neighbour expansion, token budget).
//!
//! Mutations of one assistant are serialized by a per-assistant async lock;
//! searches read the cached index without taking it.

pub mod chunk;
pub mod deep;
pub mod digest;
pub mod embed;
pub mod extract;
pub mod index;
pub mod llm;
pub mod templates;
pub mod types;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use tokio::sync::{Mutex, RwLock};

use crate::config::write_atomic;
use crate::storage::ConversationStore;

pub use embed::ModelMissing;
pub use types::*;

/// The assistant (or document) doesn't exist, or its id is invalid.
/// Detect with `err.downcast_ref::<NotFound>()` to answer 404.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound(pub &'static str);

impl std::fmt::Display for NotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} not found", self.0)
    }
}

impl std::error::Error for NotFound {}

/// The knowledge index is stale (built with another embed model or an older
/// chunker): it must be re-indexed before searching. Answer 409
/// `{error, code: "reindex_required"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReindexRequired(pub String);

impl std::fmt::Display for ReindexRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReindexRequired {}

/// Result of [`AssistantStore::list`].
#[derive(Debug, Default)]
pub struct AssistantList {
    pub assistants: Vec<Assistant>,
    /// Ids whose `assistant.json` was unreadable: each was renamed to
    /// `assistant.json.corrupt-<unix ms>` and left out of the list.
    pub corrupt: Vec<String>,
}

/// Max extracted plain text per document, in bytes (5 MB).
pub const MAX_DOC_TEXT_BYTES: usize = 5 * 1024 * 1024;
/// Max indexed chunks per assistant (all documents together).
pub const MAX_CHUNKS: usize = 20_000;

/// A document or knowledge base exceeds [`MAX_DOC_TEXT_BYTES`] /
/// [`MAX_CHUNKS`]. Checked before anything is embedded; answer 400.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooLarge(pub String);

impl std::fmt::Display for TooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TooLarge {}

/// `Err(TooLarge)` if a document's extracted text is over the cap.
pub fn check_doc_text_size(name: &str, text_len: usize) -> Result<(), TooLarge> {
    if text_len > MAX_DOC_TEXT_BYTES {
        return Err(TooLarge(format!(
            "document too large: {name} has {:.1} MB of text (limit {} MB)",
            text_len as f64 / (1024.0 * 1024.0),
            MAX_DOC_TEXT_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

/// `Err(TooLarge)` if the knowledge base would hold more than [`MAX_CHUNKS`].
pub fn check_chunk_total(total: usize) -> Result<(), TooLarge> {
    if total > MAX_CHUNKS {
        return Err(TooLarge(format!(
            "knowledge base too large: {total} chunks (limit {MAX_CHUNKS}); remove some documents first"
        )));
    }
    Ok(())
}

#[derive(Clone)]
pub struct AssistantStore {
    inner: Arc<Inner>,
}

struct Inner {
    dir: PathBuf,
    http: reqwest::Client,
    /// Loaded indexes by assistant id.
    cache: RwLock<HashMap<String, Arc<Index>>>,
    /// Per-assistant write locks.
    locks: std::sync::Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Query rewrites by conversation tail (deep mode), see [`deep`].
    rewrites: std::sync::Mutex<HashMap<String, String>>,
}

impl AssistantStore {
    /// `dir` is `Paths::assistants_dir`; `http` is used for `/api/embed`
    /// (e.g. `Ollama::http().clone()`).
    pub fn new(dir: impl Into<PathBuf>, http: reqwest::Client) -> Self {
        Self {
            inner: Arc::new(Inner {
                dir: dir.into(),
                http,
                cache: RwLock::new(HashMap::new()),
                locks: std::sync::Mutex::new(HashMap::new()),
                rewrites: std::sync::Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Same safe charset rule as conversations (ids become directory names).
    pub fn valid_id(id: &str) -> bool {
        ConversationStore::valid_id(id)
    }

    // ---- paths ----------------------------------------------------------

    fn dir_of(&self, id: &str) -> PathBuf {
        self.inner.dir.join(id)
    }
    fn assistant_file(&self, id: &str) -> PathBuf {
        self.dir_of(id).join("assistant.json")
    }
    fn index_file(&self, id: &str) -> PathBuf {
        self.dir_of(id).join("index.json")
    }
    fn doc_file(&self, id: &str, doc_id: &str) -> PathBuf {
        self.dir_of(id).join("docs").join(format!("{doc_id}.txt"))
    }

    fn lock_for(&self, id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.inner.locks.lock().unwrap_or_else(|e| e.into_inner());
        locks.entry(id.to_string()).or_default().clone()
    }

    // ---- assistant CRUD -------------------------------------------------

    /// All assistants, most recently updated first. A corrupt
    /// `assistant.json` is backed up as `assistant.json.corrupt-<unix ms>`
    /// (so it stops failing) and reported in [`AssistantList::corrupt`]; the
    /// rest of the list still loads.
    pub fn list(&self) -> anyhow::Result<AssistantList> {
        let entries = match std::fs::read_dir(&self.inner.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AssistantList::default());
            }
            Err(e) => return Err(e.into()),
        };
        let mut out = AssistantList::default();
        for entry in entries {
            let entry = entry?;
            let Some(id) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if !Self::valid_id(&id) {
                continue;
            }
            match self.get(&id) {
                Ok(Some(a)) => out.assistants.push(a),
                Ok(None) => {}
                Err(e) if e.downcast_ref::<serde_json::Error>().is_some() => {
                    let file = self.assistant_file(&id);
                    let backup =
                        file.with_file_name(format!("assistant.json.corrupt-{}", now_ms()));
                    eprintln!("assistants: {e:#}; moved to {}", backup.display());
                    let _ = std::fs::rename(&file, &backup);
                    out.corrupt.push(id);
                }
                Err(e) => eprintln!("assistants: skipping {id}: {e:#}"),
            }
        }
        out.assistants
            .sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        Ok(out)
    }

    /// `Ok(None)` if it doesn't exist (or the id is invalid).
    pub fn get(&self, id: &str) -> anyhow::Result<Option<Assistant>> {
        if !Self::valid_id(id) {
            return Ok(None);
        }
        match std::fs::read(self.assistant_file(id)) {
            Ok(bytes) => {
                let mut a: Assistant = serde_json::from_slice(&bytes)
                    .with_context(|| format!("corrupt assistant.json for {id}"))?;
                a.id = id.to_string();
                Ok(Some(a))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn require(&self, id: &str) -> anyhow::Result<Assistant> {
        self.get(id)?.ok_or_else(|| NotFound("assistant").into())
    }

    fn write(&self, a: &Assistant) -> anyhow::Result<()> {
        std::fs::create_dir_all(self.dir_of(&a.id).join("docs"))?;
        write_atomic(&self.assistant_file(&a.id), &serde_json::to_vec_pretty(a)?)
    }

    pub fn create(&self, new: NewAssistant) -> anyhow::Result<Assistant> {
        let name = new.name.trim();
        if name.is_empty() {
            bail!("name is required");
        }
        let now = now_ms();
        let a = Assistant {
            id: new_id(),
            name: name.to_string(),
            icon: new
                .icon
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| Assistant::default().icon),
            description: new.description.unwrap_or_default(),
            instructions: new.instructions.unwrap_or_default(),
            model: new.model.filter(|s| !s.trim().is_empty()),
            strict: new.strict.unwrap_or(true),
            embed_model: clean_model(new.embed_model),
            deep: new.deep.unwrap_or(true),
            num_ctx: new.num_ctx.map(clamp_ctx),
            think: new.think,
            created_at: now,
            updated_at: now,
            ..Default::default()
        };
        self.write(&a)?;
        Ok(a)
    }

    /// Apply a partial update. Changing `embed_model` makes the index stale:
    /// check [`Self::index_status`] and call [`Self::reindex`].
    pub async fn update(&self, id: &str, patch: AssistantPatch) -> anyhow::Result<Assistant> {
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        let mut a = self.require(id)?;
        if let Some(name) = patch.name {
            let name = name.trim();
            if name.is_empty() {
                bail!("name is required");
            }
            a.name = name.to_string();
        }
        if let Some(v) = patch.icon {
            a.icon = v;
        }
        if let Some(v) = patch.description {
            a.description = v;
        }
        if let Some(v) = patch.instructions {
            a.instructions = v;
        }
        if let Some(v) = patch.model {
            a.model = v.filter(|s| !s.trim().is_empty());
        }
        if let Some(v) = patch.strict {
            a.strict = v;
        }
        if patch.embed_model.is_some() {
            a.embed_model = clean_model(patch.embed_model);
        }
        if let Some(v) = patch.deep {
            a.deep = v;
        }
        if let Some(v) = patch.num_ctx {
            a.num_ctx = v.map(clamp_ctx);
        }
        if let Some(v) = patch.think {
            a.think = v;
        }
        a.updated_at = now_ms();
        self.write(&a)?;
        Ok(a)
    }

    /// Delete the assistant with all its documents and index. Missing is fine.
    pub async fn delete(&self, id: &str) -> anyhow::Result<()> {
        if !Self::valid_id(id) {
            return Ok(());
        }
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        self.inner.cache.write().await.remove(id);
        match std::fs::remove_dir_all(self.dir_of(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    // ---- index ----------------------------------------------------------

    async fn load_index(&self, id: &str) -> anyhow::Result<Arc<Index>> {
        if let Some(idx) = self.inner.cache.read().await.get(id) {
            return Ok(idx.clone());
        }
        let path = self.index_file(id);
        let idx = tokio::task::spawn_blocking(move || -> anyhow::Result<Index> {
            match std::fs::read(&path) {
                Ok(bytes) => Ok(serde_json::from_slice(&bytes).context("corrupt index.json")?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Index::default()),
                Err(e) => Err(e.into()),
            }
        })
        .await??;
        let idx = Arc::new(idx);
        self.inner
            .cache
            .write()
            .await
            .insert(id.to_string(), idx.clone());
        Ok(idx)
    }

    async fn save_index(&self, id: &str, idx: Index) -> anyhow::Result<()> {
        let path = self.index_file(id);
        let idx = Arc::new(idx);
        let to_write = idx.clone();
        tokio::task::spawn_blocking(move || write_atomic(&path, &serde_json::to_vec(&*to_write)?))
            .await??;
        self.inner.cache.write().await.insert(id.to_string(), idx);
        Ok(())
    }

    /// Index summary for the editor (chunk count, model, stale flag).
    pub async fn index_status(&self, id: &str) -> anyhow::Result<IndexStatus> {
        let a = self.require(id)?;
        let idx = self.load_index(id).await?;
        Ok(IndexStatus {
            embed_model: if idx.chunks.is_empty() {
                a.embed_model.clone()
            } else {
                idx.embed_model.clone()
            },
            dim: idx.dim,
            chunks: idx.chunks.len(),
            digest_chunks: idx.chunks.iter().filter(|c| c.kind.is_some()).count(),
            stale: is_stale(&a, &idx),
        })
    }

    /// `Some(model)` if Ollama is reachable and the assistant's embedding
    /// model is definitely not installed (checked before streaming an upload
    /// or re-index, so the API can answer 424). `None` when it is installed or
    /// when that can't be determined (the stream then reports it).
    pub async fn missing_embed_model(&self, ollama_url: &str, id: &str) -> Option<String> {
        let a = self.get(id).ok()??;
        match embed::is_installed(&self.inner.http, ollama_url, &a.embed_model).await {
            Some(false) => Some(a.embed_model),
            _ => None,
        }
    }

    /// Embed `texts` in batches, reporting `embedding` progress offset by `done`.
    async fn embed_all(
        &self,
        ollama_url: &str,
        model: &str,
        texts: &[String],
        progress: &mut (impl FnMut(Progress) + Send),
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        let total = texts.len();
        let mut out = Vec::with_capacity(total);
        progress(Progress {
            status: "embedding",
            done: 0,
            total,
        });
        for batch in texts.chunks(embed::BATCH_SIZE) {
            out.extend(embed::embed(&self.inner.http, ollama_url, model, batch).await?);
            progress(Progress {
                status: "embedding",
                done: out.len(),
                total,
            });
        }
        Ok(out)
    }

    // ---- documents ------------------------------------------------------

    /// Plain text of a document (e.g. to edit a note).
    pub fn doc_text(&self, id: &str, doc_id: &str) -> anyhow::Result<Option<String>> {
        if !Self::valid_id(id) || !Self::valid_id(doc_id) {
            return Ok(None);
        }
        match std::fs::read_to_string(self.doc_file(id, doc_id)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Extract → save text → chunk → embed → append to the index → record the
    /// doc. If the index is stale (embed model changed), the whole knowledge
    /// base is re-indexed along with the new doc. On failure nothing changes.
    pub async fn add_document(
        &self,
        ollama_url: &str,
        id: &str,
        name: &str,
        input: DocInput,
        mut progress: impl FnMut(Progress) + Send,
    ) -> anyhow::Result<DocMeta> {
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        let mut a = self.require(id)?;
        let name = name.trim();
        if name.is_empty() {
            bail!("document name is required");
        }

        progress(Progress {
            status: "extracting",
            done: 0,
            total: 1,
        });
        let (kind, size, text, warning, pages) = match input {
            DocInput::File(bytes) => {
                let size = bytes.len() as u64;
                let ex = extract::extract(name, bytes).await?;
                (DocKind::File, size, ex.text, ex.warning, ex.pages)
            }
            DocInput::Note(text) => (
                DocKind::Note,
                text.len() as u64,
                extract::normalize(&text),
                None,
                None,
            ),
        };
        if text.is_empty() {
            if pages.is_some() {
                bail!(
                    "no text found in {name}: it is probably a scanned PDF (images only); upload a copy with OCR text"
                );
            }
            bail!("no text found in {name}");
        }
        check_doc_text_size(name, text.len())?;
        progress(Progress {
            status: "extracting",
            done: 1,
            total: 1,
        });

        let doc = DocMeta {
            id: new_id(),
            name: name.to_string(),
            kind,
            size,
            chunks: 0,
            added_at: now_ms(),
            warning: warning.map(str::to_string),
            pages,
            chars: Some(text.chars().count()),
            ..Default::default()
        };
        let doc_path = self.doc_file(id, &doc.id);
        std::fs::create_dir_all(doc_path.parent().expect("docs dir"))?;
        write_atomic(&doc_path, text.as_bytes())?;

        let result = self
            .index_new_doc(ollama_url, &mut a, doc, &text, &mut progress)
            .await;
        if result.is_err() {
            let _ = std::fs::remove_file(&doc_path);
        }
        result
    }

    async fn index_new_doc(
        &self,
        ollama_url: &str,
        a: &mut Assistant,
        mut doc: DocMeta,
        text: &str,
        progress: &mut (impl FnMut(Progress) + Send),
    ) -> anyhow::Result<DocMeta> {
        let idx = self.load_index(&a.id).await?;
        if is_stale(a, &idx) {
            let saved = a.clone();
            a.docs.push(doc.clone());
            if let Err(e) = self.reindex_locked(ollama_url, a, progress).await {
                *a = saved;
                return Err(e);
            }
            return Ok(a.docs.last().cloned().expect("just pushed"));
        }

        let pieces = chunk::chunk(text);
        check_chunk_total(idx.chunks.len() + pieces.len())?;
        let vecs = self
            .embed_all(ollama_url, &a.embed_model, &pieces, progress)
            .await?;
        progress(Progress {
            status: "saving",
            done: 0,
            total: 1,
        });
        let dim = vecs.first().map_or(idx.dim, Vec::len);
        if idx.dim != 0 && !idx.chunks.is_empty() && dim != idx.dim {
            bail!(
                "embedding size changed ({} → {dim}); re-index the knowledge base",
                idx.dim
            );
        }
        let mut new_idx = Index {
            version: index::INDEX_VERSION,
            embed_model: a.embed_model.clone(),
            dim,
            chunks: idx.chunks.clone(),
        };
        new_idx.chunks.extend(
            pieces
                .into_iter()
                .zip(vecs)
                .enumerate()
                .map(|(i, (text, vec))| IndexedChunk {
                    doc_id: doc.id.clone(),
                    i,
                    kind: None,
                    text,
                    vec,
                }),
        );
        doc.chunks = new_idx
            .chunks
            .iter()
            .filter(|c| c.doc_id == doc.id && c.kind.is_none())
            .count();
        self.save_index(&a.id, new_idx).await?;

        a.docs.push(doc.clone());
        a.updated_at = now_ms();
        self.write(a)?;
        progress(Progress {
            status: "saving",
            done: 1,
            total: 1,
        });
        Ok(doc)
    }

    /// Remove a document and its chunks. `Err(NotFound)` if it doesn't exist.
    pub async fn remove_document(&self, id: &str, doc_id: &str) -> anyhow::Result<()> {
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        let mut a = self.require(id)?;
        let Some(pos) = a.docs.iter().position(|d| d.id == doc_id) else {
            return Err(NotFound("document").into());
        };
        a.docs.remove(pos);
        let idx = self.load_index(id).await?;
        if idx.chunks.iter().any(|c| c.doc_id == doc_id) {
            let chunks = idx
                .chunks
                .iter()
                .filter(|c| c.doc_id != doc_id)
                .cloned()
                .collect();
            self.save_index(
                id,
                Index {
                    version: idx.version,
                    embed_model: idx.embed_model.clone(),
                    dim: idx.dim,
                    chunks,
                },
            )
            .await?;
        }
        a.updated_at = now_ms();
        self.write(&a)?;
        if Self::valid_id(doc_id) {
            let _ = std::fs::remove_file(self.doc_file(id, doc_id));
        }
        self.remove_digest_file(id, doc_id);
        Ok(())
    }

    /// Rebuild the whole index from `docs/*.txt` with the assistant's current
    /// embed model. Returns the updated assistant (fresh chunk counts).
    pub async fn reindex(
        &self,
        ollama_url: &str,
        id: &str,
        mut progress: impl FnMut(Progress) + Send,
    ) -> anyhow::Result<Assistant> {
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        let mut a = self.require(id)?;
        self.reindex_locked(ollama_url, &mut a, &mut progress)
            .await?;
        Ok(a)
    }

    async fn reindex_locked(
        &self,
        ollama_url: &str,
        a: &mut Assistant,
        progress: &mut (impl FnMut(Progress) + Send),
    ) -> anyhow::Result<()> {
        let mut owners = Vec::new(); // (doc_id, i, is_digest)
        let mut texts = Vec::new();
        for doc in &a.docs {
            let text = match std::fs::read_to_string(self.doc_file(&a.id, &doc.id)) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for (i, piece) in chunk::chunk(&text).into_iter().enumerate() {
                owners.push((doc.id.clone(), i, false));
                texts.push(piece);
            }
            // Deep understanding: re-embed an existing digest too.
            if let Some(dg) = self.digest_text(&a.id, &doc.id)? {
                for (i, piece) in digest::digest_chunks(&doc.name, &dg)
                    .into_iter()
                    .enumerate()
                {
                    owners.push((doc.id.clone(), i, true));
                    texts.push(piece);
                }
            }
        }
        check_chunk_total(texts.len())?;
        let vecs = self
            .embed_all(ollama_url, &a.embed_model, &texts, progress)
            .await?;
        progress(Progress {
            status: "saving",
            done: 0,
            total: 1,
        });
        let dim = vecs.first().map_or(0, Vec::len);
        let chunks: Vec<IndexedChunk> = owners
            .into_iter()
            .zip(texts)
            .zip(vecs)
            .map(|(((doc_id, i, dg), text), vec)| IndexedChunk {
                doc_id,
                i,
                kind: dg.then(|| digest::DIGEST_KIND.to_string()),
                text,
                vec,
            })
            .collect();
        for doc in &mut a.docs {
            doc.chunks = chunks
                .iter()
                .filter(|c| c.doc_id == doc.id && c.kind.is_none())
                .count();
            let dg = chunks
                .iter()
                .filter(|c| c.doc_id == doc.id && c.kind.is_some())
                .count();
            if dg > 0 {
                let meta = doc.digest.get_or_insert_with(|| DigestMeta {
                    status: "ok".into(),
                    ..Default::default()
                });
                if meta.status == "failed" {
                    meta.status = "ok".into();
                    meta.error = None;
                }
                meta.chunks = dg;
            } else if doc.digest.as_ref().is_some_and(|g| g.status != "failed") {
                doc.digest = None; // digest file gone
            }
        }
        self.save_index(
            &a.id,
            Index {
                version: index::INDEX_VERSION,
                embed_model: a.embed_model.clone(),
                dim,
                chunks,
            },
        )
        .await?;
        a.updated_at = now_ms();
        self.write(a)?;
        progress(Progress {
            status: "saving",
            done: 1,
            total: 1,
        });
        Ok(())
    }

    // ---- retrieval ------------------------------------------------------

    /// Most relevant passages for `query` (default k = 5, min score
    /// [`index::MIN_SCORE`], tuned on bge-m3, and within
    /// [`index::RELATIVE_WINDOW`] of the best). Empty when nothing passes or
    /// the knowledge base is empty. `Err(ReindexRequired)` on a stale index.
    pub async fn search(
        &self,
        ollama_url: &str,
        id: &str,
        query: &str,
        k: Option<usize>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let a = self.require(id)?;
        let query = query.trim();
        let idx = self.load_index(id).await?;
        if query.is_empty() || idx.chunks.is_empty() {
            return Ok(Vec::new());
        }
        if !same_model(&idx.embed_model, &a.embed_model) {
            return Err(ReindexRequired(format!(
                "the knowledge index was built with '{}' but the assistant uses '{}'; re-index required",
                idx.embed_model, a.embed_model
            ))
            .into());
        }
        if is_stale(&a, &idx) {
            return Err(ReindexRequired(
                "the knowledge index is out of date (built by an older version); re-index required"
                    .into(),
            )
            .into());
        }
        let k = k.unwrap_or(index::DEFAULT_K).clamp(1, 50);
        // Digest chunks (deep mode, see `context`) are left out here: score
        // only document chunks so `/search` behaves as before.
        let text_only;
        let idx: &Index = if idx.chunks.iter().any(|c| c.kind.is_some()) {
            text_only = Index {
                version: idx.version,
                embed_model: idx.embed_model.clone(),
                dim: idx.dim,
                chunks: idx
                    .chunks
                    .iter()
                    .filter(|c| c.kind.is_none())
                    .cloned()
                    .collect(),
            };
            &text_only
        } else {
            &idx
        };
        let qvec = embed::embed(
            &self.inner.http,
            ollama_url,
            &idx.embed_model,
            &[query.to_string()],
        )
        .await?
        .pop()
        .unwrap_or_default();
        let names: HashMap<&str, &str> = a
            .docs
            .iter()
            .map(|d| (d.id.as_str(), d.name.as_str()))
            .collect();
        Ok(
            index::top_k(idx, &qvec, query, usize::MAX, index::MIN_SCORE)
                .into_iter()
                .filter_map(|(i, score)| {
                    let c = &idx.chunks[i];
                    let doc_name = names.get(c.doc_id.as_str())?; // skip orphaned chunks
                    Some(SearchResult {
                        doc_id: c.doc_id.clone(),
                        doc_name: doc_name.to_string(),
                        text: c.text.clone(),
                        score,
                    })
                })
                .take(k)
                .collect(),
        )
    }
}

/// The index doesn't match the assistant: built with another embed model or
/// an older [`index::INDEX_VERSION`], or some doc's chunk count disagrees with
/// what the index holds.
fn is_stale(a: &Assistant, idx: &Index) -> bool {
    if !idx.chunks.is_empty()
        && (!same_model(&idx.embed_model, &a.embed_model) || idx.version != index::INDEX_VERSION)
    {
        return true;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut digests: HashMap<&str, usize> = HashMap::new();
    for c in &idx.chunks {
        let m = if c.kind.is_some() {
            &mut digests
        } else {
            &mut counts
        };
        *m.entry(c.doc_id.as_str()).or_default() += 1;
    }
    a.docs.iter().any(|d| {
        let digest_expected = match &d.digest {
            Some(g) if g.status != "failed" => g.chunks,
            _ => 0,
        };
        counts.get(d.id.as_str()).copied().unwrap_or(0) != d.chunks
            || digests.get(d.id.as_str()).copied().unwrap_or(0) != digest_expected
    })
}

/// Same Ollama model name: `"bge-m3"` and `"bge-m3:latest"` are one model.
pub(crate) fn same_model(a: &str, b: &str) -> bool {
    let key = |m: &str| {
        m.trim()
            .strip_suffix(":latest")
            .unwrap_or(m.trim())
            .to_string()
    };
    key(a) == key(b)
}

/// Keep an assistant's `num_ctx` within [`deep::MIN_NUM_CTX`]..=[`deep::MAX_NUM_CTX`].
fn clamp_ctx(n: u32) -> u32 {
    n.clamp(deep::MIN_NUM_CTX, deep::MAX_NUM_CTX)
}

fn clean_model(m: Option<String>) -> String {
    m.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_EMBED_MODEL.into())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
