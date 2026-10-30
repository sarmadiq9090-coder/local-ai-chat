//! Conversation storage: one JSON file per conversation.
//!
//! Conversations are kept as raw `serde_json::Value` so fields added by the UI
//! later are preserved without backend changes. Only the fields needed for
//! listing (`id`, `title`, `model`, `updated_at`) are interpreted here.
//!
//! Listing parses only those four fields, and caches the result per file keyed
//! by (modified time, length), so repeated listings re-read only changed files.
//! A file that can't be parsed is listed as `corrupt` (id = file name) instead of
//! disappearing, so the user can see and delete it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::write_atomic;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub updated_at: i64,
    /// The file exists but isn't a readable conversation (only set when true).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub corrupt: bool,
}

/// The listing fields only; serde skips everything else (e.g. `messages`)
/// without building values for it.
#[derive(Deserialize)]
struct MetaFields {
    id: Option<Value>,
    title: Option<Value>,
    model: Option<Value>,
    updated_at: Option<Value>,
}

impl ConversationMeta {
    /// `None` if the bytes aren't a JSON object with a string `id`.
    fn parse(bytes: &[u8]) -> Option<Self> {
        // A struct would also deserialize from a JSON array; require an object.
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
            return None;
        }
        let f: MetaFields = serde_json::from_slice(bytes).ok()?;
        Some(Self {
            id: f.id?.as_str()?.to_string(),
            title: f
                .title
                .as_ref()
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            model: f.model.as_ref().and_then(Value::as_str).map(str::to_string),
            updated_at: f.updated_at.as_ref().and_then(Value::as_i64).unwrap_or(0),
            corrupt: false,
        })
    }

    fn corrupt(id: String, modified: Option<SystemTime>) -> Self {
        let updated_at = modified
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            id,
            title: String::new(),
            model: None,
            updated_at,
            corrupt: true,
        }
    }
}

/// A conversation file exists but can't be parsed (see [`ConversationStore::get`]).
#[derive(Debug)]
pub struct CorruptConversation(pub String);

impl std::fmt::Display for CorruptConversation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "corrupt conversation file: {}", self.0)
    }
}

impl std::error::Error for CorruptConversation {}

/// Cached listing entry: valid while the file's (mtime, len) are unchanged.
struct CacheEntry {
    modified: Option<SystemTime>,
    len: u64,
    meta: Option<ConversationMeta>,
}

#[derive(Clone)]
pub struct ConversationStore {
    dir: PathBuf,
    cache: Arc<Mutex<HashMap<PathBuf, CacheEntry>>>,
}

impl ConversationStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            cache: Arc::default(),
        }
    }

    /// Ids become file names, so only allow a safe character set.
    pub fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn list(&self) -> anyhow::Result<Vec<ConversationMeta>> {
        let mut out = Vec::new();
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut seen = HashSet::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.extension().is_some_and(|e| e == "json") {
                continue;
            }
            let Ok(md) = entry.metadata() else { continue };
            if !md.is_file() {
                continue;
            }
            let (modified, len) = (md.modified().ok(), md.len());
            let fresh = cache
                .get(&path)
                .is_some_and(|c| c.modified == modified && c.len == len && modified.is_some());
            if !fresh {
                let meta = read_meta(&path, modified);
                cache.insert(
                    path.clone(),
                    CacheEntry {
                        modified,
                        len,
                        meta,
                    },
                );
            }
            if let Some(meta) = cache.get(&path).and_then(|c| c.meta.clone()) {
                out.push(meta);
            }
            seen.insert(path);
        }
        // Forget files that are gone.
        cache.retain(|p, _| seen.contains(p));
        out.sort_by_key(|m| std::cmp::Reverse(m.updated_at));
        Ok(out)
    }

    /// `Ok(None)` if the conversation doesn't exist; `Err` if the file is
    /// unreadable, or [`CorruptConversation`] if it isn't valid JSON.
    pub fn get(&self, id: &str) -> anyhow::Result<Option<Value>> {
        match std::fs::read(self.file(id)) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(v) => Ok(Some(v)),
                Err(e) => Err(CorruptConversation(e.to_string()).into()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn put(&self, id: &str, mut conv: Value) -> anyhow::Result<()> {
        if let Some(obj) = conv.as_object_mut() {
            obj.insert("id".into(), Value::String(id.to_string()));
        }
        let file = self.file(id);
        self.forget(&file);
        write_atomic(&file, &serde_json::to_vec(&conv)?)
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        let file = self.file(id);
        self.forget(&file);
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// Drop a cached entry (belt and braces: (mtime, len) would catch it too).
    fn forget(&self, file: &Path) {
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(file);
    }
}

/// Listing entry for one file: parsed meta, a `corrupt` entry, or `None` for an
/// unreadable file or one whose name isn't a valid id.
fn read_meta(path: &Path, modified: Option<SystemTime>) -> Option<ConversationMeta> {
    let bytes = std::fs::read(path).ok()?;
    ConversationMeta::parse(&bytes).or_else(|| {
        let stem = path.file_stem()?.to_str()?;
        ConversationStore::valid_id(stem)
            .then(|| ConversationMeta::corrupt(stem.to_string(), modified))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_store() -> (ConversationStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("lac-conv-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        (ConversationStore::new(&dir), dir)
    }

    #[test]
    fn list_includes_corrupt_files_and_tracks_changes() {
        let (store, dir) = temp_store();
        store
            .put(
                "a",
                json!({"title":"A","model":"m","updated_at":5,"messages":[{"role":"user","content":"hi"}]}),
            )
            .unwrap();
        store.put("b", json!({"title":"B","updated_at":9})).unwrap();
        std::fs::write(dir.join("bad.json"), b"{ broken").unwrap();
        std::fs::write(dir.join("not valid id!.json"), b"{ broken").unwrap();
        std::fs::write(dir.join("arr.json"), b"[\"x\"]").unwrap();

        let list = store.list().unwrap();
        let ids: Vec<_> = list.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"a") && ids.contains(&"b"));
        assert_eq!(list.len(), 4, "{list:?}"); // a, b, bad, arr
        let a = list.iter().find(|m| m.id == "a").unwrap();
        assert_eq!(a.title, "A");
        assert_eq!(a.model.as_deref(), Some("m"));
        assert!(!a.corrupt);
        let bad = list.iter().find(|m| m.id == "bad").unwrap();
        assert!(bad.corrupt && bad.title.is_empty() && bad.model.is_none());
        assert!(bad.updated_at > 1_600_000_000_000, "mtime in ms");
        assert!(list.iter().find(|m| m.id == "arr").unwrap().corrupt);
        // Serialization: `corrupt` only on corrupt entries.
        assert!(serde_json::to_value(a).unwrap().get("corrupt").is_none());
        assert_eq!(serde_json::to_value(bad).unwrap()["corrupt"], true);

        // Second listing comes from the cache and is identical.
        assert_eq!(store.list().unwrap(), list);

        // Changes are picked up: edit, delete, external rewrite.
        store
            .put("a", json!({"title":"A2","updated_at":20}))
            .unwrap();
        store.delete("b").unwrap();
        std::fs::write(
            dir.join("bad.json"),
            br#"{"id":"bad","title":"Fixed","updated_at":1}"#,
        )
        .unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.iter().find(|m| m.id == "a").unwrap().title, "A2");
        assert!(list.iter().all(|m| m.id != "b"));
        let fixed = list.iter().find(|m| m.id == "bad").unwrap();
        assert!(!fixed.corrupt);
        assert_eq!(fixed.title, "Fixed");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn get_reports_corrupt_files() {
        let (store, dir) = temp_store();
        std::fs::write(dir.join("bad.json"), b"{ broken").unwrap();
        let err = store.get("bad").unwrap_err();
        assert!(err.downcast_ref::<CorruptConversation>().is_some());
        assert!(store.get("missing").unwrap().is_none());
        store.delete("bad").unwrap();
        assert!(!dir.join("bad.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
