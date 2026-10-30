//! Backend/Ollama status and "start Ollama". See `docs/API.md`.

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde_json::{Value, json};

use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/ollama/start", post(start_ollama))
}

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
