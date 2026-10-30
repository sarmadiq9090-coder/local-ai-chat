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
    ///
    /// Only for a local URL (127.0.0.1 / localhost / ::1): a remote Ollama can't be
    /// started from here, so that just reports whether it answers. The wait after
    /// spawning is capped at [`START_WAIT`].
    pub async fn start(&self, base_url: &str) -> bool {
        if self.version(base_url).await.is_some() {
            return true;
        }
        if !is_local_url(base_url) {
            return false;
        }
        let Some(exe) = find_ollama_exe() else {
            return false;
        };
        if spawn_detached(&exe, &["serve"]).is_err() {
            return false;
        }
        let deadline = tokio::time::Instant::now() + START_WAIT;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            if let Ok(Some(_)) = tokio::time::timeout(left, self.version(base_url)).await {
                return true;
            }
        }
    }
}

/// Longest wait for a freshly spawned `ollama serve` to answer.
const START_WAIT: Duration = Duration::from_secs(20);

/// The URL points at this machine (so `ollama serve` here would serve it).
fn is_local_url(url: &str) -> bool {
    let Some(host) = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
    else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn find_ollama_exe() -> Option<PathBuf> {
    let candidates = [
        dirs::data_local_dir().map(|d| d.join(r"Programs\Ollama\ollama.exe")),
        std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join(r"Ollama\ollama.exe")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .or_else(|| {
            // Fall back to PATH lookup.
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|d| d.join("ollama.exe"))
                    .find(|p| p.exists())
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

#[cfg(test)]
mod tests {
    use super::is_local_url;

    #[test]
    fn local_urls_only() {
        assert!(is_local_url("http://127.0.0.1:11434"));
        assert!(is_local_url("http://localhost:11434"));
        assert!(is_local_url("http://LOCALHOST"));
        assert!(is_local_url("http://[::1]:11434"));
        assert!(!is_local_url("http://192.168.1.5:11434"));
        assert!(!is_local_url("https://ollama.example.com"));
        assert!(!is_local_url("http://localhost.evil.com"));
        assert!(!is_local_url("not a url"));
    }
}
