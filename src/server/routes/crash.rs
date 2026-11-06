//! Crash notice: was `crash.txt` (see `crash_log`) written since the UI last
//! showed it, and "open the folder". See `docs/API.md`.

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde_json::{Value, json};

use crate::crash_log;
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/crash", get(check))
        .route("/crash/seen", post(seen))
        .route("/crash/open", post(open))
}

async fn check(State(s): State<AppState>) -> Json<Value> {
    let dir = &s.paths.data_dir;
    Json(json!({ "new": crash_log::unseen(dir), "path": crash_log::crash_file(dir) }))
}

async fn seen(State(s): State<AppState>) -> Json<Value> {
    Json(json!({ "ok": crash_log::mark_seen(&s.paths.data_dir).is_ok() }))
}

/// Opens Explorer with `crash.txt` selected.
async fn open(State(s): State<AppState>) -> Json<Value> {
    let mut cmd = std::process::Command::new("explorer");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.raw_arg(format!(
            "/select,\"{}\"",
            crash_log::crash_file(&s.paths.data_dir).display()
        ));
    }
    #[cfg(not(windows))]
    cmd.arg(&s.paths.data_dir);
    Json(json!({ "ok": cmd.spawn().is_ok() }))
}
