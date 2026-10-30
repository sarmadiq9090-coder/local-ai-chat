//! Conversation storage: one JSON file per conversation.
//!
//! Conversations are kept as raw `serde_json::Value` so fields added by the UI
//! later are preserved without backend changes. Only the fields needed for
//! listing (`id`, `title`, `model`, `updated_at`) are interpreted here.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::config::write_atomic;

#[derive(Debug, Serialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub updated_at: i64,
}

impl ConversationMeta {
    fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            id: v.get("id")?.as_str()?.to_string(),
            title: v.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
            model: v.get("model").and_then(Value::as_str).map(str::to_string),
            updated_at: v.get("updated_at").and_then(Value::as_i64).unwrap_or(0),
        })
    }
}

#[derive(Clone)]
pub struct ConversationStore {
    dir: PathBuf,
}

impl ConversationStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Ids become file names, so only allow a safe character set.
    pub fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 64
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn list(&self) -> anyhow::Result<Vec<ConversationMeta>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Some(meta) = read_json(&path).as_ref().and_then(ConversationMeta::from_value) {
                    out.push(meta);
                }
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(out)
    }

    /// `Ok(None)` if the conversation doesn't exist; `Err` if the file is unreadable or corrupt.
    pub fn get(&self, id: &str) -> anyhow::Result<Option<Value>> {
        match std::fs::read(self.file(id)) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn put(&self, id: &str, mut conv: Value) -> anyhow::Result<()> {
        if let Some(obj) = conv.as_object_mut() {
            obj.insert("id".into(), Value::String(id.to_string()));
        }
        write_atomic(&self.file(id), &serde_json::to_vec(&conv)?)
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        match std::fs::remove_file(self.file(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
