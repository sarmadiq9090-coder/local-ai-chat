//! Custom assistants with a knowledge base (RAG). See `docs/API.md` and
//! `docs/specs/assistants.md`.
//!
//! Errors: unknown assistant/document → 404, missing embedding model → 424
//! `{error, missing_model}`, invalid input → 400. Document uploads and
//! re-indexing stream NDJSON progress lines (`{status, done, total}`) and end
//! with `{doc}` / `{assistant}` or `{error[, missing_model]}`.

use axum::{
    Json, Router,
    body::Body,
    extract::{FromRequest, Multipart, Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::assistants::{
    AssistantPatch, AssistantStore, DocInput, ModelMissing, NewAssistant, NotFound, Progress,
    extract,
};
use crate::error::{ApiError, ApiResult};
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/assistants", get(list).post(create))
        .route("/assistants/{id}", get(get_one).put(update).delete(remove))
        .route("/assistants/{id}/docs", post(add_docs))
        .route(
            "/assistants/{id}/docs/{doc_id}",
            get(doc_text).delete(remove_doc),
        )
        .route("/assistants/{id}/reindex", post(reindex))
        .route("/assistants/{id}/search", post(search))
}

/// Map core errors to HTTP: NotFound → 404, ModelMissing → 424, else 500.
fn map_err(e: anyhow::Error) -> ApiError {
    if let Some(nf) = e.downcast_ref::<NotFound>() {
        return ApiError::not_found(nf.to_string());
    }
    if e.downcast_ref::<ModelMissing>().is_some() {
        // Rendered with `missing_model` by `model_missing_response`.
        return ApiError::new(StatusCode::FAILED_DEPENDENCY, format!("{e:#}"));
    }
    ApiError::from(e)
}

/// 424 with `{error, missing_model}` so the UI can offer a pull.
fn model_missing_response(e: &anyhow::Error) -> Option<Response> {
    let mm = e.downcast_ref::<ModelMissing>()?;
    Some(
        (
            StatusCode::FAILED_DEPENDENCY,
            Json(json!({ "error": mm.to_string(), "missing_model": mm.0 })),
        )
            .into_response(),
    )
}

fn check_id(id: &str) -> ApiResult<()> {
    if AssistantStore::valid_id(id) {
        Ok(())
    } else {
        Err(ApiError::bad_request("invalid id"))
    }
}

async fn list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(s.assistants.list()?)?))
}

/// The assistant plus `index` (`IndexStatus`: chunk count, stale flag).
async fn get_one(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    let a = s
        .assistants
        .get(&id)?
        .ok_or_else(|| ApiError::not_found("assistant not found"))?;
    let status = s.assistants.index_status(&id).await.map_err(map_err)?;
    let mut v = serde_json::to_value(a)?;
    v["index"] = serde_json::to_value(status)?;
    Ok(Json(v))
}

async fn create(
    State(s): State<AppState>,
    Json(body): Json<NewAssistant>,
) -> ApiResult<Json<Value>> {
    if body.name.trim().is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }
    Ok(Json(serde_json::to_value(s.assistants.create(body)?)?))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<AssistantPatch>,
) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    if patch.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::bad_request("name is required"));
    }
    let a = s.assistants.update(&id, patch).await.map_err(map_err)?;
    Ok(Json(serde_json::to_value(a)?))
}

async fn remove(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    s.assistants.delete(&id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn doc_text(
    State(s): State<AppState>,
    Path((id, doc_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    check_id(&doc_id)?;
    let text = s
        .assistants
        .doc_text(&id, &doc_id)?
        .ok_or_else(|| ApiError::not_found("document not found"))?;
    Ok(Json(json!({ "text": text })))
}

async fn remove_doc(
    State(s): State<AppState>,
    Path((id, doc_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    s.assistants
        .remove_document(&id, &doc_id)
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "ok": true })))
}

// ---- NDJSON streaming -----------------------------------------------------

/// A response whose body is the NDJSON lines sent on the returned channel.
fn ndjson_channel() -> (mpsc::UnboundedSender<Value>, Response) {
    let (tx, rx) = mpsc::unbounded_channel::<Value>();
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        let v = rx.recv().await?;
        let mut line = serde_json::to_vec(&v).unwrap_or_default();
        line.push(b'\n');
        Some((Ok::<_, std::io::Error>(line), rx))
    });
    let resp = (
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(stream),
    )
        .into_response();
    (tx, resp)
}

fn progress_line(p: &Progress) -> Value {
    json!({ "status": p.status, "done": p.done, "total": p.total })
}

fn error_line(e: &anyhow::Error, name: Option<&str>) -> Value {
    let mut v = json!({ "error": format!("{e:#}") });
    if let Some(mm) = e.downcast_ref::<ModelMissing>() {
        v["missing_model"] = json!(mm.0);
    }
    if let Some(n) = name {
        v["name"] = json!(n);
    }
    v
}

#[derive(Deserialize)]
struct NoteBody {
    name: String,
    text: String,
}

/// Multipart file(s) (any field with a file name) or JSON `{name, text}` for a
/// note. Files are indexed one after another; each ends with `{doc}` or
/// `{error, name}`.
async fn add_docs(
    State(s): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> ApiResult<Response> {
    check_id(&id)?;
    if s.assistants.get(&id)?.is_none() {
        return Err(ApiError::not_found("assistant not found"));
    }
    let is_multipart = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("multipart/form-data"));

    let mut inputs: Vec<(String, DocInput)> = Vec::new();
    if is_multipart {
        let mut mp = Multipart::from_request(req, &s)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        while let Some(field) = mp
            .next_field()
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?
        {
            let Some(name) = field.file_name().map(str::to_string) else {
                continue;
            };
            if !extract::is_supported(&name) {
                return Err(ApiError::bad_request(format!(
                    "unsupported file type: {name}"
                )));
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(e.body_text()))?;
            inputs.push((name, DocInput::File(bytes.to_vec())));
        }
        if inputs.is_empty() {
            return Err(ApiError::bad_request("no files in the upload"));
        }
    } else {
        let Json(note) = Json::<NoteBody>::from_request(req, &s)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        if note.name.trim().is_empty() || note.text.trim().is_empty() {
            return Err(ApiError::bad_request("note name and text are required"));
        }
        inputs.push((note.name, DocInput::Note(note.text)));
    }

    let (tx, resp) = ndjson_channel();
    let url = s.ollama_url().await;
    tokio::spawn(async move {
        for (name, input) in inputs {
            let ptx = tx.clone();
            let res = s
                .assistants
                .add_document(&url, &id, &name, input, move |p| {
                    let _ = ptx.send(progress_line(&p));
                })
                .await;
            let line = match res {
                Ok(doc) => json!({ "doc": doc }),
                Err(e) => error_line(&e, Some(&name)),
            };
            let _ = tx.send(line);
        }
    });
    Ok(resp)
}

async fn reindex(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Response> {
    check_id(&id)?;
    if s.assistants.get(&id)?.is_none() {
        return Err(ApiError::not_found("assistant not found"));
    }
    let (tx, resp) = ndjson_channel();
    let url = s.ollama_url().await;
    tokio::spawn(async move {
        let ptx = tx.clone();
        let res = s
            .assistants
            .reindex(&url, &id, move |p| {
                let _ = ptx.send(progress_line(&p));
            })
            .await;
        let line = match res {
            Ok(a) => json!({ "assistant": a }),
            Err(e) => error_line(&e, None),
        };
        let _ = tx.send(line);
    });
    Ok(resp)
}

#[derive(Deserialize)]
struct SearchBody {
    query: String,
    k: Option<usize>,
}

async fn search(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SearchBody>,
) -> ApiResult<Response> {
    check_id(&id)?;
    let url = s.ollama_url().await;
    match s.assistants.search(&url, &id, &body.query, body.k).await {
        Ok(results) => Ok(Json(json!({ "results": results })).into_response()),
        Err(e) => match model_missing_response(&e) {
            Some(r) => Ok(r),
            None => Err(map_err(e)),
        },
    }
}
