//! "More from FastKiwi": the other FastKiwi apps (`crate::more_from`, a vendored
//! copy of the site's apps.json compiled into the exe). Shown in Settings by
//! `ui/features/morefrom`. See `docs/API.md`.

use axum::{Json, Router, routing::get};

use crate::more_from::{self, App};
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/more-from", get(list))
}

async fn list() -> Json<Vec<App>> {
    Json(more_from::apps())
}
