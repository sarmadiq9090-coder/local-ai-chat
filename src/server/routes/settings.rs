//! User settings: read and save. See `docs/API.md`.

use axum::{Json, Router, extract::State, routing::get};

use crate::config::Settings;
use crate::error::ApiResult;
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/settings", get(get_settings).put(put_settings))
}

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
