//! Static UI files, embedded into the executable at compile time from `ui/`.
//!
//! Security headers:
//! - every file: `X-Content-Type-Options: nosniff`;
//! - `index.html`: `Content-Security-Policy` = the policy of its own
//!   `<meta http-equiv="Content-Security-Policy">` (read from the file, so the two
//!   never diverge) plus `frame-ancestors 'none'` (not allowed in a meta tag);
//! - `features/preview/frame.html`: only `frame-ancestors 'self'` — it must NOT get
//!   the page policy, because the preview runs inline and CDN scripts inside it.

use axum::{
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

use crate::error::ApiError;

#[derive(Embed)]
#[folder = "ui/"]
struct Ui;

const PREVIEW_FRAME: &str = "features/preview/frame.html";

pub async fn serve(uri: Uri) -> Response {
    let path = match uri.path().trim_start_matches('/') {
        "" => "index.html",
        p => p,
    };
    // Unknown API routes (e.g. `/api/app/nope`) answer in JSON like the rest of the API.
    if path == "api" || path.starts_with("api/") {
        return ApiError::not_found("not found").into_response();
    }
    match Ui::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let csp = match path {
                "index.html" => {
                    meta_csp(&file.data).map(|p| format!("{p}; frame-ancestors 'none'"))
                }
                PREVIEW_FRAME => Some("frame-ancestors 'self'".to_string()),
                _ => None,
            };
            let mut resp = (
                [
                    (header::CONTENT_TYPE, mime.as_ref().to_string()),
                    (header::CACHE_CONTROL, "no-cache".to_string()),
                ],
                file.data,
            )
                .into_response();
            let headers = resp.headers_mut();
            headers.insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            if let Some(csp) = csp.and_then(|c| HeaderValue::from_str(&c).ok()) {
                headers.insert(header::CONTENT_SECURITY_POLICY, csp);
            }
            resp
        }
        None => (
            StatusCode::NOT_FOUND,
            [(header::X_CONTENT_TYPE_OPTIONS, "nosniff")],
            "not found",
        )
            .into_response(),
    }
}

/// The policy in `<meta http-equiv="Content-Security-Policy" content="...">`.
fn meta_csp(html: &[u8]) -> Option<String> {
    let html = std::str::from_utf8(html).ok()?;
    let marker = "http-equiv=\"Content-Security-Policy\"";
    let tag_start = html.find(marker)?;
    let rest = &html[tag_start..];
    let rest = &rest[..rest.find('>')?];
    let content = &rest[rest.find("content=\"")? + "content=\"".len()..];
    let policy = content[..content.find('"')?].trim();
    (!policy.is_empty()).then(|| policy.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_meta_csp_is_found() {
        let index = Ui::get("index.html").unwrap();
        let csp = meta_csp(&index.data).expect("index.html has a CSP meta tag");
        assert!(csp.starts_with("default-src 'self'"), "{csp}");
        assert!(!csp.contains('"'));
    }

    #[tokio::test]
    async fn headers_per_file() {
        let index = serve(Uri::from_static("/")).await;
        let csp = index.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains("script-src 'self'") && csp.ends_with("frame-ancestors 'none'"));
        assert_eq!(index.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

        let frame = serve(Uri::from_static("/features/preview/frame.html")).await;
        assert_eq!(
            frame.headers()[header::CONTENT_SECURITY_POLICY],
            "frame-ancestors 'self'"
        );
        assert_eq!(frame.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

        let js = serve(Uri::from_static("/main.js")).await;
        assert!(js.headers().get(header::CONTENT_SECURITY_POLICY).is_none());
        assert_eq!(js.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

        let api = serve(Uri::from_static("/api/app/nope")).await;
        assert_eq!(api.status(), StatusCode::NOT_FOUND);
        assert!(
            api.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("application/json")
        );
    }
}
