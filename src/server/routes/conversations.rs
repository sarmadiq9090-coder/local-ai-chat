//! App endpoints: status, system, settings, conversations, export. See `docs/API.md`.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use crate::config::{Settings, write_atomic};
use crate::error::{ApiError, ApiResult};
use crate::storage::{ConversationMeta, ConversationStore};
use crate::system::SystemInfo;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/ollama/start", post(start_ollama))
        .route("/system", get(system))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/conversations", get(list_conversations))
        .route(
            "/conversations/{id}",
            get(get_conversation).put(put_conversation).delete(delete_conversation),
        )
        .route("/export", post(export))
}

// ---------- status ----------

async fn status(State(s): State<AppState>) -> Json<Value> {
    let version = s.ollama.version(&s.ollama_url().await).await;
    Json(json!({
        "ollama": version.is_some(),
        "version": version,
        "app_version": env!("CARGO_PKG_VERSION"),
        "data_dir": s.paths.data_dir,
    }))
}

async fn start_ollama(State(s): State<AppState>) -> Json<Value> {
    let ok = s.ollama.start(&s.ollama_url().await).await;
    Json(json!({ "ok": ok }))
}

// ---------- system ----------

async fn system(State(s): State<AppState>) -> Json<SystemInfo> {
    Json(s.system_info().await)
}

// ---------- settings ----------

async fn get_settings(State(s): State<AppState>) -> Json<Settings> {
    Json(s.settings.read().await.clone())
}

async fn put_settings(State(s): State<AppState>, Json(new): Json<Settings>) -> ApiResult<Json<Settings>> {
    let new = new.sanitized();
    // Hold the lock across save + assign so disk and memory never disagree.
    let mut current = s.settings.write().await;
    new.save(&s.paths.settings_file)?;
    *current = new.clone();
    Ok(Json(new))
}

// ---------- conversations ----------

fn check_id(id: &str) -> ApiResult<()> {
    if ConversationStore::valid_id(id) { Ok(()) } else { Err(ApiError::bad_request("invalid id")) }
}

async fn list_conversations(State(s): State<AppState>) -> ApiResult<Json<Vec<ConversationMeta>>> {
    Ok(Json(s.store.list()?))
}

async fn get_conversation(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    s.store.get(&id)?.map(Json).ok_or_else(|| ApiError::not_found("conversation not found"))
}

async fn put_conversation(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(conv): Json<Value>,
) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    if !conv.is_object() {
        return Err(ApiError::bad_request("conversation must be an object"));
    }
    s.store.put(&id, conv)?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_conversation(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    s.store.delete(&id)?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- export ----------

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
        .map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' })
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
