//! Auto-update (specs: `docs/specs/auto-update.md`, `docs/specs/update-manifest.md`).
//!
//! - `fetch_latest()` asks the signed update manifest on fastkiwi.app first
//!   ([`MANIFEST_URL`]) and GitHub Releases of [`REPO`] as the fallback (a
//!   [`ServerUnreachable`] error when GitHub refuses or finds no release / no
//!   repo); `is_newer()` compares it to this build. See [`resolve`].
//! - `install()` downloads the `local-ai-chat.exe` asset from each address in
//!   order (GitHub first, then the GitLab copy), verifies it against the manifest's SHA-256 or the
//!   `local-ai-chat.exe.sha256` asset and — once [`TRUSTED_PUBLIC_KEYS`] is not
//!   empty — against the minisign signature `local-ai-chat.exe.minisig`
//!   (spec: `docs/specs/signing.md`), swaps it in place of the running exe
//!   (running exe → `local-ai-chat.old.exe`), and spawns it with `--after-update`.
//! - `after_update()` (called from `main` on `--after-update`) deletes the old exe
//!   and remembers the new version so the UI can show a "updated" toast once.
//! - `request_exit()` asks the app to quit (the window installs a handler; in
//!   `--serve` mode the process just exits).
//!
//! Dev overrides: if `LOCALAI_UPDATE_FEED=<url>` is set, that URL is used instead of
//! the GitHub API (it must return JSON shaped like a GitHub release) and the
//! manifest is not asked unless `LOCALAI_UPDATE_MANIFEST=<url>` is set too
//! (`off` turns the manifest off). The signature rules are exactly the same
//! with or without the overrides.

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
/// Release asset with the minisign signature of [`ASSET_NAME`] (made in CI by the
/// `sign` job of `.github/workflows/release.yml`).
pub const SIG_ASSET_NAME: &str = "local-ai-chat.exe.minisig";
/// The previous exe is renamed to this (next to the running exe) during the swap.
pub const OLD_EXE_NAME: &str = "local-ai-chat.old.exe";
/// The new exe is written here first, then renamed into place.
const STAGED_EXE_NAME: &str = "local-ai-chat.new.exe";
/// Command-line flag passed to the freshly installed exe.
pub const AFTER_UPDATE_ARG: &str = "--after-update";
/// Dev/test override for the release feed URL.
const FEED_ENV: &str = "LOCALAI_UPDATE_FEED";
/// Dev/test override for the update manifest URL (`off`: do not ask it).
const MANIFEST_ENV: &str = "LOCALAI_UPDATE_MANIFEST";
/// The signed update manifest on the website (Cloudflare Pages), asked before
/// GitHub so updates keep working while the GitHub API is down
/// (`docs/specs/update-manifest.md`). Its signature is `<this>.minisig`.
pub const MANIFEST_URL: &str = "https://fastkiwi.app/updates/local-ai-chat/latest.json";
/// The fallback copies of the exe that the manifest may list after GitHub's:
/// release assets of this repository's GitLab copy (populated later; until
/// then they answer 404). No other GitLab project is accepted.
pub const GITLAB_HOST: &str = "gitlab.com";
pub const GITLAB_RELEASES_PATH: &str = "/fastkiwi1/local-ai-chat/-/releases/";
/// The manifest's `app`.
pub const MANIFEST_APP: &str = "local-ai-chat";
/// This exe's key in the manifest's `assets` (Lemon ships x64 only).
const MANIFEST_ARCH: &str = "x64";
/// Largest manifest we are willing to download (64 KB; a real one is ~1 KB).
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
/// Largest executable we are willing to download (200 MB).
pub const MAX_EXE_BYTES: u64 = 200 * 1024 * 1024;
/// Largest `.sha256` file we are willing to download (1 KB).
pub const MAX_SHA_BYTES: u64 = 1024;
/// Largest `.minisig` file we are willing to download (4 KB; a real one is ~350 bytes).
pub const MAX_SIG_BYTES: u64 = 4096;

// ---------------------------------------------------------------- signing keys

// TODO(owner): paste minisign public keys here.
//
// Each entry is the SECOND line of a `minisign.pub` file (the base64 key that
// starts with `RW`), NOT the "untrusted comment" line and never a secret key.
// Put the primary key first and the offline backup key second, e.g.
//
//     pub const TRUSTED_PUBLIC_KEYS: &[&str] = &[
//         "RWQ...primary...", // primary: keys/minisign-primary.pub (signs releases in CI)
//         "RWQ...backup...",  // backup: keys/minisign-backup.pub (secret kept offline)
//     ];
//
// How the keys are made and where the secrets live: docs/specs/signing.md.
// Commit keys/minisign-{primary,backup}.pub in the SAME change: the release
// workflow signs only when they exist (test `keys_dir_matches_trusted_keys`).
//
// While this list is EMPTY, updates are verified with the sha256 file only (as
// in 2.2.0 and earlier) and a log line says signature checking is not
// configured. As soon as it holds at least one key, every update MUST carry a
// valid `local-ai-chat.exe.minisig` made by one of these keys, or it is refused.
// This is decided at compile time only: there is no runtime or server switch.
/// Production minisign public keys trusted to sign updates (see the comment above).
#[cfg_attr(feature = "dev-test-update-key", allow(dead_code))]
pub const TRUSTED_PUBLIC_KEYS: &[&str] = &[];

/// TEST-ONLY key pair (public on purpose, protects nothing); see
/// `tests/fixtures/update-signing/regen.sh`.
#[cfg(any(test, feature = "dev-test-update-key"))]
const TEST_PRIMARY_PUB: &str = include_str!("../tests/fixtures/update-signing/test-primary.pub");

#[cfg(all(feature = "dev-test-update-key", not(debug_assertions)))]
compile_error!(
    "the `dev-test-update-key` feature trusts a public TEST key; it is for local debug builds only"
);

/// The base64 key line of a `minisign.pub` file (its second line).
#[cfg(any(test, feature = "dev-test-update-key"))]
fn pub_key_line(pub_file: &str) -> &str {
    pub_file.lines().nth(1).unwrap_or_default().trim()
}

/// The keys this build trusts: [`TRUSTED_PUBLIC_KEYS`], or — only in a debug build
/// with the `dev-test-update-key` feature — the TEST key instead.
pub fn trusted_keys() -> Vec<&'static str> {
    #[cfg(feature = "dev-test-update-key")]
    {
        vec![pub_key_line(TEST_PRIMARY_PUB)]
    }
    #[cfg(not(feature = "dev-test-update-key"))]
    {
        TRUSTED_PUBLIC_KEYS.to_vec()
    }
}

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
    /// Where `local-ai-chat.exe` can be downloaded, tried in order (GitHub
    /// first, then the GitLab copy for a release from the manifest).
    pub asset_urls: Vec<String>,
    /// GitHub's `local-ai-chat.exe.sha256` asset. `None` when [`Self::sha256`]
    /// comes from the manifest.
    pub sha_url: Option<String>,
    /// The expected SHA-256 of the exe, from the manifest.
    pub sha256: Option<[u8; 32]>,
    /// `local-ai-chat.exe.minisig` next to each download address, tried in order;
    /// empty if the release has none. Optional here so the check still reports
    /// the new version; [`install`] refuses an update without it once
    /// [`TRUSTED_PUBLIC_KEYS`] is not empty.
    pub sig_urls: Vec<String>,
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
    let sig_url = url_of(SIG_ASSET_NAME);
    let mut version = format!("{}.{}.{}", v.major, v.minor, v.patch);
    if !v.pre.is_empty() {
        version = format!("{version}-{}", v.pre.join("."));
    }
    Ok(ReleaseInfo {
        version,
        notes: r.body.unwrap_or_default(),
        published_at: r.published_at,
        asset_urls: vec![asset_url],
        sha_url: Some(sha_url),
        sha256: None,
        sig_urls: sig_url.into_iter().collect(),
    })
}

// ---------------------------------------------------------------- manifest

#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    app: String,
    version: String,
    tag: String,
    #[serde(default)]
    released: Option<String>,
    #[serde(default)]
    notes: String,
    assets: std::collections::HashMap<String, ManifestAsset>,
}

#[derive(Deserialize)]
struct ManifestAsset {
    name: String,
    size: u64,
    sha256: String,
    urls: Vec<String>,
}

/// Parse the update manifest (`docs/specs/update-manifest.md`) into a release,
/// whatever its version. Download addresses that are not `https://` (see
/// [`check_download_url`]) on GitHub or this project's GitLab releases (see
/// [`manifest_host_ok`]), or that name another file, are dropped; at least one
/// must remain.
pub fn parse_manifest(json: &[u8], allow_local_http: bool) -> anyhow::Result<ReleaseInfo> {
    if json.len() as u64 > MAX_MANIFEST_BYTES {
        bail!("the update manifest is too large");
    }
    let m: Manifest = serde_json::from_slice(json).context("invalid update manifest")?;
    if m.schema != 1 {
        bail!("unknown update manifest schema {}", m.schema);
    }
    if m.app != MANIFEST_APP {
        bail!("the update manifest is for {:?}", m.app);
    }
    let v = Version::parse(&m.version).filter(|v| v.pre.is_empty());
    let normalized = v.map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch));
    if normalized.as_deref() != Some(m.version.as_str()) || m.tag != format!("v{}", m.version) {
        bail!(
            "bad version {:?} / tag {:?} in the update manifest",
            m.version,
            m.tag
        );
    }
    let a = m
        .assets
        .get(MANIFEST_ARCH)
        .filter(|a| a.name == ASSET_NAME)
        .with_context(|| format!("the update manifest has no {ASSET_NAME}"))?;
    if a.size == 0 || a.size > MAX_EXE_BYTES {
        bail!("bad size {} in the update manifest", a.size);
    }
    let sha256 = Some(a.sha256.as_str())
        .filter(|h| h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .and_then(parse_sha256_file)
        .context("bad sha256 in the update manifest")?;
    let asset_urls: Vec<String> = a
        .urls
        .iter()
        .filter(|u| {
            check_download_url(u, allow_local_http).is_ok()
                && reqwest::Url::parse(u).is_ok_and(|p| {
                    // Exactly as written: nothing the URL parser would rewrite.
                    p.as_str() == u.as_str()
                        && manifest_host_ok(&p, allow_local_http)
                        && p.query().is_none()
                        && p.fragment().is_none()
                        && p.path_segments().and_then(|mut s| s.next_back()) == Some(ASSET_NAME)
                })
        })
        .take(8)
        .cloned()
        .collect();
    if asset_urls.is_empty() {
        bail!("the update manifest has no usable download address");
    }
    Ok(ReleaseInfo {
        sig_urls: asset_urls.iter().map(|u| format!("{u}.minisig")).collect(),
        asset_urls,
        sha_url: None,
        sha256: Some(sha256),
        version: m.version,
        notes: m.notes,
        published_at: m.released,
    })
}

/// A download host the manifest may name: GitHub (and its download hosts),
/// or a release asset of this project on GitLab (`GITLAB_RELEASES_PATH`, no
/// `%` escapes). The loopback only with the dev override, for tests.
pub fn manifest_host_ok(u: &reqwest::Url, allow_local_http: bool) -> bool {
    let Some(host) = u.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    if allow_local_http && is_loopback_url(u) {
        return true;
    }
    if u.scheme() != "https"
        || u.port().is_some()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return false;
    }
    host == "github.com"
        || host.ends_with(".github.com")
        || host.ends_with(".githubusercontent.com")
        || (host == GITLAB_HOST
            && u.path().starts_with(GITLAB_RELEASES_PATH)
            && !u.path().contains('%'))
}

/// The trusted comment the manifest's signature must carry, e.g.
/// `local-ai-chat update-manifest v2.3.0`. Unlike the exe's (`local-ai-chat v2.3.0`),
/// so one signature can never stand in for the other.
pub fn expected_manifest_comment(version: &str) -> String {
    format!("{MANIFEST_APP} update-manifest v{version}")
}

/// The manifest URL to ask first, if any: [`MANIFEST_URL`], or the
/// `LOCALAI_UPDATE_MANIFEST` override. With only the `LOCALAI_UPDATE_FEED`
/// override set (local tests of the GitHub path), none.
fn manifest_url() -> Option<String> {
    match std::env::var(MANIFEST_ENV)
        .ok()
        .filter(|u| !u.trim().is_empty())
    {
        Some(u) if u.trim().eq_ignore_ascii_case("off") => None,
        Some(u) => Some(u),
        None if std::env::var(FEED_ENV).is_ok_and(|u| !u.trim().is_empty()) => None,
        None => Some(MANIFEST_URL.to_string()),
    }
}

/// GET a small file (status 200..299, at most `max` bytes).
async fn get_small(
    url: &str,
    max: u64,
    what: &str,
    allow_local_http: bool,
) -> anyhow::Result<Vec<u8>> {
    check_download_url(url, allow_local_http)?;
    let resp = http()
        .get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .with_context(|| format!("could not download the {what}"))?;
    if !resp.status().is_success() {
        bail!("the {what} server returned HTTP {}", resp.status().as_u16());
    }
    read_capped(resp, max, what, |_, _| {}).await
}

/// Download and parse the manifest at `url`; with `keys`, also download
/// `<url>.minisig` and verify it (see [`verify_minisign`]).
async fn fetch_manifest(
    url: &str,
    keys: &[&str],
    allow_local_http: bool,
) -> anyhow::Result<ReleaseInfo> {
    let data = get_small(url, MAX_MANIFEST_BYTES, "update manifest", allow_local_http).await?;
    let release = parse_manifest(&data, allow_local_http)?;
    if !keys.is_empty() {
        let sig = get_small(
            &format!("{url}.minisig"),
            MAX_SIG_BYTES,
            "manifest signature",
            allow_local_http,
        )
        .await
        .map_err(|e| sig_error(format!("{e:#}")))?;
        let sig = std::str::from_utf8(&sig)
            .map_err(|_| sig_error("the manifest signature is not valid"))?;
        verify_minisign(
            &data,
            sig,
            &expected_manifest_comment(&release.version),
            keys,
        )?;
    }
    Ok(release)
}

/// The SHA-256 GitHub publishes for `release` (its `.sha256` asset).
async fn github_sha256(release: &ReleaseInfo, allow_local_http: bool) -> Option<[u8; 32]> {
    if release.sha256.is_some() {
        return release.sha256;
    }
    let bytes = get_small(
        release.sha_url.as_deref()?,
        MAX_SHA_BYTES,
        "checksum file",
        allow_local_http,
    )
    .await
    .ok()?;
    parse_sha256_file(&String::from_utf8_lossy(&bytes))
}

/// The release to report (`docs/specs/update-manifest.md`):
/// 1. Fetch the manifest; on any failure use the GitHub `feed`.
/// 2. With `keys`, its signature must verify. Without keys (today), it is used
///    only if GitHub reports the same version with the same SHA-256.
/// 3. A trusted manifest newer than `current` is returned with its download
///    addresses in order (GitHub, then GitLab). A trusted manifest that is not
///    newer is returned (up to date) unless GitHub has a newer release (a stale
///    manifest); a failed GitHub check then does not matter.
///
/// The caller offers only a strictly newer version ([`is_newer`]): never a downgrade.
pub async fn resolve(
    manifest: Option<&str>,
    feed: &str,
    keys: &[&str],
    current: &str,
    allow_local_http: bool,
) -> anyhow::Result<ReleaseInfo> {
    let manifest = match manifest {
        Some(url) => fetch_manifest(url, keys, allow_local_http).await,
        None => return fetch_github(feed).await,
    };
    let m = match manifest {
        Ok(m) => m,
        Err(e) => {
            eprintln!("update: manifest not used: {e:#}");
            return fetch_github(feed).await;
        }
    };
    if keys.is_empty() {
        // No keys yet: trusted only when GitHub publishes the same exe.
        let gh = fetch_github(feed).await;
        if let Ok(g) = &gh
            && g.version == m.version
            && github_sha256(g, allow_local_http).await == m.sha256
        {
            return Ok(m);
        }
        eprintln!(
            "update: manifest not used: GitHub does not confirm v{} with the same sha256",
            m.version
        );
        return gh;
    }
    if is_newer(&m.version, current) {
        return Ok(m);
    }
    match fetch_github(feed).await {
        Ok(g) if is_newer(&g.version, current) && is_newer(&g.version, &m.version) => Ok(g),
        _ => Ok(m),
    }
}

/// A dev override is active (`LOCALAI_UPDATE_FEED` or `LOCALAI_UPDATE_MANIFEST`
/// set and non-empty).
fn dev_feed_active() -> bool {
    [FEED_ENV, MANIFEST_ENV]
        .iter()
        .any(|k| std::env::var(k).is_ok_and(|u| !u.trim().is_empty()))
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

/// The update server refused the check or has no release for us (HTTP 403,
/// 404, 429: rate limit, repository missing or unavailable). Nothing the user
/// can fix and nothing wrong with the app, so the UI shows a neutral, localized
/// message (`code: "unreachable"`, string `upd_unreachable`) on a manual check
/// only; automatic checks stay silent and simply run again later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerUnreachable(pub u16);

impl std::fmt::Display for ServerUnreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(SERVER_UNREACHABLE_MESSAGE)
    }
}

impl std::error::Error for ServerUnreachable {}

/// English text of [`ServerUnreachable`] (the UI shows its own `upd_unreachable`).
pub const SERVER_UNREACHABLE_MESSAGE: &str =
    "The update server can't be reached right now. Your app keeps working; we'll try again later.";

/// True if `e` (or anything in its chain) is a [`ServerUnreachable`].
pub fn is_server_unreachable(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<ServerUnreachable>())
}

/// The error for a non-success HTTP status of the release feed.
fn feed_status_error(status: u16) -> anyhow::Error {
    match status {
        403 | 404 | 429 => anyhow::Error::new(ServerUnreachable(status)),
        _ => anyhow::anyhow!("update server returned HTTP {status}"),
    }
}

/// Fetch the latest release: the manifest first, GitHub as the fallback
/// ([`resolve`]). `Err` = network/HTTP/parse problem (message is user-facing);
/// [`ServerUnreachable`] when GitHub refuses or has no release.
pub async fn fetch_latest() -> anyhow::Result<ReleaseInfo> {
    let keys = trusted_keys();
    resolve(
        manifest_url().as_deref(),
        &feed_url(),
        &keys,
        CURRENT_VERSION,
        dev_feed_active(),
    )
    .await
}

/// Fetch the latest GitHub release from `feed`.
async fn fetch_github(feed: &str) -> anyhow::Result<ReleaseInfo> {
    let resp = http()
        .get(feed)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .context("could not reach the update server")?;
    let status = resp.status();
    if !status.is_success() {
        return Err(feed_status_error(status.as_u16()));
    }
    let bytes = resp
        .bytes()
        .await
        .context("could not read the update info")?;
    parse_release(&bytes)
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

// ---------------------------------------------------------------- signature

/// The update could not be proven to come from the owner's signing key
/// (signature missing, unreadable, made by another key, or for another
/// version). The UI shows a dedicated message for it (`code: "signature"`).
#[derive(Debug)]
pub struct SignatureError(pub String);

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the update could not be verified ({}); it was not installed and the current \
             version was kept. Download it manually from https://github.com/{REPO}/releases \
             or https://fastkiwi.app/",
            self.0
        )
    }
}

impl std::error::Error for SignatureError {}

/// True if `e` (or anything in its chain) is a [`SignatureError`].
pub fn is_signature_error(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<SignatureError>())
}

fn sig_error(why: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(SignatureError(why.into()))
}

/// The trusted comment a release signature must carry, e.g. `local-ai-chat v2.3.0`
/// (`version` as in [`ReleaseInfo::version`]; the release tag is `v<version>`).
/// Binding the version stops an older signed exe from being replayed as a newer release.
pub fn expected_trusted_comment(version: &str) -> String {
    format!("local-ai-chat v{version}")
}

/// Verify `data` against the minisign signature file `sig_text`: it must be a
/// (pre-hashed) signature by one of `keys` (base64 public keys) whose trusted
/// comment is exactly [`expected_trusted_comment`]`(version)`. Errors are
/// [`SignatureError`]s. An empty `keys` list never verifies.
pub fn verify_signature(
    data: &[u8],
    sig_text: &str,
    version: &str,
    keys: &[&str],
) -> anyhow::Result<()> {
    verify_minisign(data, sig_text, &expected_trusted_comment(version), keys)
}

/// [`verify_signature`] for any trusted comment (`expected`); the manifest's is
/// [`expected_manifest_comment`].
pub fn verify_minisign(
    data: &[u8],
    sig_text: &str,
    expected: &str,
    keys: &[&str],
) -> anyhow::Result<()> {
    let sig = minisign_verify::Signature::decode(sig_text.trim_start_matches('\u{feff}'))
        .map_err(|e| sig_error(format!("the signature file is not valid: {e}")))?;
    for key in keys {
        let Ok(pk) = minisign_verify::PublicKey::from_base64(key.trim()) else {
            continue; // a malformed compiled-in key is caught by a unit test
        };
        // `false`: legacy (non-pre-hashed) signatures are not accepted.
        if pk.verify(data, &sig, false).is_ok() {
            if sig.trusted_comment() != expected {
                return Err(sig_error(format!(
                    "the signature is for {:?}, not {expected:?}",
                    sig.trusted_comment()
                )));
            }
            return Ok(());
        }
    }
    Err(sig_error("the signature does not match a trusted key"))
}

/// Download the release's `.minisig` and verify `data` with it (see
/// [`verify_signature`]), from each address in order until one verifies.
async fn check_release_signature(
    release: &ReleaseInfo,
    data: &[u8],
    keys: &[&str],
    allow_local_http: bool,
) -> anyhow::Result<()> {
    if release.sig_urls.is_empty() {
        return Err(sig_error(format!("the release has no {SIG_ASSET_NAME}")));
    }
    let mut last = None;
    for url in &release.sig_urls {
        match check_signature_at(url, release, data, keys, allow_local_http).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("update: signature from {url} not used: {e:#}");
                last = Some(e);
            }
        }
    }
    Err(last.expect("at least one signature address"))
}

async fn check_signature_at(
    sig_url: &str,
    release: &ReleaseInfo,
    data: &[u8],
    keys: &[&str],
    allow_local_http: bool,
) -> anyhow::Result<()> {
    check_download_url(sig_url, allow_local_http)?;
    let resp = http()
        .get(sig_url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| sig_error(format!("could not download the signature: {e}")))?;
    let sig_bytes = read_capped(resp, MAX_SIG_BYTES, "signature file", |_, _| {})
        .await
        .map_err(|e| sig_error(format!("could not download the signature: {e:#}")))?;
    let sig_text = std::str::from_utf8(&sig_bytes)
        .map_err(|_| sig_error("the signature file is not valid"))?;
    verify_signature(data, sig_text, &release.version, keys)
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
/// On error (insecure URL, oversize, checksum mismatch, missing/bad signature,
/// I/O) the current exe is left in place.
pub async fn install(
    release: &ReleaseInfo,
    mut progress: impl FnMut(Progress),
) -> anyhow::Result<()> {
    let keys = trusted_keys();
    let data = download_verified(release, &keys, dev_feed_active(), &mut progress).await?;

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

/// Get the expected checksum (the manifest's, or GitHub's `.sha256` asset),
/// then download the exe from each address in order until a copy matches it
/// (size-capped, URL-checked); then, unless `keys` is empty, download and
/// verify the signature. Nothing touches the disk here.
async fn download_verified(
    release: &ReleaseInfo,
    keys: &[&str],
    allow_local_http: bool,
    progress: &mut impl FnMut(Progress),
) -> anyhow::Result<Vec<u8>> {
    let expected = match release.sha256 {
        Some(h) => h,
        None => {
            let sha_url = release
                .sha_url
                .as_deref()
                .context("the release has no checksum")?;
            check_download_url(sha_url, allow_local_http)?;
            let resp = http()
                .get(sha_url)
                .timeout(Duration::from_secs(30))
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .context("could not download the checksum")?;
            let sha_bytes = read_capped(resp, MAX_SHA_BYTES, "checksum file", |_, _| {})
                .await
                .context("could not download the checksum")?;
            let sha_text = String::from_utf8_lossy(&sha_bytes);
            parse_sha256_file(&sha_text).context("the checksum file is not valid")?
        }
    };

    let mut last = None;
    let mut data = None;
    for url in &release.asset_urls {
        match download_one(url, &expected, allow_local_http, progress).await {
            Ok(d) => {
                data = Some(d);
                break;
            }
            Err(e) => {
                eprintln!("update: {url} not used: {e:#}");
                last = Some(e);
            }
        }
    }
    let Some(data) = data else {
        return Err(last.unwrap_or_else(|| anyhow::anyhow!("the release has no download address")));
    };
    if keys.is_empty() {
        eprintln!(
            "update: signature checking is not configured (update::TRUSTED_PUBLIC_KEYS is empty); \
             verified the sha256 only"
        );
    } else {
        check_release_signature(release, &data, keys, allow_local_http).await?;
    }
    Ok(data)
}

/// Download one copy of the exe from `url` and check it against `expected`.
async fn download_one(
    url: &str,
    expected: &[u8; 32],
    allow_local_http: bool,
    progress: &mut impl FnMut(Progress),
) -> anyhow::Result<Vec<u8>> {
    check_download_url(url, allow_local_http)?;
    let resp = http()
        .get(url)
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
    verify_sha256(&data, expected)?;
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
        assert_eq!(r.asset_urls.len(), 1);
        assert!(r.asset_urls[0].ends_with("/local-ai-chat.exe"));
        assert!(
            r.sha_url
                .as_deref()
                .unwrap()
                .ends_with("/local-ai-chat.exe.sha256")
        );
        assert_eq!(r.sha256, None);
        assert!(r.sig_urls.is_empty()); // optional: old releases have no signature

        let signed = RELEASE_JSON.replace(
            r#""assets": ["#,
            r#""assets": [{"name": "local-ai-chat.exe.minisig", "browser_download_url": "https://github.com/x/y/releases/download/v2.1.0/local-ai-chat.exe.minisig"},"#,
        );
        let r = parse_release(signed.as_bytes()).unwrap();
        assert_eq!(r.sig_urls.len(), 1);
        assert!(r.sig_urls[0].ends_with("/local-ai-chat.exe.minisig"));

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
        // 403 (rate limit or blocked), 404 (no release / repo missing or suspended)
        // and 429 all give the neutral "can't be reached" error, never "try again in an hour".
        for (status_line, body) in [
            ("404 Not Found", r#"{"message":"Not Found"}"#),
            ("403 Forbidden", r#"{"message":"API rate limit exceeded"}"#),
            ("429 Too Many Requests", r#"{"message":"slow down"}"#),
        ] {
            unsafe { std::env::set_var(FEED_ENV, one_shot_server(status_line, body)) };
            let e = fetch_latest().await.unwrap_err();
            assert!(is_server_unreachable(&e), "{status_line}: {e:#}");
            assert_eq!(
                format!("{e:#}"),
                SERVER_UNREACHABLE_MESSAGE,
                "{status_line}"
            );
            assert!(!format!("{e:#}").contains("rate limit"), "{status_line}");
        }

        unsafe { std::env::set_var(FEED_ENV, one_shot_server("500 Internal Server Error", "{}")) };
        let e = fetch_latest().await.unwrap_err();
        assert!(
            !is_server_unreachable(&e) && format!("{e:#}").contains("HTTP 500"),
            "{e:#}"
        );

        unsafe { std::env::set_var(FEED_ENV, one_shot_server("200 OK", RELEASE_JSON)) };
        assert_eq!(fetch_latest().await.unwrap().version, "2.1.0");

        unsafe { std::env::set_var(FEED_ENV, "http://127.0.0.1:1/unreachable") };
        let e = fetch_latest().await.unwrap_err();
        assert!(!is_server_unreachable(&e), "{e:#}");
        unsafe { std::env::remove_var(FEED_ENV) };
    }

    #[test]
    fn unreachable_message_is_neutral() {
        assert_eq!(
            SERVER_UNREACHABLE_MESSAGE,
            "The update server can't be reached right now. Your app keeps working; we'll try again later."
        );
        for status in [403u16, 404, 429] {
            let e = feed_status_error(status);
            assert!(is_server_unreachable(&e));
            assert!(is_server_unreachable(&e.context("checking for updates")));
        }
        for status in [400u16, 500, 502] {
            assert!(!is_server_unreachable(&feed_status_error(status)));
        }
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
            asset_urls: vec![format!("{base}{exe}")],
            sha_url: Some(format!("{base}{sha}")),
            sha256: None,
            sig_urls: Vec::new(),
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

        let data = download_verified(&release_at(&base, "/exe", "/sha"), &[], true, &mut progress)
            .await
            .unwrap();
        assert_eq!(data, b"new exe");
        let data = download_verified(
            &release_at(&base, "/exe-nolen", "/sha"),
            &[],
            true,
            &mut progress,
        )
        .await
        .unwrap();
        assert_eq!(data, b"new exe");

        let err = download_verified(
            &release_at(&base, "/tampered", "/sha"),
            &[],
            true,
            &mut progress,
        )
        .await
        .unwrap_err();
        assert!(format!("{err:#}").contains("checksum mismatch"), "{err:#}");

        for sha in ["/sha-big", "/sha-big-nolen"] {
            let err = download_verified(&release_at(&base, "/exe", sha), &[], true, &mut progress)
                .await
                .unwrap_err();
            assert!(format!("{err:#}").contains("too large"), "{err:#}");
        }

        // Local http without the dev override is refused before any request.
        let err = download_verified(
            &release_at(&base, "/exe", "/sha"),
            &[],
            false,
            &mut progress,
        )
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

    // ------------------------------------------------------------ signatures
    // Fixtures: tests/fixtures/update-signing (TEST-ONLY keys, see regen.sh).

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../tests/fixtures/update-signing/", $name))
        };
    }
    const PAYLOAD: &str = fixture!("payload.bin");
    const SIG_PRIMARY: &str = fixture!("payload.bin.minisig");
    const SIG_BACKUP: &str = fixture!("payload.bin.backup.minisig");
    const SIG_OTHER_KEY: &str = fixture!("payload.bin.other-key.minisig");
    const SIG_WRONG_VERSION: &str = fixture!("payload.bin.wrong-version.minisig");
    const SIG_WRONG_NAME: &str = fixture!("payload.bin.wrong-name.minisig");
    const SIG_LEGACY: &str = fixture!("payload.bin.legacy.minisig");
    const TEST_BACKUP_PUB: &str = fixture!("test-backup.pub");
    const TEST_OTHER_PUB: &str = fixture!("test-other.pub");
    /// The version the fixtures were signed for (`local-ai-chat v9.9.9`).
    const SIGNED_VERSION: &str = "9.9.9";

    /// Primary + backup test keys, as the production list will hold them.
    fn test_keys() -> Vec<&'static str> {
        vec![
            pub_key_line(TEST_PRIMARY_PUB),
            pub_key_line(TEST_BACKUP_PUB),
        ]
    }

    fn assert_sig_err(r: anyhow::Result<()>, needle: &str) {
        let e = r.expect_err("verification should fail");
        assert!(is_signature_error(&e), "not a SignatureError: {e:#}");
        assert!(
            format!("{e:#}").contains(needle),
            "{needle:?} not in: {e:#}"
        );
    }

    #[test]
    fn signature_valid_passes() {
        let keys = test_keys();
        verify_signature(PAYLOAD.as_bytes(), SIG_PRIMARY, SIGNED_VERSION, &keys).unwrap();
        // Only the primary key in the list is enough too.
        verify_signature(PAYLOAD.as_bytes(), SIG_PRIMARY, SIGNED_VERSION, &keys[..1]).unwrap();
        assert_eq!(expected_trusted_comment("2.3.0"), "local-ai-chat v2.3.0");
        // CRLF line endings / a BOM (file re-saved on Windows) still parse.
        let crlf = format!("\u{feff}{}", SIG_PRIMARY.replace('\n', "\r\n"));
        verify_signature(PAYLOAD.as_bytes(), &crlf, SIGNED_VERSION, &keys).unwrap();
    }

    #[test]
    fn signature_backup_key_passes() {
        let keys = test_keys();
        verify_signature(PAYLOAD.as_bytes(), SIG_BACKUP, SIGNED_VERSION, &keys).unwrap();
        // ...but not when the backup key is not trusted.
        assert_sig_err(
            verify_signature(PAYLOAD.as_bytes(), SIG_BACKUP, SIGNED_VERSION, &keys[..1]),
            "does not match a trusted key",
        );
    }

    #[test]
    fn signature_tampered_exe_fails() {
        let keys = test_keys();
        let mut tampered = PAYLOAD.as_bytes().to_vec();
        tampered[0] ^= 1;
        assert_sig_err(
            verify_signature(&tampered, SIG_PRIMARY, SIGNED_VERSION, &keys),
            "does not match a trusted key",
        );
        let mut longer = PAYLOAD.as_bytes().to_vec();
        longer.push(0);
        assert_sig_err(
            verify_signature(&longer, SIG_PRIMARY, SIGNED_VERSION, &keys),
            "does not match",
        );
    }

    #[test]
    fn signature_from_other_key_fails() {
        assert_sig_err(
            verify_signature(
                PAYLOAD.as_bytes(),
                SIG_OTHER_KEY,
                SIGNED_VERSION,
                &test_keys(),
            ),
            "does not match a trusted key",
        );
        // Trusting the other key makes it pass (the fixture itself is fine).
        let other = [pub_key_line(TEST_OTHER_PUB)];
        verify_signature(PAYLOAD.as_bytes(), SIG_OTHER_KEY, SIGNED_VERSION, &other).unwrap();
    }

    #[test]
    fn signature_wrong_trusted_comment_fails() {
        let keys = test_keys();
        // Signed as v9.9.8: replaying an older signed exe as v9.9.9 is refused.
        assert_sig_err(
            verify_signature(PAYLOAD.as_bytes(), SIG_WRONG_VERSION, SIGNED_VERSION, &keys),
            "not \"local-ai-chat v9.9.9\"",
        );
        verify_signature(PAYLOAD.as_bytes(), SIG_WRONG_VERSION, "9.9.8", &keys).unwrap();
        // Right version, wrong product name.
        assert_sig_err(
            verify_signature(PAYLOAD.as_bytes(), SIG_WRONG_NAME, SIGNED_VERSION, &keys),
            "lemon v9.9.9",
        );
        // A valid signature checked against another version.
        assert_sig_err(
            verify_signature(PAYLOAD.as_bytes(), SIG_PRIMARY, "9.9.10", &keys),
            "not \"local-ai-chat v9.9.10\"",
        );
        // An edited trusted comment breaks the global signature.
        let edited = SIG_WRONG_VERSION.replace("v9.9.8", "v9.9.9");
        assert_sig_err(
            verify_signature(PAYLOAD.as_bytes(), &edited, SIGNED_VERSION, &keys),
            "does not match",
        );
    }

    #[test]
    fn signature_missing_or_malformed_fails() {
        let keys = test_keys();
        for bad in [
            "",
            "garbage",
            "untrusted comment: x\nnot base64!!\ntrusted comment: local-ai-chat v9.9.9\nAAAA\n",
            &SIG_PRIMARY.replace("trusted comment: ", "comment: "),
            &SIG_PRIMARY.lines().take(2).collect::<Vec<_>>().join("\n"),
        ] {
            assert_sig_err(
                verify_signature(PAYLOAD.as_bytes(), bad, SIGNED_VERSION, &keys),
                "not valid",
            );
        }
        // Legacy (non-pre-hashed) signatures are not accepted.
        assert_sig_err(
            verify_signature(PAYLOAD.as_bytes(), SIG_LEGACY, SIGNED_VERSION, &keys),
            "does not match a trusted key",
        );
        // No trusted keys at all, or only malformed ones: nothing verifies.
        assert_sig_err(
            verify_signature(PAYLOAD.as_bytes(), SIG_PRIMARY, SIGNED_VERSION, &[]),
            "does not match a trusted key",
        );
        assert_sig_err(
            verify_signature(PAYLOAD.as_bytes(), SIG_PRIMARY, SIGNED_VERSION, &["RWnope"]),
            "does not match a trusted key",
        );
    }

    #[test]
    fn production_keys_never_contain_test_keys() {
        let test = [
            pub_key_line(TEST_PRIMARY_PUB),
            pub_key_line(TEST_BACKUP_PUB),
            pub_key_line(TEST_OTHER_PUB),
        ];
        for t in test {
            assert!(
                t.starts_with("RW") && t.len() == 56,
                "bad fixture key {t:?}"
            );
            assert!(
                !TRUSTED_PUBLIC_KEYS.iter().any(|k| k.trim() == t),
                "a TEST key is in update::TRUSTED_PUBLIC_KEYS"
            );
        }
        // Every production key must parse (a typo would silently disable it).
        for k in TRUSTED_PUBLIC_KEYS {
            assert!(
                minisign_verify::PublicKey::from_base64(k.trim()).is_ok(),
                "TRUSTED_PUBLIC_KEYS entry {k:?} is not a minisign public key"
            );
        }
        #[cfg(not(feature = "dev-test-update-key"))]
        assert_eq!(trusted_keys(), TRUSTED_PUBLIC_KEYS);
    }

    /// The release workflow signs only when `keys/minisign-{primary,backup}.pub`
    /// are committed. So: every committed key must be trusted by the app, and a
    /// non-empty `TRUSTED_PUBLIC_KEYS` needs committed keys, or CI would publish
    /// an unsigned release that these builds refuse. (Both empty = signing off.)
    #[test]
    fn keys_dir_matches_trusted_keys() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("keys");
        let committed: Vec<String> = ["primary", "backup"]
            .iter()
            .filter_map(|k| std::fs::read_to_string(dir.join(format!("minisign-{k}.pub"))).ok())
            .map(|f| pub_key_line(&f).to_string())
            .collect();
        for k in &committed {
            assert!(
                TRUSTED_PUBLIC_KEYS.iter().any(|t| t.trim() == k),
                "keys/*.pub key {k:?} is not in update::TRUSTED_PUBLIC_KEYS"
            );
        }
        assert_eq!(
            committed.is_empty(),
            TRUSTED_PUBLIC_KEYS.is_empty(),
            "TRUSTED_PUBLIC_KEYS and keys/minisign-{{primary,backup}}.pub must be added together \
             (docs/specs/signing.md)"
        );
    }

    /// With an empty key list (the shipped state until the owner has keys) a
    /// release is installed on its sha256 alone: a `.minisig` is neither
    /// required nor downloaded, even a bad one, and a bad sha256 still fails.
    #[tokio::test]
    async fn empty_key_list_checks_sha256_only() {
        let data = b"new exe bytes".to_vec();
        let sha = format!("{}  local-ai-chat.exe\n", to_hex(&Sha256::digest(&data)));
        let base = file_server(vec![
            ("/exe", data.clone(), false),
            ("/sha", sha.into_bytes(), false),
            ("/bad.minisig", b"not a signature".to_vec(), false),
            (
                "/badsha",
                format!("{}  x\n", "0".repeat(64)).into_bytes(),
                false,
            ),
        ]);
        let mut p = |_| {};
        let mut r = release_at(&base, "/exe", "/sha");
        assert!(r.sig_urls.is_empty());
        assert_eq!(
            download_verified(&r, &[], true, &mut p).await.unwrap(),
            data
        );
        r.sig_urls = vec![format!("{base}/bad.minisig")];
        assert_eq!(
            download_verified(&r, &[], true, &mut p).await.unwrap(),
            data
        );
        let r = release_at(&base, "/exe", "/badsha");
        assert!(download_verified(&r, &[], true, &mut p).await.is_err());
    }

    #[tokio::test]
    async fn download_requires_valid_signature_when_keys_set() {
        let payload = PAYLOAD.as_bytes().to_vec();
        let sha = format!("{}  local-ai-chat.exe\n", to_hex(&Sha256::digest(&payload)));
        let base = file_server(vec![
            ("/exe", payload.clone(), true),
            ("/sha", sha.into_bytes(), true),
            ("/sig", SIG_PRIMARY.as_bytes().to_vec(), true),
            ("/sig-other", SIG_OTHER_KEY.as_bytes().to_vec(), true),
            ("/sig-big", vec![b'a'; MAX_SIG_BYTES as usize + 1], false),
        ]);
        let keys = test_keys();
        let mut progress = |_: Progress| {};
        let with_sig = |sig: Option<&str>| {
            let mut r = release_at(&base, "/exe", "/sha");
            r.sig_urls = sig.map(|s| format!("{base}{s}")).into_iter().collect();
            r
        };

        let data = download_verified(&with_sig(Some("/sig")), &keys, true, &mut progress)
            .await
            .unwrap();
        assert_eq!(data, payload);

        // Empty key list (today's production build): sha256 only, signature ignored.
        let data = download_verified(&with_sig(None), &[], true, &mut progress)
            .await
            .unwrap();
        assert_eq!(data, payload);

        for (sig, needle) in [
            (None, "has no local-ai-chat.exe.minisig"),
            (Some("/missing"), "could not download the signature"),
            (Some("/sig-other"), "does not match a trusted key"),
            (Some("/sig-big"), "too large"),
        ] {
            let e = download_verified(&with_sig(sig), &keys, true, &mut progress)
                .await
                .unwrap_err();
            assert!(is_signature_error(&e), "{sig:?}: {e:#}");
            assert!(format!("{e:#}").contains(needle), "{sig:?}: {e:#}");
            assert!(format!("{e}").contains("https://fastkiwi.app/"), "{e}");
        }

        // A wrong version in the release (tag) vs. the signed comment.
        let mut r = with_sig(Some("/sig"));
        r.version = "9.9.10".into();
        let e = download_verified(&r, &keys, true, &mut progress)
            .await
            .unwrap_err();
        assert!(is_signature_error(&e), "{e:#}");

        // A signature URL over plain http (no dev override) is refused too.
        let e = download_verified(&with_sig(Some("/sig")), &keys, false, &mut progress)
            .await
            .unwrap_err();
        assert!(format!("{e:#}").contains("insecure"), "{e:#}");
    }

    /// CI dry run (`.github/workflows/release.yml`, `workflow_dispatch`): verify a
    /// file signed in the job with a throwaway key, using this module's verifier.
    /// Run with `cargo test ci_dry_run_signature -- --ignored` and the env vars below.
    #[test]
    #[ignore = "CI dry run: needs LEMON_DRYRUN_{FILE,SIG,PUB,VERSION}"]
    fn ci_dry_run_signature() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} is not set"));
        let data = std::fs::read(var("LEMON_DRYRUN_FILE")).unwrap();
        let sig = std::fs::read_to_string(var("LEMON_DRYRUN_SIG")).unwrap();
        let pub_file = std::fs::read_to_string(var("LEMON_DRYRUN_PUB")).unwrap();
        let version = var("LEMON_DRYRUN_VERSION");
        let key = [pub_key_line(&pub_file)];
        verify_signature(&data, &sig, &version, &key).expect("dry-run signature must verify");
        // The same signature must be refused for another version, a tampered
        // file, or when only the production keys are trusted.
        assert!(verify_signature(&data, &sig, &format!("{version}-x"), &key).is_err());
        let mut tampered = data.clone();
        tampered[0] ^= 1;
        assert!(verify_signature(&tampered, &sig, &version, &key).is_err());
        assert!(verify_signature(&data, &sig, &version, TRUSTED_PUBLIC_KEYS).is_err());
        println!("dry run: signature verified for local-ai-chat v{version}");
    }

    // ------------------------------------------------------------ manifest
    // Fixtures: tests/fixtures/update-signing/manifest.json*, a fake v9.9.9 whose
    // exe is payload.bin, made by tools/make-update-manifest and signed with the
    // same TEST-ONLY keys (regen.sh).

    const MANIFEST: &str = fixture!("manifest.json");
    const MSIG_PRIMARY: &str = fixture!("manifest.json.minisig");
    const MSIG_BACKUP: &str = fixture!("manifest.json.backup.minisig");
    const MSIG_OTHER_KEY: &str = fixture!("manifest.json.other-key.minisig");
    const MSIG_WRONG_VERSION: &str = fixture!("manifest.json.wrong-version.minisig");
    const MSIG_EXE_COMMENT: &str = fixture!("manifest.json.exe-comment.minisig");
    const MSIG_LEGACY: &str = fixture!("manifest.json.legacy.minisig");
    const GITLAB_EXE: &str =
        "https://gitlab.com/fastkiwi1/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe";
    const GITHUB_EXE: &str = "https://github.com/sarmadiq9090-coder/local-ai-chat/releases/download/v9.9.9/local-ai-chat.exe";

    fn payload_sha() -> [u8; 32] {
        Sha256::digest(PAYLOAD.as_bytes()).into()
    }

    /// A GitHub release `tag` whose `.sha256` asset is `{files}{sha_path}`.
    fn gh_json(tag: &str, files: &str, sha_path: &str) -> Vec<u8> {
        format!(
            r#"{{"tag_name": "{tag}", "body": "notes", "draft": false, "assets": [
              {{"name": "local-ai-chat.exe", "browser_download_url": "{files}/gh.exe"}},
              {{"name": "local-ai-chat.exe.sha256", "browser_download_url": "{files}{sha_path}"}}]}}"#
        )
        .into_bytes()
    }

    fn is_manifest_release(r: &ReleaseInfo) -> bool {
        r.asset_urls == [GITHUB_EXE, GITLAB_EXE] && r.sha_url.is_none()
    }

    #[test]
    fn manifest_fixture_parses() {
        let r = parse_manifest(MANIFEST.as_bytes(), false).unwrap();
        assert_eq!(r.version, "9.9.9");
        assert!(is_manifest_release(&r), "{r:?}");
        assert_eq!(r.sha256, Some(payload_sha()));
        assert_eq!(
            r.sig_urls,
            [
                format!("{GITHUB_EXE}.minisig"),
                format!("{GITLAB_EXE}.minisig")
            ]
        );
        assert_eq!(r.published_at.as_deref(), Some("2026-11-13T06:00:00Z"));
        assert_eq!(
            expected_manifest_comment("9.9.9"),
            "local-ai-chat update-manifest v9.9.9"
        );
        assert_ne!(
            expected_manifest_comment("9.9.9"),
            expected_trusted_comment("9.9.9")
        );
    }

    #[test]
    fn hostile_manifests_are_refused() {
        let m = |from: &str, to: &str| MANIFEST.replacen(from, to, 1);
        let hex = to_hex(&payload_sha());
        for (what, bad) in [
            (
                "other app",
                m("\"app\": \"local-ai-chat\"", "\"app\": \"instafind\""),
            ),
            ("schema", m("\"schema\": 1", "\"schema\": 2")),
            ("tag", m("\"tag\": \"v9.9.9\"", "\"tag\": \"v9.9.8\"")),
            (
                "pre-release",
                m("\"version\": \"9.9.9\"", "\"version\": \"9.9.9-rc.1\""),
            ),
            (
                "leading zero",
                m("\"version\": \"9.9.9\"", "\"version\": \"09.9.9\""),
            ),
            ("size 0", m("\"size\": 54", "\"size\": 0")),
            ("huge", m("\"size\": 54", "\"size\": 999999999999")),
            (
                "upper-case sha",
                MANIFEST.replacen(&hex, &hex.to_uppercase(), 1),
            ),
            ("short sha", MANIFEST.replacen(&hex, "abc", 1)),
            (
                "other name",
                m("\"name\": \"local-ai-chat.exe\"", "\"name\": \"lemon.exe\""),
            ),
            ("other arch", m("\"x64\"", "\"arm64\"")),
            (
                "no usable url",
                m(
                    GITLAB_EXE,
                    "http://gitlab.com/fastkiwi1/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe",
                )
                .replacen(GITHUB_EXE, "https://github.com/x/other.exe", 1),
            ),
            ("not json", "nope".into()),
            (
                "too big",
                format!("{MANIFEST}{}", " ".repeat(MAX_MANIFEST_BYTES as usize)),
            ),
        ] {
            assert!(parse_manifest(bad.as_bytes(), false).is_err(), "{what}");
        }
        assert!(MANIFEST.contains("\"size\": 54"), "fixture size changed");
        // Bad addresses are dropped, the good one stays.
        for evil in [
            "http://gitlab.com/fastkiwi1/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe",
            "https://gitlab.com/fastkiwi1/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe?x=1",
            "https://gitlab.com/fastkiwi1/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe.zip",
            "https://gitlab.com/evil/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe",
            "https://gitlab.com/fastkiwi1/local-ai-chat/-/releases/../../../../evil/x/-/raw/local-ai-chat.exe",
            "https://gitlab.com/fastkiwi1/local-ai-chat/-/releases/v9.9.9/%2e%2e/local-ai-chat.exe",
            "https://gitlab.com:8443/fastkiwi1/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe",
            "https://u@gitlab.com/fastkiwi1/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe",
            "https://gitlab.com.evil.example/fastkiwi1/local-ai-chat/-/releases/v9.9.9/downloads/local-ai-chat.exe",
            "https://updates.fastkiwi.app/local-ai-chat/v9.9.9/local-ai-chat.exe",
            "https://evil.example/local-ai-chat.exe",
            "http://127.0.0.1:9/local-ai-chat.exe",
        ] {
            let r = parse_manifest(m(GITLAB_EXE, evil).as_bytes(), false).unwrap();
            assert_eq!(r.asset_urls, [GITHUB_EXE], "{evil}");
        }
        // Local http only with the dev override.
        let local = m(GITLAB_EXE, "http://127.0.0.1:9/local-ai-chat.exe");
        assert_eq!(
            parse_manifest(local.as_bytes(), true)
                .unwrap()
                .asset_urls
                .len(),
            2
        );
    }

    #[test]
    fn manifest_signatures() {
        let keys = test_keys();
        let data = MANIFEST.as_bytes();
        let want = expected_manifest_comment("9.9.9");
        verify_minisign(data, MSIG_PRIMARY, &want, &keys).unwrap();
        verify_minisign(data, MSIG_BACKUP, &want, &keys).unwrap();
        for (sig, needle) in [
            (MSIG_OTHER_KEY, "does not match a trusted key"),
            (MSIG_WRONG_VERSION, "local-ai-chat update-manifest v9.9.8"),
            (MSIG_EXE_COMMENT, "\"local-ai-chat v9.9.9\""),
            (MSIG_LEGACY, "does not match a trusted key"),
        ] {
            assert_sig_err(verify_minisign(data, sig, &want, &keys), needle);
        }
        let changed = MANIFEST.replacen("9.9.9", "9.9.10", 1);
        assert_sig_err(
            verify_minisign(changed.as_bytes(), MSIG_PRIMARY, &want, &keys),
            "trusted key",
        );
        assert_sig_err(
            verify_minisign(data, MSIG_PRIMARY, &want, &[]),
            "trusted key",
        );
        // The exe's signature never verifies the manifest.
        assert!(verify_minisign(data, SIG_PRIMARY, &want, &keys).is_err());
    }

    #[tokio::test]
    async fn resolve_uses_the_signed_manifest_and_falls_back_to_github() {
        let sha_file = format!("{}  local-ai-chat.exe\n", to_hex(&payload_sha()));
        let files = file_server(vec![
            ("/sha", sha_file.into_bytes(), true),
            (
                "/badsha",
                format!("{}  x\n", "0".repeat(64)).into_bytes(),
                true,
            ),
        ]);
        let site = file_server(vec![
            ("/ok/latest.json", MANIFEST.into(), true),
            ("/ok/latest.json.minisig", MSIG_PRIMARY.into(), true),
            ("/backup/latest.json", MANIFEST.into(), true),
            ("/backup/latest.json.minisig", MSIG_BACKUP.into(), true),
            ("/other/latest.json", MANIFEST.into(), true),
            ("/other/latest.json.minisig", MSIG_OTHER_KEY.into(), true),
            ("/replay/latest.json", MANIFEST.into(), true),
            (
                "/replay/latest.json.minisig",
                MSIG_WRONG_VERSION.into(),
                true,
            ),
            ("/nosig/latest.json", MANIFEST.into(), true),
            ("/html/latest.json", b"<html>".to_vec(), true),
        ]);
        let gh = file_server(vec![
            ("/latest", gh_json("v9.9.9", &files, "/sha"), true),
            ("/newer", gh_json("v9.9.10", &files, "/sha"), true),
            ("/other-sha", gh_json("v9.9.9", &files, "/badsha"), true),
        ]);
        let (gh_ok, gh_newer, gh_other) = (
            format!("{gh}/latest"),
            format!("{gh}/newer"),
            format!("{gh}/other-sha"),
        );
        let gh_404 = format!("{gh}/missing");
        let down = "http://127.0.0.1:1/latest";
        let url = |p: &str| format!("{site}{p}/latest.json");
        let keys = test_keys();

        // The signed manifest works while GitHub is down (404, or not answering).
        for feed in [down, gh_404.as_str()] {
            for p in ["/ok", "/backup"] {
                let r = resolve(Some(&url(p)), feed, &keys, "9.9.8", true)
                    .await
                    .unwrap();
                assert!(is_manifest_release(&r), "{p} {feed}: {r:?}");
            }
        }
        // A missing, broken or wrongly signed manifest: GitHub's release.
        for p in ["/missing", "/other", "/replay", "/nosig", "/html"] {
            let r = resolve(Some(&url(p)), &gh_ok, &keys, "9.9.8", true)
                .await
                .unwrap();
            assert!(r.sha_url.is_some() && r.version == "9.9.9", "{p}: {r:?}");
        }
        let r = resolve(
            Some("http://127.0.0.1:1/latest.json"),
            &gh_ok,
            &keys,
            "9.9.8",
            true,
        )
        .await
        .unwrap();
        assert!(r.sha_url.is_some());
        // Both down: GitHub's neutral "can't be reached" error.
        let e = resolve(Some(&url("/missing")), &gh_404, &keys, "9.9.8", true)
            .await
            .unwrap_err();
        assert!(is_server_unreachable(&e), "{e:#}");
        // Up to date or newer than the manifest: reported, never offered (no downgrade).
        for current in ["9.9.9", "10.0.0"] {
            let r = resolve(Some(&url("/ok")), down, &keys, current, true)
                .await
                .unwrap();
            assert!(
                is_manifest_release(&r) && !is_newer(&r.version, current),
                "{current}"
            );
        }
        // A stale manifest: GitHub's newer release wins.
        let r = resolve(Some(&url("/ok")), &gh_newer, &keys, "9.9.9", true)
            .await
            .unwrap();
        assert_eq!(r.version, "9.9.10");
        // No manifest URL (dev feed override only): GitHub.
        let r = resolve(None, &gh_ok, &keys, "9.9.8", true).await.unwrap();
        assert!(r.sha_url.is_some());

        // No keys (today): the manifest only when GitHub has the same exe.
        let r = resolve(Some(&url("/other")), &gh_ok, &[], "9.9.8", true)
            .await
            .unwrap();
        assert!(
            is_manifest_release(&r),
            "the signature is not even checked: {r:?}"
        );
        for feed in [&gh_other, &gh_newer] {
            let r = resolve(Some(&url("/ok")), feed, &[], "9.9.8", true)
                .await
                .unwrap();
            assert!(r.sha_url.is_some(), "{feed}: {r:?}");
        }
        let e = resolve(Some(&url("/ok")), &gh_404, &[], "9.9.8", true)
            .await
            .unwrap_err();
        assert!(
            is_server_unreachable(&e),
            "nothing confirms the manifest: {e:#}"
        );
    }

    #[tokio::test]
    async fn download_tries_each_address_in_order() {
        let payload = PAYLOAD.as_bytes().to_vec();
        let base = file_server(vec![
            ("/broken", b"evil exe".to_vec(), true),
            ("/exe", payload.clone(), true),
            ("/sig", SIG_PRIMARY.as_bytes().to_vec(), true),
            ("/badsig", SIG_OTHER_KEY.as_bytes().to_vec(), true),
        ]);
        let at = |paths: &[&str]| {
            paths
                .iter()
                .map(|p| format!("{base}{p}"))
                .collect::<Vec<_>>()
        };
        let release = |exes: &[&str], sigs: &[&str]| ReleaseInfo {
            version: SIGNED_VERSION.into(),
            notes: String::new(),
            published_at: None,
            asset_urls: at(exes),
            sha_url: None,
            sha256: Some(payload_sha()),
            sig_urls: at(sigs),
        };
        let keys = test_keys();
        let mut p = |_: Progress| {};
        // Missing, then altered, then good; the same for the signature.
        let r = release(
            &["/missing", "/broken", "/exe"],
            &["/missing.minisig", "/badsig", "/sig"],
        );
        assert_eq!(
            download_verified(&r, &keys, true, &mut p).await.unwrap(),
            payload
        );
        assert_eq!(
            download_verified(&r, &[], true, &mut p).await.unwrap(),
            payload
        );
        // No good copy: the last address's error.
        let r = release(&["/missing", "/broken"], &["/sig"]);
        let e = download_verified(&r, &keys, true, &mut p)
            .await
            .unwrap_err();
        assert!(format!("{e:#}").contains("checksum mismatch"), "{e:#}");
        // No good signature: refused.
        let r = release(&["/exe"], &["/missing.minisig", "/badsig"]);
        let e = download_verified(&r, &keys, true, &mut p)
            .await
            .unwrap_err();
        assert!(is_signature_error(&e), "{e:#}");
        let r = release(&["/exe"], &[]);
        assert!(is_signature_error(
            &download_verified(&r, &keys, true, &mut p)
                .await
                .unwrap_err()
        ));
        // Plain http without the dev override: refused before downloading.
        let r = release(&["/exe"], &["/sig"]);
        let e = download_verified(&r, &keys, false, &mut p)
            .await
            .unwrap_err();
        assert!(format!("{e:#}").contains("insecure"), "{e:#}");
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
