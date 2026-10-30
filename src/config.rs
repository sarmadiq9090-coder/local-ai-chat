//! User settings and on-disk locations.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

const APP_DIR: &str = "LocalAiChat";
/// Env var that overrides every data location (see `Paths::resolve`).
pub const DATA_DIR_ENV: &str = "LOCALAI_DATA_DIR";

/// Filesystem locations used by the app.
#[derive(Clone, Debug)]
pub struct Paths {
    /// `%APPDATA%\LocalAiChat`
    pub data_dir: PathBuf,
    /// `%APPDATA%\LocalAiChat\conversations`
    pub conversations_dir: PathBuf,
    /// `%APPDATA%\LocalAiChat\assistants`
    pub assistants_dir: PathBuf,
    /// `%APPDATA%\LocalAiChat\settings.json`
    pub settings_file: PathBuf,
    /// `%LOCALAPPDATA%\LocalAiChat\webview` (WebView2 cache/profile)
    pub webview_dir: PathBuf,
    /// `Documents\LocalAiChat` (exports)
    pub export_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> anyhow::Result<Self> {
        // `LOCALAI_DATA_DIR` keeps everything (data, WebView2 profile, exports) in one
        // folder: for tests, screenshots and portable use.
        if let Some(root) = std::env::var_os(DATA_DIR_ENV).filter(|v| !v.is_empty()) {
            let root = PathBuf::from(root);
            return Self::build(root.clone(), root.join("local"), root.join("exports"));
        }
        let data_dir = dirs::data_dir().context("no APPDATA dir")?.join(APP_DIR);
        let local_dir = dirs::data_local_dir()
            .context("no LOCALAPPDATA dir")?
            .join(APP_DIR);
        let docs = dirs::document_dir().unwrap_or_else(|| data_dir.clone());
        Self::build(data_dir, local_dir, docs.join(APP_DIR))
    }

    fn build(data_dir: PathBuf, local_dir: PathBuf, export_dir: PathBuf) -> anyhow::Result<Self> {
        let paths = Self {
            conversations_dir: data_dir.join("conversations"),
            assistants_dir: data_dir.join("assistants"),
            settings_file: data_dir.join("settings.json"),
            webview_dir: local_dir.join("webview"),
            export_dir,
            data_dir,
        };
        std::fs::create_dir_all(&paths.conversations_dir)?;
        std::fs::create_dir_all(&paths.assistants_dir)?;
        std::fs::create_dir_all(&paths.webview_dir)?;
        Ok(paths)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

/// User-editable settings. Missing fields fall back to defaults, so older
/// settings files keep working when new fields are added.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ollama_url: String,
    pub system_prompt: String,
    pub temperature: f32,
    pub num_ctx: u32,
    pub theme: Theme,
    pub default_model: Option<String>,
    pub think: bool,
    /// UI language: "ar" (default) or "en". Unknown values become "ar".
    pub language: String,
    /// Check GitHub for a newer release on start and every 6 h (default on).
    pub auto_update: bool,
}

/// Default system prompts per UI language. A prompt still equal to one of these
/// follows the language when it changes; a custom prompt is never touched.
pub const DEFAULT_PROMPT_AR: &str = "أنت مساعد ذكي ومفيد. أجب دائمًا بنفس لغة المستخدم؛ إذا كتب بالعربية فأجب بعربية فصحى واضحة. ضع أي كود داخل كتل ``` مع اسم اللغة.";
pub const DEFAULT_PROMPT_EN: &str = "You are a helpful, smart assistant. Always reply in the user's language; if they write in English, answer in clear, natural English. Put any code inside ``` blocks with the language name.";

impl Default for Settings {
    fn default() -> Self {
        Self {
            ollama_url: "http://127.0.0.1:11434".into(),
            system_prompt: DEFAULT_PROMPT_AR.into(),
            temperature: 0.7,
            num_ctx: 8192,
            theme: Theme::Dark,
            default_model: None,
            think: false,
            language: "ar".into(),
            auto_update: true,
        }
    }
}

impl Settings {
    /// Missing file → defaults. A file that isn't a JSON object is renamed to
    /// `settings.json.corrupt-<unix ms>` and defaults are used. In an object,
    /// only the fields that fail (wrong type, unknown enum value, invalid
    /// `ollama_url`) fall back to their defaults; the original file is copied
    /// to a `.corrupt-<unix ms>` backup first, so nothing is silently lost.
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        let obj = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Object(obj)) => obj,
            Ok(_) | Err(_) => {
                let backup = corrupt_backup_path(path);
                match std::fs::rename(path, &backup) {
                    Ok(()) => eprintln!(
                        "settings: {} is corrupt; moved to {}, using defaults",
                        path.display(),
                        backup.display()
                    ),
                    Err(re) => eprintln!(
                        "settings: {} is corrupt; backup failed ({re}), using defaults",
                        path.display()
                    ),
                }
                return Self::default();
            }
        };
        let (settings, dropped) = Self::default().merged_lenient(obj);
        if !dropped.is_empty() {
            let backup = corrupt_backup_path(path);
            let _ = std::fs::copy(path, &backup);
            eprintln!(
                "settings: invalid field(s) {dropped:?} reset to defaults; original kept as {}",
                backup.display()
            );
        }
        settings
    }

    /// `self` with every valid field of `obj` applied; returns the names of the
    /// fields that were ignored because they don't fit (wrong type, bad URL).
    fn merged_lenient(
        &self,
        obj: serde_json::Map<String, serde_json::Value>,
    ) -> (Self, Vec<String>) {
        let mut out = self.clone();
        let mut dropped = Vec::new();
        for (key, value) in obj {
            let mut one = serde_json::Map::new();
            one.insert(key.clone(), value);
            match out.merged(serde_json::Value::Object(one)) {
                Ok(next) if key != "ollama_url" || next.validate().is_ok() => out = next,
                _ => dropped.push(key),
            }
        }
        (out, dropped)
    }

    /// `self` with the top-level fields of `patch` (a JSON object) applied:
    /// fields missing from `patch` keep their current value. Unknown fields are
    /// ignored; a wrongly typed field is an error.
    pub fn merged(&self, patch: serde_json::Value) -> anyhow::Result<Self> {
        let serde_json::Value::Object(patch) = patch else {
            anyhow::bail!("settings must be a JSON object");
        };
        let mut base = serde_json::to_value(self)?;
        if let Some(obj) = base.as_object_mut() {
            obj.extend(patch);
        }
        Ok(serde_json::from_value(base)?)
    }

    /// `ollama_url` (empty = default) must be an absolute http(s) URL with a host.
    pub fn validate(&self) -> Result<(), &'static str> {
        let url = self.ollama_url.trim();
        if url.is_empty() {
            return Ok(());
        }
        match reqwest::Url::parse(url) {
            Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => Ok(()),
            _ => Err("ollama_url must be an absolute http(s) URL, e.g. http://127.0.0.1:11434"),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }

    /// Clamp values into sane ranges.
    pub fn sanitized(mut self) -> Self {
        self.temperature = self.temperature.clamp(0.0, 2.0);
        self.num_ctx = self.num_ctx.clamp(512, 131_072);
        self.ollama_url = self.ollama_url.trim().trim_end_matches('/').to_string();
        if self.ollama_url.is_empty() {
            self.ollama_url = Settings::default().ollama_url;
        }
        if self.language != "en" {
            self.language = "ar".into();
        }
        // Keep the built-in prompt in the UI language; never touch a custom one.
        let (own, other) = if self.language == "en" {
            (DEFAULT_PROMPT_EN, DEFAULT_PROMPT_AR)
        } else {
            (DEFAULT_PROMPT_AR, DEFAULT_PROMPT_EN)
        };
        if self.system_prompt.trim() == other {
            self.system_prompt = own.into();
        }
        self
    }
}

/// `<file>.corrupt-<unix ms>` next to `path`, never an existing file.
fn corrupt_backup_path(path: &Path) -> PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    (0u32..)
        .map(|n| {
            let mut name = path.as_os_str().to_owned();
            name.push(if n == 0 {
                format!(".corrupt-{ms}")
            } else {
                format!(".corrupt-{ms}-{n}")
            });
            PathBuf::from(name)
        })
        .find(|p| !p.exists())
        .expect("a free backup name")
}

/// Write via a temp file + rename so a crash never leaves a half-written file.
/// Each call uses its own temp name, so concurrent writes never collide.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}.{n}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn language_defaults_and_sanitizes() {
        let s: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert_eq!(s.language, "ar");
        let s: Settings = serde_json::from_str(r#"{"language":"fr"}"#).unwrap();
        assert_eq!(s.sanitized().language, "ar");
    }

    #[test]
    fn default_prompt_follows_language_custom_kept() {
        let s = Settings {
            language: "en".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.system_prompt, DEFAULT_PROMPT_EN);
        let s = Settings {
            language: "ar".into(),
            ..s
        }
        .sanitized();
        assert_eq!(s.system_prompt, DEFAULT_PROMPT_AR);
        let s = Settings {
            language: "en".into(),
            system_prompt: "custom".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.system_prompt, "custom");
    }

    #[test]
    fn partial_merge_keeps_other_fields() {
        let cur = Settings {
            temperature: 1.2,
            theme: Theme::Light,
            default_model: Some("m".into()),
            ..Default::default()
        };
        let s = cur.merged(serde_json::json!({ "think": true })).unwrap();
        assert!(s.think);
        assert_eq!(s.temperature, 1.2);
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.default_model.as_deref(), Some("m"));
        // Explicit null clears an optional field; a full body replaces everything.
        let s = cur
            .merged(serde_json::json!({ "default_model": null }))
            .unwrap();
        assert_eq!(s.default_model, None);
        let full = serde_json::to_value(Settings::default()).unwrap();
        let s = cur.merged(full).unwrap();
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::to_value(Settings::default()).unwrap()
        );
        assert!(
            cur.merged(serde_json::json!({ "temperature": "hot" }))
                .is_err()
        );
        assert!(cur.merged(serde_json::json!([1])).is_err());
    }

    #[test]
    fn ollama_url_validation() {
        let with = |u: &str| Settings {
            ollama_url: u.into(),
            ..Default::default()
        };
        assert!(with("http://127.0.0.1:11434").validate().is_ok());
        assert!(with("https://ollama.example.com/").validate().is_ok());
        assert!(with("").validate().is_ok());
        assert!(with("127.0.0.1:11434").validate().is_err());
        assert!(with("ftp://host").validate().is_err());
        assert!(with("http://").validate().is_err());
        assert!(with("not a url").validate().is_err());
    }

    #[test]
    fn one_bad_field_keeps_the_others() {
        let dir = std::env::temp_dir().join(format!("lac-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(
            &file,
            br#"{"language":"en","temperature":"hot","theme":"auto","ollama_url":"localhost:11434","default_model":"m"}"#,
        )
        .unwrap();
        let s = Settings::load(&file);
        assert_eq!(s.language, "en");
        assert_eq!(s.default_model.as_deref(), Some("m"));
        assert_eq!(s.temperature, Settings::default().temperature);
        assert_eq!(s.theme, Settings::default().theme);
        assert_eq!(s.ollama_url, Settings::default().ollama_url);
        assert!(file.exists(), "original stays in place");
        let backups = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .count();
        assert_eq!(backups, 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_settings_are_backed_up() {
        let dir = std::env::temp_dir().join(format!("lac-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(&file, b"{ not json").unwrap();
        let s = Settings::load(&file);
        assert_eq!(s.ollama_url, Settings::default().ollama_url);
        assert!(!file.exists());
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.corrupt-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read(backups[0].path()).unwrap(), b"{ not json");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
