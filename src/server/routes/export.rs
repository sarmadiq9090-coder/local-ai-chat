//! Export a conversation as Markdown into `Documents\LocalAiChat\`. See `docs/API.md`.

use axum::{Json, Router, extract::State, routing::post};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::write_atomic;
use crate::error::ApiResult;
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/export", post(export))
}

#[derive(Deserialize)]
struct ExportReq {
    filename: String,
    content: String,
}

async fn export(State(s): State<AppState>, Json(req): Json<ExportReq>) -> ApiResult<Json<Value>> {
    let name = sanitize_filename(&req.filename);
    std::fs::create_dir_all(&s.paths.export_dir)?;
    let path = s.paths.export_dir.join(name);
    write_atomic(&path, req.content.as_bytes())?;
    reveal_in_explorer(&path);
    Ok(Json(json!({ "path": path })))
}

/// Keep letters (incl. Arabic), digits, spaces, `-_.`; ensure a `.md` extension.
fn sanitize_filename(raw: &str) -> String {
    let mut name: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .chars()
        .take(80)
        .collect();
    if name.is_empty() {
        name = "conversation".into();
    }
    if !name.to_lowercase().ends_with(".md") {
        name.push_str(".md");
    }
    name
}

fn reveal_in_explorer(path: &std::path::Path) {
    let mut cmd = std::process::Command::new("explorer");
    // explorer doesn't understand a fully quoted `"/select,..."` argument, so
    // pass it raw with only the path quoted (paths often contain spaces).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.raw_arg(format!("/select,\"{}\"", path.display()));
    }
    #[cfg(not(windows))]
    cmd.arg(path);
    let _ = cmd.spawn();
}
