//! Ollama helpers: health check and launching the Ollama server.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

#[derive(Clone)]
pub struct Ollama {
    http: reqwest::Client,
}

impl Ollama {
    pub fn new() -> Self {
        // No overall timeout: chat and pull responses stream for a long time.
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("http client");
        Self { http }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Returns the Ollama version, or `None` if it is not reachable.
    pub async fn version(&self, base_url: &str) -> Option<String> {
        #[derive(Deserialize)]
        struct V {
            version: String,
        }
        let resp = self
            .http
            .get(format!("{base_url}/api/version"))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .ok()?;
        resp.json::<V>().await.ok().map(|v| v.version)
    }

    /// Start a background `ollama serve` (if installed) and wait until it answers.
    pub async fn start(&self, base_url: &str) -> bool {
        if self.version(base_url).await.is_some() {
            return true;
        }
        let Some(exe) = find_ollama_exe() else { return false };
        if spawn_detached(&exe, &["serve"]).is_err() {
            return false;
        }
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if self.version(base_url).await.is_some() {
                return true;
            }
        }
        false
    }
}

fn find_ollama_exe() -> Option<PathBuf> {
    let candidates = [
        dirs::data_local_dir().map(|d| d.join(r"Programs\Ollama\ollama.exe")),
        std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join(r"Ollama\ollama.exe")),
    ];
    candidates.into_iter().flatten().find(|p| p.exists()).or_else(|| {
        // Fall back to PATH lookup.
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths).map(|d| d.join("ollama.exe")).find(|p| p.exists())
        })
    })
}

/// Spawn a process with no console window that outlives this app.
fn spawn_detached(exe: &PathBuf, args: &[&str]) -> std::io::Result<()> {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    cmd.spawn().map(|_| ())
}
