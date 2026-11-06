//! App endpoints, mounted under `/api/app` (see `docs/API.md`).
//!
//! One file per backend feature. Each exposes `pub fn router() -> Router<AppState>`
//! with routes relative to `/api/app`. To add a feature: create `routes/<name>.rs`,
//! then add one `mod` line and one `.merge(...)` line below.

use axum::Router;

use super::AppState;

mod assistants;
mod conversations;
mod crash;
mod export;
mod notices;
mod settings;
mod status;
mod system;
mod update;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(status::router())
        .merge(settings::router())
        .merge(conversations::router())
        .merge(export::router())
        .merge(system::router())
        .merge(assistants::router())
        .merge(update::router())
        .merge(crash::router())
        .merge(notices::router())
}
