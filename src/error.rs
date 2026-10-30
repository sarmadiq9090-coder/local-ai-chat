//! API error type: every handler returns `Result<_, ApiError>` and errors are
//! rendered as `{"error": "..."}` with a matching status code.

use axum::{
    Json,
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{:#}", e.into()))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Largest non-JSON error body we rewrap (rejection messages are short).
const MAX_REWRAP_BYTES: usize = 16 * 1024;

/// Middleware for `/api/app`: turn axum's plain-text extractor rejections
/// (bad JSON → 400/415/422, bad path/query → 400, body too large → 413,
/// multipart errors, 405…) into `{"error": "..."}` with the same status.
/// JSON (and NDJSON) responses and successful responses pass through.
pub async fn json_errors(req: Request, next: Next) -> Response {
    let resp = next.run(req).await;
    let status = resp.status();
    if !(status.is_client_error() || status.is_server_error()) || is_json(&resp) {
        return resp;
    }
    let (parts, body) = resp.into_parts();
    let text = match axum::body::to_bytes(body, MAX_REWRAP_BYTES).await {
        Ok(b) => String::from_utf8_lossy(&b).trim().to_string(),
        Err(_) => String::new(),
    };
    let message = if text.is_empty() {
        status.canonical_reason().unwrap_or("error").to_string()
    } else {
        text
    };
    let mut out = ApiError::new(status, message).into_response();
    // Keep headers such as `Allow` (405); content type/length come from JSON.
    for (name, value) in parts.headers.iter() {
        if name != header::CONTENT_TYPE && name != header::CONTENT_LENGTH {
            out.headers_mut().append(name.clone(), value.clone());
        }
    }
    out
}

fn is_json(resp: &Response<Body>) -> bool {
    resp.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v: &HeaderValue| v.to_str().ok())
        .is_some_and(|ct| {
            let ct = ct.to_ascii_lowercase();
            ct.starts_with("application/json") || ct.starts_with("application/x-ndjson")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        extract::{DefaultBodyLimit, Path, Query},
        middleware,
        routing::{get, put},
    };
    use serde::Deserialize;
    use serde_json::Value;

    #[derive(Deserialize)]
    struct Body1 {
        #[allow(dead_code)]
        name: String,
    }
    #[derive(Deserialize)]
    struct Q {
        #[allow(dead_code)]
        n: u32,
    }

    fn app() -> Router {
        Router::new()
            .route("/json", put(|Json(_): Json<Body1>| async { "ok" }))
            .route("/num/{n}", get(|Path(_n): Path<u32>| async { "ok" }))
            .route("/q", get(|Query(_): Query<Q>| async { "ok" }))
            .route(
                "/api-error",
                get(|| async { ApiError::bad_request("custom").into_response() }),
            )
            .layer(DefaultBodyLimit::max(64))
            .layer(middleware::from_fn(json_errors))
    }

    async fn call(method: &str, uri: &str, ct: Option<&str>, body: &str) -> (StatusCode, Value) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app()).await.unwrap() });
        let mut b = reqwest::Client::new().request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            format!("http://{addr}{uri}"),
        );
        if let Some(ct) = ct {
            b = b.header(header::CONTENT_TYPE.as_str(), ct);
        }
        let resp = b.body(body.to_string()).send().await.unwrap();
        let status = StatusCode::from_u16(resp.status().as_u16()).unwrap();
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            ct.starts_with("application/json"),
            "{uri}: content-type {ct}"
        );
        (status, resp.json().await.unwrap())
    }

    #[tokio::test]
    async fn rejections_become_json_errors() {
        let json = Some("application/json");
        let (st, v) = call("PUT", "/json", json, "{bad").await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("JSON"), "{v}");

        let (st, v) = call("PUT", "/json", json, r#"{"nope":1}"#).await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(v["error"].as_str().unwrap().contains("name"), "{v}");

        let (st, _) = call("PUT", "/json", None, "{}").await;
        assert_eq!(st, StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let big = format!(r#"{{"name":"{}"}}"#, "x".repeat(200));
        let (st, v) = call("PUT", "/json", json, &big).await;
        assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(v["error"].is_string());

        let (st, v) = call("GET", "/num/abc", None, "").await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(v["error"].is_string());

        let (st, _) = call("GET", "/q?n=x", None, "").await;
        assert_eq!(st, StatusCode::BAD_REQUEST);

        let (st, v) = call("POST", "/json", json, "{}").await;
        assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(v["error"], "Method Not Allowed");

        // Already-JSON errors are untouched.
        let (st, v) = call("GET", "/api-error", None, "").await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert_eq!(v["error"], "custom");
    }
}
