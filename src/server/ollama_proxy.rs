//! Transparent, streaming passthrough: `/api/ollama/<path>` → `<ollama_url>/api/<path>`.
//!
//! The UI talks to Ollama only through here, so the Ollama URL is configurable
//! in one place and there are no CORS issues.

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::any,
};
use futures_util::TryStreamExt;

use super::AppState;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new().route("/{*path}", any(forward))
}

async fn forward(
    State(state): State<AppState>,
    Path(path): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let mut url = format!("{}/api/{}", state.ollama_url().await, path);
    if let Some(q) = uri.query() {
        url.push('?');
        url.push_str(q);
    }

    let method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|_| ApiError::bad_request("bad method"))?;
    let mut req = state.ollama.http().request(method, url);
    if let Some(ct) = headers.get(header::CONTENT_TYPE) {
        req = req.header(header::CONTENT_TYPE.as_str(), ct.as_bytes());
    } else if !body.is_empty() {
        req = req.header(header::CONTENT_TYPE.as_str(), "application/json");
    }
    if !body.is_empty() {
        req = req.body(body);
    }

    let upstream = req.send().await.map_err(|e| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("Ollama is not reachable / Ollama غير متاح: {e}"),
        )
    })?;

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = upstream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let stream = upstream.bytes_stream().map_err(std::io::Error::other);

    Ok((
        status,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-cache".into()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
