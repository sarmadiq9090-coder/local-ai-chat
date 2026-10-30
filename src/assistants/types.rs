//! Data types for custom assistants: the persisted `assistant.json` and
//! `index.json` shapes, request bodies for create/update, progress events and
//! search results.
//!
//! Persisted structs use `#[serde(default)]` (older files keep loading when
//! fields are added) and a flattened `extra` map (fields written by newer
//! versions or by the UI survive a load/save round trip).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Default embedding model: multilingual with strong Arabic support.
pub const DEFAULT_EMBED_MODEL: &str = "bge-m3";

/// `assistant.json`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Assistant {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub instructions: String,
    /// Preferred chat model (`None` = use the app's current model).
    pub model: Option<String>,
    /// Answer only from the knowledge base.
    pub strict: bool,
    pub embed_model: String,
    /// Unix epoch milliseconds (same convention as conversations).
    pub created_at: i64,
    pub updated_at: i64,
    pub docs: Vec<DocMeta>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Assistant {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            icon: "🤖".into(),
            description: String::new(),
            instructions: String::new(),
            model: None,
            strict: true,
            embed_model: DEFAULT_EMBED_MODEL.into(),
            created_at: 0,
            updated_at: 0,
            docs: Vec::new(),
            extra: Map::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    #[default]
    File,
    Note,
}

/// One knowledge document (an uploaded file or a note written in the app).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DocMeta {
    pub id: String,
    pub name: String,
    pub kind: DocKind,
    /// Size of the original upload (files) or the note text, in bytes.
    pub size: u64,
    pub chunks: usize,
    pub added_at: i64,
    /// Extraction quality warning, e.g. `"arabic_pdf_quality"` (an Arabic PDF
    /// whose text had to be repaired and may be inaccurate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Body of `POST /api/app/assistants`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NewAssistant {
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub model: Option<String>,
    pub strict: Option<bool>,
    pub embed_model: Option<String>,
}

/// Body of `PUT /api/app/assistants/{id}`: only present fields change.
/// `model: null` clears the preferred model; an absent `model` keeps it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AssistantPatch {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
    #[serde(deserialize_with = "double_option")]
    pub model: Option<Option<String>>,
    pub strict: Option<bool>,
    pub embed_model: Option<String>,
}

fn double_option<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

/// What to add to the knowledge base.
#[derive(Clone, Debug)]
pub enum DocInput {
    /// An uploaded file; the type is decided by the file name's extension.
    File(Vec<u8>),
    /// A note typed in the app.
    Note(String),
}

/// Indexing progress, streamed to the UI as NDJSON `{status, done, total}`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Progress {
    /// `"extracting"`, `"embedding"`, `"saving"`.
    pub status: &'static str,
    pub done: usize,
    pub total: usize,
}

/// `index.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Index {
    /// Chunking/format version ([`crate::assistants::index::INDEX_VERSION`]);
    /// an index from an older version is stale and must be rebuilt.
    /// Missing in indexes written before v2.1.1 (= 0).
    pub version: u32,
    pub embed_model: String,
    pub dim: usize,
    pub chunks: Vec<IndexedChunk>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexedChunk {
    pub doc_id: String,
    /// Position of the chunk within its document.
    pub i: usize,
    pub text: String,
    /// Unit-length embedding (cosine similarity is a dot product).
    pub vec: Vec<f32>,
}

/// One retrieved passage.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SearchResult {
    pub doc_id: String,
    pub doc_name: String,
    pub text: String,
    pub score: f32,
}

/// Summary of an assistant's index, for the editor UI.
#[derive(Clone, Debug, Serialize)]
pub struct IndexStatus {
    pub embed_model: String,
    pub dim: usize,
    pub chunks: usize,
    /// The index was built with a different model than `assistant.embed_model`,
    /// by an older chunker version, or is missing chunks for some docs: a
    /// re-index is required.
    pub stale: bool,
}
