//! User settings and on-disk locations.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

const APP_DIR: &str = "LocalAiChat";

/// Filesystem locations used by the app.
#[derive(Clone, Debug)]
pub struct Paths {
    /// `%APPDATA%\LocalAiChat`
    pub data_dir: PathBuf,
    /// `%APPDATA%\LocalAiChat\conversations`
    pub conversations_dir: PathBuf,
    /// `%APPDATA%\LocalAiChat\settings.json`
    pub settings_file: PathBuf,
    /// `%LOCALAPPDATA%\LocalAiChat\webview` (WebView2 cache/profile)
    pub webview_dir: PathBuf,
    /// `Documents\LocalAiChat` (exports)
    pub export_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> anyhow::Result<Self> {
        let data_dir = dirs::data_dir().context("no APPDATA dir")?.join(APP_DIR);
        let local_dir = dirs::data_local_dir().context("no LOCALAPPDATA dir")?.join(APP_DIR);
        let docs = dirs::document_dir().unwrap_or_else(|| data_dir.clone());
        let paths = Self {
            conversations_dir: data_dir.join("conversations"),
            settings_file: data_dir.join("settings.json"),
            webview_dir: local_dir.join("webview"),
            export_dir: docs.join(APP_DIR),
            data_dir,
        };
        std::fs::create_dir_all(&paths.conversations_dir)?;
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ollama_url: "http://127.0.0.1:11434".into(),
            system_prompt: "أنت مساعد ذكي ومفيد. أجب دائمًا بنفس لغة المستخدم؛ إذا كتب بالعربية فأجب بعربية فصحى واضحة. ضع أي كود داخل كتل ``` مع اسم اللغة.".into(),
            temperature: 0.7,
            num_ctx: 8192,
            theme: Theme::Dark,
            default_model: None,
            think: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
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
        self
    }
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
