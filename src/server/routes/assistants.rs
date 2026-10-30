//! Custom assistants with a knowledge base (RAG). See `docs/API.md` and
//! `docs/specs/assistants.md`.
//!
//! Errors: unknown assistant/document → 404, missing embedding model → 424
//! `{error, missing_model}` (also checked before an upload/re-index stream
//! starts), stale index on search → 409 `{error, code: "reindex_required"}`,
//! invalid input → 400. Document uploads and re-indexing stream NDJSON
//! progress lines (`{status, done, total}`) and end with `{doc}` /
//! `{assistant}` or `{error[, missing_model]}`.
//!
//! Deep understanding (v2.2): `/context` (per-turn retrieval with query
//! rewriting, expansion and a token budget), `/digest` (generate document
//! digests, NDJSON), `/docs/{doc_id}/digest` (view/edit) and `/templates`.
//! Closing an NDJSON stream cancels pending digest work.

use axum::{
    Json, Router,
    body::Body,
    extract::{FromRequest, Multipart, Path, Query, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::assistants::{
    AssistantPatch, AssistantStore, DocInput, ModelMissing, NewAssistant, NotFound, Progress,
    ReindexRequired, TooLarge, check_doc_text_size, deep, digest, extract, templates,
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
        .route("/assistants/templates", get(list_templates))
        .route("/assistants/{id}/context", post(context))
        .route("/assistants/{id}/digest", post(make_digests))
        .route(
            "/assistants/{id}/docs/{doc_id}/digest",
            get(get_digest).put(put_digest),
        )
}

/// Map core errors to HTTP: NotFound → 404, TooLarge → 400,
/// ModelMissing → 424, else 500.
fn map_err(e: anyhow::Error) -> ApiError {
    if let Some(nf) = e.downcast_ref::<NotFound>() {
        return ApiError::not_found(nf.to_string());
    }
    if let Some(tl) = e.downcast_ref::<TooLarge>() {
        return ApiError::bad_request(tl.to_string());
    }
    if e.downcast_ref::<ModelMissing>().is_some() {
        // Rendered with `missing_model` by `model_missing_response`.
        return ApiError::new(StatusCode::FAILED_DEPENDENCY, format!("{e:#}"));
    }
    ApiError::from(e)
}

/// 424 with `{error, missing_model}` so the UI can offer a pull.
fn missing_model_json(model: &str) -> Response {
    let mm = ModelMissing(model.to_string());
    (
        StatusCode::FAILED_DEPENDENCY,
        Json(json!({ "error": mm.to_string(), "missing_model": mm.0 })),
    )
        .into_response()
}

/// Typed errors with a JSON body richer than `{error}`: 424 for a missing
/// embedding model, 409 `{error, code: "reindex_required"}` for a stale index.
fn typed_error_response(e: &anyhow::Error) -> Option<Response> {
    if let Some(mm) = e.downcast_ref::<ModelMissing>() {
        return Some(missing_model_json(&mm.0));
    }
    let rr = e.downcast_ref::<ReindexRequired>()?;
    Some(
        (
            StatusCode::CONFLICT,
            Json(json!({ "error": rr.to_string(), "code": "reindex_required" })),
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

/// `Assistant[]`. Unreadable `assistant.json` files are backed up and left
/// out; their ids are listed in the `X-Corrupt-Assistants` header.
async fn list(State(s): State<AppState>) -> ApiResult<Response> {
    let list = s.assistants.list()?;
    let mut resp = Json(serde_json::to_value(list.assistants)?).into_response();
    if !list.corrupt.is_empty()
        && let Ok(v) = header::HeaderValue::from_str(&list.corrupt.join(","))
    {
        resp.headers_mut().insert("x-corrupt-assistants", v);
    }
    Ok(resp)
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

#[derive(Deserialize, Default)]
#[serde(default)]
struct TextPage {
    /// First character (not byte) to return.
    offset: Option<usize>,
    /// Max characters to return (default: all).
    limit: Option<usize>,
}

/// `{text, offset, total_chars, chunks, digest_chunks, pages?, warning?}`;
/// `?offset=&limit=` (characters) pages through a large document.
async fn doc_text(
    State(s): State<AppState>,
    Path((id, doc_id)): Path<(String, String)>,
    Query(page): Query<TextPage>,
) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    check_id(&doc_id)?;
    let text = s
        .assistants
        .doc_text(&id, &doc_id)?
        .ok_or_else(|| ApiError::not_found("document not found"))?;
    let total = text.chars().count();
    let offset = page.offset.unwrap_or(0).min(total);
    let slice: String = match page.limit {
        Some(n) => text.chars().skip(offset).take(n).collect(),
        None if offset == 0 => text,
        None => text.chars().skip(offset).collect(),
    };
    let doc = s
        .assistants
        .get(&id)?
        .and_then(|a| a.docs.into_iter().find(|d| d.id == doc_id));
    Ok(Json(json!({
        "text": slice,
        "offset": offset,
        "total_chars": total,
        "chunks": doc.as_ref().map_or(0, |d| d.chunks),
        "digest_chunks": doc.as_ref().and_then(|d| d.digest.as_ref()).map_or(0, |g| g.chunks),
        "pages": doc.as_ref().and_then(|d| d.pages),
        "warning": doc.as_ref().and_then(|d| d.warning.clone()),
    })))
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

/// App settings the deep mode depends on.
async fn app_defaults(s: &AppState) -> deep::AppDefaults {
    let st = s.settings.read().await;
    deep::AppDefaults {
        num_ctx: st.num_ctx,
        think: st.think,
        default_model: st.default_model.clone(),
    }
}

/// Chat model that writes digests: the assistant's own, else the one the UI
/// passed, else the app's default model.
fn digest_model(
    a: &crate::assistants::Assistant,
    ui: Option<String>,
    app: &deep::AppDefaults,
) -> Option<String> {
    a.model
        .clone()
        .or(ui)
        .or_else(|| app.default_model.clone())
        .filter(|m| !m.trim().is_empty())
}

/// Generate the digest of one document, streaming `digesting` progress.
/// Returns the error text when it failed (the document stays indexed).
async fn digest_one(
    s: &AppState,
    url: &str,
    id: &str,
    doc_id: &str,
    ui_model: Option<String>,
    tx: &mpsc::UnboundedSender<Value>,
) -> Option<String> {
    let app = app_defaults(s).await;
    let a = s.assistants.get(id).ok()??;
    let Some(model) = digest_model(&a, ui_model, &app) else {
        return Some("no chat model selected for digests".into());
    };
    let num_ctx = digest::digest_num_ctx(&a, &app);
    let ctx = tx.clone();
    let cancelled = move || ctx.is_closed();
    let job = digest::DigestJob {
        model: &model,
        num_ctx,
        cancelled: &cancelled,
    };
    let ptx = tx.clone();
    let mut err = None;
    let res = s
        .assistants
        .digest_documents(
            url,
            id,
            Some(doc_id),
            true,
            &job,
            move |p| {
                let _ = ptx.send(progress_line(&p));
            },
            |_, _, r| {
                if let Err(e) = r {
                    err = Some(format!("{e:#}"));
                }
            },
        )
        .await;
    if let Err(e) = res {
        return Some(format!("{e:#}"));
    }
    err
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct UploadQuery {
    /// Chat model for digests when the assistant has none (the UI's model).
    model: Option<String>,
}

#[derive(Deserialize)]
struct NoteBody {
    name: String,
    text: String,
}

/// Multipart file(s) (any field with a file name) or JSON `{name, text}` for a
/// note. Files are indexed one after another; each ends with `{doc}` or
/// `{error, name}`. Unsupported files among supported ones get an
/// `{error, name}` line; if none is supported → 400.
async fn add_docs(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(uq): Query<UploadQuery>,
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
    let mut skipped: Vec<String> = Vec::new();
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
                skipped.push(name);
                continue;
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(e.body_text()))?;
            inputs.push((name, DocInput::File(bytes.to_vec())));
        }
        if inputs.is_empty() {
            return Err(ApiError::bad_request(match skipped.first() {
                Some(name) => format!("unsupported file type: {name}"),
                None => "no files in the upload".into(),
            }));
        }
    } else {
        let Json(note) = Json::<NoteBody>::from_request(req, &s)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        if note.name.trim().is_empty() || note.text.trim().is_empty() {
            return Err(ApiError::bad_request("note name and text are required"));
        }
        // Cheap early 400 for oversized notes; files are checked after
        // extraction (reported as an `{error, name}` line, before embedding).
        check_doc_text_size(note.name.trim(), note.text.len())
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
        inputs.push((note.name, DocInput::Note(note.text)));
    }

    let url = s.ollama_url().await;
    if let Some(model) = s.assistants.missing_embed_model(&url, &id).await {
        return Ok(missing_model_json(&model));
    }
    let (tx, resp) = ndjson_channel();
    tokio::spawn(async move {
        // Unsupported files in a multi-file upload are skipped, not fatal.
        for name in skipped {
            let e = anyhow::anyhow!("unsupported file type: {name}");
            let _ = tx.send(error_line(&e, Some(&name)));
        }
        for (name, input) in inputs {
            if tx.is_closed() {
                break; // the client cancelled
            }
            let ptx = tx.clone();
            let res = s
                .assistants
                .add_document(&url, &id, &name, input, move |p| {
                    let _ = ptx.send(progress_line(&p));
                })
                .await;
            let line = match res {
                Ok(doc) => {
                    // Deep understanding: digest the new document (optional:
                    // a failure leaves it indexed, reported as `digest_error`).
                    let deep_on = s.assistants.get(&id).ok().flatten().is_some_and(|a| a.deep);
                    if deep_on && !tx.is_closed() {
                        let err = digest_one(&s, &url, &id, &doc.id, uq.model.clone(), &tx).await;
                        let fresh = s
                            .assistants
                            .get(&id)
                            .ok()
                            .flatten()
                            .and_then(|a| a.docs.into_iter().find(|d| d.id == doc.id))
                            .unwrap_or(doc);
                        let mut v = json!({ "doc": fresh });
                        if let Some(e) = err {
                            v["digest_error"] = json!(e);
                        }
                        v
                    } else {
                        json!({ "doc": doc })
                    }
                }
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
    let url = s.ollama_url().await;
    if let Some(model) = s.assistants.missing_embed_model(&url, &id).await {
        return Ok(missing_model_json(&model));
    }
    let (tx, resp) = ndjson_channel();
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
        Err(e) => match typed_error_response(&e) {
            Some(r) => Ok(r),
            None => Err(map_err(e)),
        },
    }
}

// ---- deep understanding ---------------------------------------------------

/// Instruction templates (`[{id, icon, name, description, instructions}]`,
/// each text `{ar, en}`).
async fn list_templates() -> Json<Value> {
    Json(json!(templates::TEMPLATES))
}

/// Retrieval for one chat turn: rewritten query, passages (with digests and
/// neighbour expansion in deep mode), effective `num_ctx`/`think`.
async fn context(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<deep::ContextRequest>,
) -> ApiResult<Response> {
    check_id(&id)?;
    let url = s.ollama_url().await;
    let app = app_defaults(&s).await;
    match s.assistants.context(&url, &id, body, &app).await {
        Ok(r) => Ok(Json(serde_json::to_value(r)?).into_response()),
        Err(e) => match typed_error_response(&e) {
            Some(r) => Ok(r),
            None => Err(map_err(e)),
        },
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DigestBody {
    /// Only this document (default: every document without a good digest).
    doc_id: Option<String>,
    /// Regenerate existing digests too.
    force: bool,
    /// Chat model when the assistant has none (the UI's model).
    model: Option<String>,
}

/// NDJSON: `{status: "digesting"|"embedding"|"saving", done, total}` lines,
/// per document `{doc}` or `{error, name}`, then `{assistant}`. Closing the
/// stream cancels.
async fn make_digests(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<DigestBody>,
) -> ApiResult<Response> {
    check_id(&id)?;
    let a = s
        .assistants
        .get(&id)?
        .ok_or_else(|| ApiError::not_found("assistant not found"))?;
    if let Some(d) = &body.doc_id
        && !a.docs.iter().any(|x| &x.id == d)
    {
        return Err(ApiError::not_found("document not found"));
    }
    let app = app_defaults(&s).await;
    let Some(model) = digest_model(&a, body.model.clone(), &app) else {
        return Err(ApiError::bad_request(
            "no chat model: choose a model for the assistant first",
        ));
    };
    let url = s.ollama_url().await;
    if let Some(m) = s.assistants.missing_embed_model(&url, &id).await {
        return Ok(missing_model_json(&m));
    }
    let num_ctx = digest::digest_num_ctx(&a, &app);
    let (tx, resp) = ndjson_channel();
    tokio::spawn(async move {
        let ctx = tx.clone();
        let cancelled = move || ctx.is_closed();
        let job = digest::DigestJob {
            model: &model,
            num_ctx,
            cancelled: &cancelled,
        };
        let ptx = tx.clone();
        let dtx = tx.clone();
        let st = s.clone();
        let sid = id.clone();
        let res =
            s.assistants
                .digest_documents(
                    &url,
                    &id,
                    body.doc_id.as_deref(),
                    body.force,
                    &job,
                    move |p| {
                        let _ = ptx.send(progress_line(&p));
                    },
                    move |doc_id, name, r| {
                        let line =
                            match r {
                                Ok(_) => {
                                    let doc =
                                        st.assistants.get(&sid).ok().flatten().and_then(|a| {
                                            a.docs.into_iter().find(|d| d.id == doc_id)
                                        });
                                    json!({ "doc": doc })
                                }
                                Err(e) => error_line(&e, Some(name)),
                            };
                        let _ = dtx.send(line);
                    },
                )
                .await;
        let line = match res {
            Ok(a) => json!({ "assistant": a }),
            Err(e) => error_line(&e, None),
        };
        let _ = tx.send(line);
    });
    Ok(resp)
}

/// `{text, digest: DigestMeta}`; 404 when the document has no digest.
async fn get_digest(
    State(s): State<AppState>,
    Path((id, doc_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    check_id(&id)?;
    check_id(&doc_id)?;
    let a = s
        .assistants
        .get(&id)?
        .ok_or_else(|| ApiError::not_found("assistant not found"))?;
    let doc = a
        .docs
        .iter()
        .find(|d| d.id == doc_id)
        .ok_or_else(|| ApiError::not_found("document not found"))?;
    let text = s.assistants.digest_text(&id, &doc_id)?;
    if text.is_none() && doc.digest.is_none() {
        return Err(ApiError::not_found("no digest"));
    }
    Ok(Json(
        json!({ "text": text.unwrap_or_default(), "digest": doc.digest }),
    ))
}

#[derive(Deserialize)]
struct DigestEdit {
    text: String,
}

/// Save an edited digest (re-embeds its chunks; empty text removes it) →
/// `{doc}`.
async fn put_digest(
    State(s): State<AppState>,
    Path((id, doc_id)): Path<(String, String)>,
    Json(body): Json<DigestEdit>,
) -> ApiResult<Response> {
    check_id(&id)?;
    check_id(&doc_id)?;
    let url = s.ollama_url().await;
    match s
        .assistants
        .save_digest(&url, &id, &doc_id, &body.text)
        .await
    {
        Ok(doc) => Ok(Json(json!({ "doc": doc })).into_response()),
        Err(e) => match typed_error_response(&e) {
            Some(r) => Ok(r),
            None => Err(map_err(e)),
        },
    }
}
