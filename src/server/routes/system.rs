//! Hardware summary for model suggestions (probed once, then cached). See `docs/API.md`.

use axum::{Json, Router, extract::State, routing::get};

use crate::server::AppState;
use crate::system::SystemInfo;

pub fn router() -> Router<AppState> {
    Router::new().route("/system", get(system))
}

async fn system(State(s): State<AppState>) -> Json<SystemInfo> {
    Json(s.system_info().await)
}
