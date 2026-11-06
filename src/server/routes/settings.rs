//! User settings: read and save. See `docs/API.md`.

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::Value;

use crate::config::Settings;
use crate::error::{ApiError, ApiResult};
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/settings", get(get_settings).put(put_settings))
}

/// The settings, plus `"language_unsaved": true` while no language is stored yet
/// (first run: `language` is the Windows display language).
async fn get_settings(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let current = s.settings.read().await.clone();
    let mut body = serde_json::to_value(&current)?;
    if current.language_unsaved {
        body["language_unsaved"] = Value::Bool(true);
    }
    Ok(Json(body))
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
    // Only a request that sets the URL is checked, so a bad stored URL never
    // blocks unrelated saves (e.g. picking a model).
    let sets_url = patch.get("ollama_url").is_some();
    // Hold the lock across merge + save + assign so disk and memory never disagree
    // and concurrent partial updates don't lose each other's fields.
    let mut current = s.settings.write().await;
    let new = current
        .merged(patch)
        .map_err(|e| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")))?;
    if sets_url {
        new.validate()
            .map_err(|m| ApiError::bad_request(m).with_code("invalid_ollama_url"))?;
    }
    let new = new.sanitized();
    new.save(&s.paths.settings_file)?;
    *current = new.clone();
    Ok(Json(new))
}
