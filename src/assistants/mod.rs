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
//! Mutations of one assistant are serialized by a per-assistant async lock;
//! searches read the cached index without taking it.

// Until routes are wired.
#![allow(dead_code)]

pub mod chunk;
pub mod embed;
pub mod extract;
pub mod index;
pub mod types;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use tokio::sync::{Mutex, RwLock};

use crate::config::write_atomic;
use crate::storage::ConversationStore;

#[allow(unused_imports)] // re-exported for the routes layer
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

    /// All assistants, most recently updated first.
    pub fn list(&self) -> anyhow::Result<Vec<Assistant>> {
        let entries = match std::fs::read_dir(&self.inner.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry?;
            let Some(id) = entry.file_name().to_str().map(str::to_string) else { continue };
            if !Self::valid_id(&id) {
                continue;
            }
            if let Ok(Some(a)) = self.get(&id) {
                out.push(a);
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
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
            icon: new.icon.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| Assistant::default().icon),
            description: new.description.unwrap_or_default(),
            instructions: new.instructions.unwrap_or_default(),
            model: new.model.filter(|s| !s.trim().is_empty()),
            strict: new.strict.unwrap_or(true),
            embed_model: clean_model(new.embed_model),
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
        self.inner.cache.write().await.insert(id.to_string(), idx.clone());
        Ok(idx)
    }

    async fn save_index(&self, id: &str, idx: Index) -> anyhow::Result<()> {
        let path = self.index_file(id);
        let idx = Arc::new(idx);
        let to_write = idx.clone();
        tokio::task::spawn_blocking(move || write_atomic(&path, &serde_json::to_vec(&*to_write)?)).await??;
        self.inner.cache.write().await.insert(id.to_string(), idx);
        Ok(())
    }

    /// Index summary for the editor (chunk count, model, stale flag).
    pub async fn index_status(&self, id: &str) -> anyhow::Result<IndexStatus> {
        let a = self.require(id)?;
        let idx = self.load_index(id).await?;
        Ok(IndexStatus {
            embed_model: if idx.chunks.is_empty() { a.embed_model.clone() } else { idx.embed_model.clone() },
            dim: idx.dim,
            chunks: idx.chunks.len(),
            stale: is_stale(&a, &idx),
        })
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
        progress(Progress { status: "embedding", done: 0, total });
        for batch in texts.chunks(embed::BATCH_SIZE) {
            out.extend(embed::embed(&self.inner.http, ollama_url, model, batch).await?);
            progress(Progress { status: "embedding", done: out.len(), total });
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

        progress(Progress { status: "extracting", done: 0, total: 1 });
        let (kind, size, text) = match input {
            DocInput::File(bytes) => {
                let size = bytes.len() as u64;
                (DocKind::File, size, extract::extract(name, bytes).await?)
            }
            DocInput::Note(text) => (DocKind::Note, text.len() as u64, extract::normalize(&text)),
        };
        if text.is_empty() {
            bail!("no text found in {name}");
        }
        progress(Progress { status: "extracting", done: 1, total: 1 });

        let doc = DocMeta { id: new_id(), name: name.to_string(), kind, size, chunks: 0, added_at: now_ms(), ..Default::default() };
        let doc_path = self.doc_file(id, &doc.id);
        std::fs::create_dir_all(doc_path.parent().expect("docs dir"))?;
        write_atomic(&doc_path, text.as_bytes())?;

        let result = self.index_new_doc(ollama_url, &mut a, doc, &text, &mut progress).await;
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
        let vecs = self.embed_all(ollama_url, &a.embed_model, &pieces, progress).await?;
        progress(Progress { status: "saving", done: 0, total: 1 });
        let dim = vecs.first().map_or(idx.dim, Vec::len);
        if idx.dim != 0 && !idx.chunks.is_empty() && dim != idx.dim {
            bail!("embedding size changed ({} → {dim}); re-index the knowledge base", idx.dim);
        }
        let mut new_idx = Index { embed_model: a.embed_model.clone(), dim, chunks: idx.chunks.clone() };
        new_idx.chunks.extend(pieces.into_iter().zip(vecs).enumerate().map(|(i, (text, vec))| {
            IndexedChunk { doc_id: doc.id.clone(), i, text, vec }
        }));
        doc.chunks = new_idx.chunks.iter().filter(|c| c.doc_id == doc.id).count();
        self.save_index(&a.id, new_idx).await?;

        a.docs.push(doc.clone());
        a.updated_at = now_ms();
        self.write(a)?;
        progress(Progress { status: "saving", done: 1, total: 1 });
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
            let chunks = idx.chunks.iter().filter(|c| c.doc_id != doc_id).cloned().collect();
            self.save_index(id, Index { embed_model: idx.embed_model.clone(), dim: idx.dim, chunks }).await?;
        }
        a.updated_at = now_ms();
        self.write(&a)?;
        if Self::valid_id(doc_id) {
            let _ = std::fs::remove_file(self.doc_file(id, doc_id));
        }
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
        self.reindex_locked(ollama_url, &mut a, &mut progress).await?;
        Ok(a)
    }

    async fn reindex_locked(
        &self,
        ollama_url: &str,
        a: &mut Assistant,
        progress: &mut (impl FnMut(Progress) + Send),
    ) -> anyhow::Result<()> {
        let mut owners = Vec::new(); // (doc_id, i)
        let mut texts = Vec::new();
        for doc in &a.docs {
            let text = match std::fs::read_to_string(self.doc_file(&a.id, &doc.id)) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for (i, piece) in chunk::chunk(&text).into_iter().enumerate() {
                owners.push((doc.id.clone(), i));
                texts.push(piece);
            }
        }
        let vecs = self.embed_all(ollama_url, &a.embed_model, &texts, progress).await?;
        progress(Progress { status: "saving", done: 0, total: 1 });
        let dim = vecs.first().map_or(0, Vec::len);
        let chunks: Vec<IndexedChunk> = owners
            .into_iter()
            .zip(texts)
            .zip(vecs)
            .map(|(((doc_id, i), text), vec)| IndexedChunk { doc_id, i, text, vec })
            .collect();
        for doc in &mut a.docs {
            doc.chunks = chunks.iter().filter(|c| c.doc_id == doc.id).count();
        }
        self.save_index(&a.id, Index { embed_model: a.embed_model.clone(), dim, chunks }).await?;
        a.updated_at = now_ms();
        self.write(a)?;
        progress(Progress { status: "saving", done: 1, total: 1 });
        Ok(())
    }

    // ---- retrieval ------------------------------------------------------

    /// Most relevant passages for `query` (default k = 5, min score 0.35).
    /// Empty when nothing passes the threshold or the knowledge base is empty.
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
        if idx.embed_model != a.embed_model {
            bail!(
                "the knowledge index was built with '{}' but the assistant uses '{}'; re-index required",
                idx.embed_model,
                a.embed_model
            );
        }
        let k = k.unwrap_or(index::DEFAULT_K).clamp(1, 50);
        let qvec = embed::embed(&self.inner.http, ollama_url, &idx.embed_model, &[query.to_string()])
            .await?
            .pop()
            .unwrap_or_default();
        let names: HashMap<&str, &str> = a.docs.iter().map(|d| (d.id.as_str(), d.name.as_str())).collect();
        Ok(index::top_k(&idx, &qvec, query, usize::MAX, index::MIN_SCORE)
            .into_iter()
            .filter_map(|(i, score)| {
                let c = &idx.chunks[i];
                let doc_name = names.get(c.doc_id.as_str())?; // skip orphaned chunks
                Some(SearchResult { doc_id: c.doc_id.clone(), doc_name: doc_name.to_string(), text: c.text.clone(), score })
            })
            .take(k)
            .collect())
    }

    /// Base directory (for diagnostics).
    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }
}

/// The index doesn't match the assistant: built with another embed model, or
/// some doc's chunk count disagrees with what the index holds.
fn is_stale(a: &Assistant, idx: &Index) -> bool {
    if !idx.chunks.is_empty() && idx.embed_model != a.embed_model {
        return true;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for c in &idx.chunks {
        *counts.entry(c.doc_id.as_str()).or_default() += 1;
    }
    a.docs.iter().any(|d| counts.get(d.id.as_str()).copied().unwrap_or(0) != d.chunks)
}

fn clean_model(m: Option<String>) -> String {
    m.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| DEFAULT_EMBED_MODEL.into())
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
