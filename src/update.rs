//! Auto-update from GitHub Releases (spec: `docs/specs/auto-update.md`).
//!
//! - `fetch_latest()` asks GitHub for the latest release of [`REPO`] (`Ok(None)`
//!   when there is no release / no repo); `is_newer()` compares it to this build.
//! - `install()` downloads the `local-ai-chat.exe` asset, verifies it against the
//!   `local-ai-chat.exe.sha256` asset, swaps it in place of the running exe
//!   (running exe → `local-ai-chat.old.exe`), and spawns it with `--after-update`.
//! - `after_update()` (called from `main` on `--after-update`) deletes the old exe
//!   and remembers the new version so the UI can show a "updated" toast once.
//! - `request_exit()` asks the app to quit (the window installs a handler; in
//!   `--serve` mode the process just exits).
//!
//! Dev override: if `LOCALAI_UPDATE_FEED=<url>` is set, that URL is used instead of
//! the GitHub API (it must return JSON shaped like a GitHub release).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, bail};
use futures_util::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// GitHub `owner/repo` whose releases are checked.
pub const REPO: &str = "sarmadiq9090-coder/local-ai-chat";
/// Release asset with the new executable.
pub const ASSET_NAME: &str = "local-ai-chat.exe";
/// Release asset with `<sha256 hex>  local-ai-chat.exe`.
pub const SHA_ASSET_NAME: &str = "local-ai-chat.exe.sha256";
/// The previous exe is renamed to this (next to the running exe) during the swap.
pub const OLD_EXE_NAME: &str = "local-ai-chat.old.exe";
/// The new exe is written here first, then renamed into place.
const STAGED_EXE_NAME: &str = "local-ai-chat.new.exe";
/// Command-line flag passed to the freshly installed exe.
pub const AFTER_UPDATE_ARG: &str = "--after-update";
/// Dev/test override for the release feed URL.
const FEED_ENV: &str = "LOCALAI_UPDATE_FEED";
/// Largest executable we are willing to download (200 MB).
pub const MAX_EXE_BYTES: u64 = 200 * 1024 * 1024;
/// Largest `.sha256` file we are willing to download (1 KB).
pub const MAX_SHA_BYTES: u64 = 1024;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------- versions

/// A `vMAJOR.MINOR.PATCH[-pre][+build]` version (leading `v` optional).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
}

impl Version {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
        let s = s.split('+').next()?; // build metadata is ignored
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) if !p.is_empty() => (c, p.split('.').map(str::to_string).collect()),
            Some(_) => return None,
            None => (s, Vec::new()),
        };
        let mut parts = core.split('.');
        let num = |p: Option<&str>| {
            p.filter(|x| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit()))?
                .parse()
                .ok()
        };
        let v = Self {
            major: num(parts.next())?,
            minor: num(parts.next())?,
            patch: num(parts.next())?,
            pre,
        };
        if parts.next().is_some() || v.pre.iter().any(String::is_empty) {
            return None;
        }
        Some(v)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater, // 2.1.0 > 2.1.0-beta
                (false, true) => Ordering::Less,
                (false, false) => cmp_pre(&self.pre, &other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Semver pre-release precedence: numeric ids compare numerically and sort
/// before alphanumeric ones; a shorter list sorts first when it is a prefix.
fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

/// True if `latest` is a valid version strictly newer than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (Version::parse(latest), Version::parse(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

// ---------------------------------------------------------------- releases

/// The parts of the latest release the app needs.
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseInfo {
    /// Version without the leading `v`, e.g. `2.1.0`.
    pub version: String,
    pub notes: String,
    pub published_at: Option<String>,
    pub asset_url: String,
    pub sha_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// Parse a GitHub "release" JSON object.
pub fn parse_release(json: &[u8]) -> anyhow::Result<ReleaseInfo> {
    let r: GhRelease = serde_json::from_slice(json).context("invalid release JSON")?;
    if r.draft {
        bail!("release is a draft");
    }
    let v = Version::parse(&r.tag_name)
        .with_context(|| format!("invalid version tag {:?}", r.tag_name))?;
    let url_of = |name: &str| {
        r.assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .map(|a| a.browser_download_url.clone())
    };
    let asset_url =
        url_of(ASSET_NAME).with_context(|| format!("release has no {ASSET_NAME} asset"))?;
    let sha_url =
        url_of(SHA_ASSET_NAME).with_context(|| format!("release has no {SHA_ASSET_NAME} asset"))?;
    let mut version = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        version = format!("{version}-{}", v.pre.join("."));
    }
    Ok(ReleaseInfo {
        version,
        notes: r.body.unwrap_or_default(),
        published_at: r.published_at,
        asset_url,
        sha_url,
    })
}

/// The dev feed override is active (`LOCALAI_UPDATE_FEED` set and non-empty).
fn dev_feed_active() -> bool {
    std::env::var(FEED_ENV).is_ok_and(|u| !u.trim().is_empty())
}

/// Download URLs must be `https://`. Plain `http://` is accepted only for
/// `127.0.0.1` / `localhost` / `[::1]`, and only when `allow_local_http`
/// (i.e. the `LOCALAI_UPDATE_FEED` dev override is set).
pub fn check_download_url(url: &str, allow_local_http: bool) -> anyhow::Result<()> {
    let u = reqwest::Url::parse(url).with_context(|| format!("invalid download URL {url:?}"))?;
    match u.scheme() {
        "https" if u.host_str().is_some() => Ok(()),
        "http" if allow_local_http && is_loopback_url(&u) => Ok(()),
        _ => bail!("refusing to download the update over an insecure URL: {url}"),
    }
}

fn is_loopback_url(u: &reqwest::Url) -> bool {
    matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

fn feed_url() -> String {
    std::env::var(FEED_ENV)
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| format!("https://api.github.com/repos/{REPO}/releases/latest"))
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(format!("local-ai-chat/{CURRENT_VERSION}"))
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(60))
            // Never follow a redirect from https down to plain http.
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    attempt.error("too many redirects")
                } else if check_download_url(attempt.url().as_str(), dev_feed_active()).is_ok() {
                    attempt.follow()
                } else {
                    attempt.error("insecure redirect")
                }
            }))
            .build()
            .expect("http client")
    })
}

/// Fetch the latest release. `Ok(None)` = there is no release (404: repo or
/// release missing). `Err` = network/HTTP/parse problem (message is user-facing).
pub async fn fetch_latest() -> anyhow::Result<Option<ReleaseInfo>> {
    let resp = http()
        .get(feed_url())
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .context("could not reach the update server")?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        bail!(
            "update server rate limit reached, try again later (HTTP {})",
            status.as_u16()
        );
    }
    if !status.is_success() {
        bail!("update server returned HTTP {}", status.as_u16());
    }
    let bytes = resp
        .bytes()
        .await
        .context("could not read the update info")?;
    parse_release(&bytes).map(Some)
}

// ---------------------------------------------------------------- checksums

/// Parse a `sha256sum`-style file (`<64 hex>  <name>`, or just the hex).
pub fn parse_sha256_file(text: &str) -> Option<[u8; 32]> {
    let token = text
        .trim_start_matches('\u{feff}')
        .split_whitespace()
        .next()?;
    if token.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in token.as_bytes().chunks(2).enumerate() {
        let hex = std::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(hex, 16).ok()?;
    }
    Some(out)
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Check `data` against an expected SHA-256 digest.
pub fn verify_sha256(data: &[u8], expected: &[u8; 32]) -> anyhow::Result<()> {
    let actual: [u8; 32] = Sha256::digest(data).into();
    if &actual != expected {
        bail!(
            "checksum mismatch (expected {}, got {})",
            to_hex(expected),
            to_hex(&actual)
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- install

/// Progress reported by [`install`].
#[derive(Clone, Debug)]
pub enum Progress {
    Downloading { downloaded: u64, total: Option<u64> },
    Verifying,
    Restarting,
}

/// Download, verify, swap and relaunch. On success the new exe is running (or
/// starting) and the caller should report "restarting" and call [`request_exit`].
/// On error (insecure URL, oversize, checksum mismatch, I/O) the current exe is
/// left in place.
pub async fn install(
    release: &ReleaseInfo,
    mut progress: impl FnMut(Progress),
) -> anyhow::Result<()> {
    let data = download_verified(release, dev_feed_active(), &mut progress).await?;

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let exe = std::env::current_exe().context("cannot locate the running exe")?;
        swap_exe(&exe, &data)?;
        if let Err(e) = relaunch(&exe) {
            rollback(&exe);
            return Err(e);
        }
        Ok(())
    })
    .await
    .context("update task failed")??;

    progress(Progress::Restarting);
    Ok(())
}

/// Download the checksum and the exe (size-capped, URL-checked) and verify
/// them. Nothing touches the disk here.
async fn download_verified(
    release: &ReleaseInfo,
    allow_local_http: bool,
    progress: &mut impl FnMut(Progress),
) -> anyhow::Result<Vec<u8>> {
    check_download_url(&release.sha_url, allow_local_http)?;
    check_download_url(&release.asset_url, allow_local_http)?;

    let resp = http()
        .get(&release.sha_url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .context("could not download the checksum")?;
    let sha_bytes = read_capped(resp, MAX_SHA_BYTES, "checksum file", |_, _| {})
        .await
        .context("could not download the checksum")?;
    let sha_text = String::from_utf8_lossy(&sha_bytes);
    let expected = parse_sha256_file(&sha_text).context("the checksum file is not valid")?;

    let resp = http()
        .get(&release.asset_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .context("could not download the update")?;
    let data = read_capped(resp, MAX_EXE_BYTES, "update", |downloaded, total| {
        progress(Progress::Downloading { downloaded, total })
    })
    .await?;
    if data.is_empty() {
        bail!("the downloaded file is empty");
    }

    progress(Progress::Verifying);
    verify_sha256(&data, &expected)?;
    Ok(data)
}

/// Read a response body into memory, failing as soon as it exceeds `max`
/// bytes (checked against `Content-Length` first, then while streaming).
/// `on_progress(downloaded, total)` is called at the start, every 256 KB and
/// at the end.
async fn read_capped(
    resp: reqwest::Response,
    max: u64,
    what: &str,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> anyhow::Result<Vec<u8>> {
    let too_big = || anyhow::anyhow!("the {what} is too large (limit {max} bytes)");
    let total = resp.content_length();
    if total.is_some_and(|t| t > max) {
        return Err(too_big());
    }
    let mut data = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut stream = resp.bytes_stream();
    let mut last_report = 0u64;
    on_progress(0, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("{what} download interrupted"))?;
        if data.len() as u64 + chunk.len() as u64 > max {
            return Err(too_big());
        }
        data.extend_from_slice(&chunk);
        let n = data.len() as u64;
        if n - last_report >= 256 * 1024 {
            last_report = n;
            on_progress(n, total);
        }
    }
    on_progress(data.len() as u64, total);
    Ok(data)
}

fn sibling(exe: &Path, name: &str) -> PathBuf {
    exe.with_file_name(name)
}

/// Replace `exe` with `new_bytes`; the previous file is kept as `local-ai-chat.old.exe`.
/// Windows allows renaming (not overwriting) a running exe. On any failure the
/// original exe is back in place.
pub fn swap_exe(exe: &Path, new_bytes: &[u8]) -> anyhow::Result<()> {
    let staged = sibling(exe, STAGED_EXE_NAME);
    let old = sibling(exe, OLD_EXE_NAME);
    std::fs::write(&staged, new_bytes)
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&staged);
        })
        .context("could not write the new version (is the folder writable?)")?;
    let _ = std::fs::remove_file(&old);
    if let Err(e) = std::fs::rename(exe, &old) {
        let _ = std::fs::remove_file(&staged);
        return Err(e).context("could not move the running exe aside");
    }
    if let Err(e) = std::fs::rename(&staged, exe) {
        let _ = std::fs::rename(&old, exe);
        let _ = std::fs::remove_file(&staged);
        return Err(e).context("could not put the new version in place");
    }
    Ok(())
}

/// Undo [`swap_exe`]: put `local-ai-chat.old.exe` back.
fn rollback(exe: &Path) {
    let old = sibling(exe, OLD_EXE_NAME);
    if old.exists() {
        let _ = std::fs::remove_file(exe);
        let _ = std::fs::rename(&old, exe);
    }
}

static RELAUNCH_ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// Extra args for the relaunched exe (e.g. `--serve <port>` in dev mode).
/// `--after-update` is always added.
pub fn set_relaunch_args(args: Vec<String>) {
    let _ = RELAUNCH_ARGS.set(args);
}

fn relaunch(exe: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(RELAUNCH_ARGS.get().into_iter().flatten())
        .arg(AFTER_UPDATE_ARG);
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().context("could not start the new version")?;
    Ok(())
}

// ---------------------------------------------------------------- exit

type ExitHandler = Box<dyn Fn() + Send + Sync>;
static EXIT_HANDLER: OnceLock<ExitHandler> = OnceLock::new();

/// Install how the app quits (the window closes its event loop). Without a
/// handler (`--serve` dev mode) [`request_exit`] exits the process.
pub fn set_exit_handler(f: impl Fn() + Send + Sync + 'static) {
    let _ = EXIT_HANDLER.set(Box::new(f));
}

pub fn request_exit() {
    match EXIT_HANDLER.get() {
        Some(f) => f(),
        None => std::process::exit(0),
    }
}

// ---------------------------------------------------------------- after update

static JUST_UPDATED: Mutex<Option<String>> = Mutex::new(None);

/// Called on start with `--after-update`: remember the new version for the UI
/// and delete the old exe in the background (retrying while the old process exits).
pub fn after_update() {
    *JUST_UPDATED.lock().unwrap_or_else(|e| e.into_inner()) = Some(CURRENT_VERSION.to_string());
    std::thread::spawn(|| {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let old = sibling(&exe, OLD_EXE_NAME);
        for _ in 0..40 {
            if !old.exists() || std::fs::remove_file(&old).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
}

/// The version we just updated to — returned once, then cleared.
pub fn take_just_updated() -> Option<String> {
    JUST_UPDATED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

/// `--serve` after an update: wait (up to ~10 s) until the old process freed the port.
pub fn wait_for_port(port: u16) {
    for _ in 0..50 {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_and_compare() {
        let v = Version::parse("v2.1.0").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (2, 1, 0));
        assert!(Version::parse("2.1").is_none());
        assert!(Version::parse("v2.1.x").is_none());
        assert!(Version::parse("2.1.0.4").is_none());
        assert!(Version::parse("2.1.0-").is_none());
        assert!(Version::parse("").is_none());
        assert!(is_newer("v2.1.0", "2.0.0"));
        assert!(is_newer("v2.0.10", "2.0.9"));
        assert!(is_newer("v10.0.0", "9.9.9"));
        assert!(!is_newer("v2.0.0", "2.0.0"));
        assert!(!is_newer("v1.9.9", "2.0.0"));
        assert!(is_newer("2.1.0", "2.1.0-beta.2"));
        assert!(!is_newer("2.1.0-beta.2", "2.1.0"));
        assert!(is_newer("2.1.0-beta.10", "2.1.0-beta.2"));
        assert!(is_newer("2.1.0-rc.1", "2.1.0-beta.9"));
        assert!(is_newer("2.1.0-alpha.1", "2.1.0-alpha"));
        assert!(!is_newer("2.1.0+build5", "2.1.0"));
        assert!(!is_newer("garbage", "2.0.0"));
    }

    const SHA_LINE: &str =
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08  local-ai-chat.exe\n";

    #[test]
    fn sha256_parse_and_verify() {
        let expected = parse_sha256_file(SHA_LINE).unwrap();
        assert_eq!(
            to_hex(&expected),
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
        verify_sha256(b"test", &expected).unwrap();
        assert!(verify_sha256(b"tesT", &expected).is_err());
        assert!(
            parse_sha256_file("9F86D081884C7D659A2FEAA0C55AD015A3BF4F1B2B0B822CD15D6C15B0F00A08")
                .is_some()
        );
        assert!(parse_sha256_file("").is_none());
        assert!(parse_sha256_file("abc  local-ai-chat.exe").is_none());
        assert!(parse_sha256_file(&"zz".repeat(32)).is_none());
    }

    const RELEASE_JSON: &str = r###"{
        "url": "https://api.github.com/repos/x/y/releases/1",
        "tag_name": "v2.1.0",
        "name": "Local AI Chat 2.1.0",
        "draft": false,
        "prerelease": false,
        "published_at": "2026-10-01T12:00:00Z",
        "body": "## What's new\n- Auto update",
        "assets": [
            {"name": "local-ai-chat.exe.sha256", "size": 84, "browser_download_url": "https://github.com/x/y/releases/download/v2.1.0/local-ai-chat.exe.sha256"},
            {"name": "local-ai-chat.exe", "size": 12345678, "browser_download_url": "https://github.com/x/y/releases/download/v2.1.0/local-ai-chat.exe"}
        ]
    }"###;

    #[test]
    fn release_json_parsing() {
        let r = parse_release(RELEASE_JSON.as_bytes()).unwrap();
        assert_eq!(r.version, "2.1.0");
        assert_eq!(r.notes, "## What's new\n- Auto update");
        assert_eq!(r.published_at.as_deref(), Some("2026-10-01T12:00:00Z"));
        assert!(r.asset_url.ends_with("/local-ai-chat.exe"));
        assert!(r.sha_url.ends_with("/local-ai-chat.exe.sha256"));

        let no_assets = r#"{"tag_name": "v2.1.0", "assets": []}"#;
        assert!(parse_release(no_assets.as_bytes()).is_err());
        let bad_tag =
            RELEASE_JSON.replace("v2.1.0\",\n        \"name", "nightly\",\n        \"name");
        assert!(parse_release(bad_tag.as_bytes()).is_err());
        let github_404 =
            r#"{"message": "Not Found", "documentation_url": "https://docs.github.com/rest"}"#;
        assert!(parse_release(github_404.as_bytes()).is_err());
    }

    /// Serve one canned HTTP response on a loopback port; returns its URL.
    fn one_shot_server(status_line: &'static str, body: &'static str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/latest", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = write!(
                    s,
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        url
    }

    #[tokio::test]
    async fn feed_404_and_errors() {
        // Only this test touches the env var (tests in this module run it serially here).
        // SAFETY: no other test reads or writes LOCALAI_UPDATE_FEED.
        unsafe {
            std::env::set_var(
                FEED_ENV,
                one_shot_server("404 Not Found", r#"{"message":"Not Found"}"#),
            )
        };
        assert_eq!(fetch_latest().await.unwrap(), None);

        unsafe {
            std::env::set_var(
                FEED_ENV,
                one_shot_server("403 Forbidden", r#"{"message":"rate limit"}"#),
            )
        };
        assert!(
            fetch_latest()
                .await
                .unwrap_err()
                .to_string()
                .contains("rate limit")
        );

        unsafe { std::env::set_var(FEED_ENV, one_shot_server("200 OK", RELEASE_JSON)) };
        assert_eq!(fetch_latest().await.unwrap().unwrap().version, "2.1.0");

        unsafe { std::env::set_var(FEED_ENV, "http://127.0.0.1:1/unreachable") };
        assert!(fetch_latest().await.is_err());
        unsafe { std::env::remove_var(FEED_ENV) };
    }

    #[test]
    fn download_urls_must_be_https() {
        let gh = "https://github.com/x/y/releases/download/v2.1.0/local-ai-chat.exe";
        assert!(check_download_url(gh, false).is_ok());
        assert!(check_download_url(gh, true).is_ok());
        assert!(check_download_url("http://github.com/x.exe", false).is_err());
        assert!(check_download_url("http://github.com/x.exe", true).is_err());
        assert!(check_download_url("http://127.0.0.1:8080/x.exe", false).is_err());
        assert!(check_download_url("http://127.0.0.1:8080/x.exe", true).is_ok());
        assert!(check_download_url("http://localhost/x.exe", true).is_ok());
        assert!(check_download_url("http://[::1]:9/x.exe", true).is_ok());
        assert!(check_download_url("http://127.0.0.1.evil.com/x.exe", true).is_err());
        assert!(check_download_url("http://localhost@evil.com/x.exe", true).is_err());
        assert!(check_download_url("file:///C:/x.exe", true).is_err());
        assert!(check_download_url("ftp://example.com/x.exe", true).is_err());
        assert!(check_download_url("not a url", true).is_err());
    }

    /// Serve fixed bodies by path on a loopback port until the test ends.
    /// `with_len = false` omits Content-Length (body ends at connection close).
    fn file_server(routes: Vec<(&'static str, Vec<u8>, bool)>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { return };
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let Some((_, body, with_len)) = routes.iter().find(|r| r.0 == path) else {
                    let _ = s.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    continue;
                };
                let len = if *with_len {
                    format!("Content-Length: {}\r\n", body.len())
                } else {
                    String::new()
                };
                let _ = write!(s, "HTTP/1.1 200 OK\r\n{len}Connection: close\r\n\r\n");
                let _ = s.write_all(body);
            }
        });
        base
    }

    fn release_at(base: &str, exe: &str, sha: &str) -> ReleaseInfo {
        ReleaseInfo {
            version: "9.9.9".into(),
            notes: String::new(),
            published_at: None,
            asset_url: format!("{base}{exe}"),
            sha_url: format!("{base}{sha}"),
        }
    }

    #[tokio::test]
    async fn download_is_verified_and_capped() {
        let good_sha = format!(
            "{}  local-ai-chat.exe\n",
            to_hex(&Sha256::digest(b"new exe"))
        );
        let base = file_server(vec![
            ("/exe", b"new exe".to_vec(), true),
            ("/exe-nolen", b"new exe".to_vec(), false),
            ("/tampered", b"evil exe".to_vec(), true),
            ("/sha", good_sha.into_bytes(), true),
            ("/sha-big", vec![b'a'; MAX_SHA_BYTES as usize + 1], true),
            (
                "/sha-big-nolen",
                vec![b'a'; MAX_SHA_BYTES as usize + 1],
                false,
            ),
        ]);
        let mut events = 0;
        let mut progress = |_: Progress| events += 1;

        let data = download_verified(&release_at(&base, "/exe", "/sha"), true, &mut progress)
            .await
            .unwrap();
        assert_eq!(data, b"new exe");
        let data = download_verified(
            &release_at(&base, "/exe-nolen", "/sha"),
            true,
            &mut progress,
        )
        .await
        .unwrap();
        assert_eq!(data, b"new exe");

        let err = download_verified(&release_at(&base, "/tampered", "/sha"), true, &mut progress)
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("checksum mismatch"), "{err:#}");

        for sha in ["/sha-big", "/sha-big-nolen"] {
            let err = download_verified(&release_at(&base, "/exe", sha), true, &mut progress)
                .await
                .unwrap_err();
            assert!(format!("{err:#}").contains("too large"), "{err:#}");
        }

        // Local http without the dev override is refused before any request.
        let err = download_verified(&release_at(&base, "/exe", "/sha"), false, &mut progress)
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("insecure"), "{err:#}");
        assert!(events > 0);
    }

    #[tokio::test]
    async fn read_capped_enforces_limit_with_and_without_length() {
        let base = file_server(vec![
            ("/len", vec![7u8; 5000], true),
            ("/nolen", vec![7u8; 5000], false),
        ]);
        for path in ["/len", "/nolen"] {
            let url = format!("{base}{path}");
            let resp = http().get(&url).send().await.unwrap();
            let err = read_capped(resp, 4999, "update", |_, _| {})
                .await
                .unwrap_err();
            assert!(err.to_string().contains("too large"), "{path}: {err:#}");
            let resp = http().get(&url).send().await.unwrap();
            assert_eq!(
                read_capped(resp, 5000, "update", |_, _| {})
                    .await
                    .unwrap()
                    .len(),
                5000
            );
        }
    }

    #[test]
    fn swap_and_rollback() {
        let dir = std::env::temp_dir().join(format!("lac-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("local-ai-chat.exe");
        std::fs::write(&exe, b"old").unwrap();

        swap_exe(&exe, b"new").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join(OLD_EXE_NAME)).unwrap(), b"old");
        assert!(!dir.join(STAGED_EXE_NAME).exists());

        rollback(&exe);
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        assert!(!dir.join(OLD_EXE_NAME).exists());

        // Moving the exe aside fails (it doesn't exist): nothing else changes.
        let missing = dir.join("missing.exe");
        assert!(swap_exe(&missing, b"new").is_err());
        assert!(!dir.join(STAGED_EXE_NAME).exists());
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
