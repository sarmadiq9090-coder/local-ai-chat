//! Conversations: list, get, create/replace, delete. See `docs/API.md`.
//!
//! Conversations are stored as opaque JSON objects, so unknown fields added by
//! the UI are preserved without backend changes.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult};
use crate::server::AppState;
use crate::storage::{ConversationMeta, ConversationStore};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/conversations", get(list_conversations))
        .route(
            "/conversations/{id}",
            get(get_conversation)
                .put(put_conversation)
                .delete(delete_conversation),
        )
}

fn check_id(id: &str) -> ApiResult<()> {
    if ConversationStore::valid_id(id) {
        Ok(())
    } else {
        Err(ApiError::bad_request("invalid id"))
    }
}

async fn list_conversations(State(s): State<AppState>) -> ApiResult<Json<Vec<ConversationMeta>>> {
    Ok(Json(s.store.list()?))
}

async fn get_conversation(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    s.store
        .get(&id)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("conversation not found"))
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

async fn delete_conversation(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    s.store.delete(&id)?;
    Ok(Json(json!({ "ok": true })))
}
