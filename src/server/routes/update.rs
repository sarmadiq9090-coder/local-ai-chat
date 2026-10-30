//! Auto-update: check GitHub for a newer release and install it. See `docs/API.md`
//! and `src/update.rs`. Network problems never produce a 5xx: `check` returns
//! `available: false` + `error`, `install` streams `{"error": ...}`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::Query,
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;

use crate::server::AppState;
use crate::update::{self, CURRENT_VERSION, Progress, ReleaseInfo};

const CACHE_TTL: Duration = Duration::from_secs(10 * 60);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/update/check", get(check))
        .route("/update/install", post(install))
}

#[derive(Clone, Serialize)]
struct CheckResponse {
    current: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<String>,
    available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    just_updated: Option<String>,
}

/// Last check result: (when, response, newer release if any).
static CACHE: Mutex<Option<(Instant, CheckResponse, Option<ReleaseInfo>)>> = Mutex::new(None);
static INSTALLING: AtomicBool = AtomicBool::new(false);

fn empty_response() -> CheckResponse {
    CheckResponse {
        current: CURRENT_VERSION,
        latest: None,
        available: false,
        notes: None,
        published_at: None,
        error: None,
        just_updated: None,
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CheckQuery {
    /// `1`: ignore the 10-minute cache.
    force: Option<String>,
    /// `1`: no network; return the cached result (or just `current`) + `just_updated`.
    local: Option<String>,
}

fn flag(v: &Option<String>) -> bool {
    v.as_deref().is_some_and(|s| s != "0" && s != "false")
}

/// Fresh (or cached) check. Returns the response and the installable release.
async fn run_check(force: bool) -> (CheckResponse, Option<ReleaseInfo>) {
    if !force {
        if let Some((at, resp, rel)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            if at.elapsed() < CACHE_TTL {
                return (resp, rel);
            }
        }
    }
    let mut resp = empty_response();
    let mut release = None;
    match update::fetch_latest().await {
        Ok(Some(r)) => {
            resp.available = update::is_newer(&r.version, CURRENT_VERSION);
            resp.latest = Some(r.version.clone());
            resp.notes = Some(r.notes.clone());
            resp.published_at = r.published_at.clone();
            release = resp.available.then_some(r);
        }
        Ok(None) => {} // no release published (or repo not found)
        Err(e) => resp.error = Some(format!("{e:#}")),
    }
    // Errors are cached too (avoids hammering GitHub), but a forced check retries.
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((Instant::now(), resp.clone(), release.clone()));
    (resp, release)
}

async fn check(Query(q): Query<CheckQuery>) -> Json<CheckResponse> {
    let mut resp = if flag(&q.local) {
        CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|c| c.1.clone())
            .unwrap_or_else(empty_response)
    } else {
        run_check(flag(&q.force)).await.0
    };
    resp.just_updated = update::take_just_updated();
    Json(resp)
}

fn line(v: serde_json::Value) -> Bytes {
    let mut s = v.to_string();
    s.push('\n');
    Bytes::from(s)
}

async fn install() -> Response {
    let (tx, rx) = mpsc::unbounded_channel::<Bytes>();

    if INSTALLING.swap(true, Ordering::SeqCst) {
        let _ = tx.send(line(
            json!({ "error": "an update is already being installed" }),
        ));
    } else {
        tokio::spawn(async move {
            let release = match run_check(false).await {
                (_, Some(r)) => Some(r),
                (resp, None) if resp.error.is_some() => run_check(true).await.1,
                _ => None,
            };
            let result = match release {
                None => Err(anyhow::anyhow!("no update available")),
                Some(r) => {
                    let tx = tx.clone();
                    update::install(&r, move |p| {
                        let v = match p {
                            Progress::Downloading { downloaded, total } => {
                                json!({ "status": "downloading", "downloaded": downloaded, "total": total })
                            }
                            Progress::Verifying => json!({ "status": "verifying" }),
                            Progress::Restarting => json!({ "status": "restarting" }),
                        };
                        let _ = tx.send(line(v));
                    })
                    .await
                }
            };
            match result {
                Ok(()) => {
                    drop(tx); // end the stream so the UI sees "restarting"
                    tokio::time::sleep(Duration::from_millis(800)).await;
                    update::request_exit();
                }
                Err(e) => {
                    let _ = tx.send(line(json!({ "error": format!("{e:#}") })));
                    INSTALLING.store(false, Ordering::SeqCst);
                }
            }
        });
    }

    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|b| (Ok::<_, std::io::Error>(b), rx))
    });
    (
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}
