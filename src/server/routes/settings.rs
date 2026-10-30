//! User settings: read and save. See `docs/API.md`.

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::Value;

use crate::config::Settings;
use crate::error::{ApiError, ApiResult};
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/settings", get(get_settings).put(put_settings))
}

async fn get_settings(State(s): State<AppState>) -> Json<Settings> {
    Json(s.settings.read().await.clone())
}

/// Accepts a full or partial settings object: given fields replace the current
/// values, missing ones are kept.
async fn put_settings(
    State(s): State<AppState>,
    Json(patch): Json<Value>,
) -> ApiResult<Json<Settings>> {
    if !patch.is_object() {
        return Err(ApiError::bad_request("settings must be a JSON object"));
    }
    // Hold the lock across merge + save + assign so disk and memory never disagree
    // and concurrent partial updates don't lose each other's fields.
    let mut current = s.settings.write().await;
    let new = current
        .merged(patch)
        .map_err(|e| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")))?;
    new.validate()
        .map_err(|m| ApiError::bad_request(m).with_code("invalid_ollama_url"))?;
    let new = new.sanitized();
    new.save(&s.paths.settings_file)?;
    *current = new.clone();
    Ok(Json(new))
}
