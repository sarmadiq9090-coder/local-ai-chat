//! Request guard against other websites talking to the local server.
//!
//! The server listens on 127.0.0.1, but any page open in the user's normal
//! browser can still send requests to it (CSRF), and DNS rebinding can make a
//! foreign hostname resolve to 127.0.0.1. So every request must:
//! - carry a `Host` of `127.0.0.1:<port>` or `localhost:<port>`, and
//! - if it carries an `Origin`, that origin must be this server itself.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// The port the server is bound to (middleware state).
#[derive(Clone, Copy)]
pub struct BoundPort(pub u16);

pub async fn check(State(BoundPort(port)): State<BoundPort>, req: Request, next: Next) -> Response {
    if allowed(req.headers(), port) {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, "forbidden").into_response()
    }
}

fn allowed(headers: &HeaderMap, port: u16) -> bool {
    let local = |authority: &str| {
        authority == format!("127.0.0.1:{port}") || authority == format!("localhost:{port}")
    };
    let host_ok = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(local);
    let origin_ok = match headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
        None => true,
        Some(origin) => origin.strip_prefix("http://").is_some_and(local),
    };
    host_ok && origin_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(host: Option<&str>, origin: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(v) = host {
            h.insert(header::HOST, HeaderValue::from_str(v).unwrap());
        }
        if let Some(v) = origin {
            h.insert(header::ORIGIN, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn accepts_own_origin_and_plain_requests() {
        assert!(allowed(&headers(Some("127.0.0.1:8790"), None), 8790));
        assert!(allowed(
            &headers(Some("localhost:8790"), Some("http://localhost:8790")),
            8790
        ));
        assert!(allowed(
            &headers(Some("127.0.0.1:8790"), Some("http://127.0.0.1:8790")),
            8790
        ));
    }

    #[test]
    fn rejects_foreign_sites_and_rebinding() {
        assert!(!allowed(
            &headers(Some("127.0.0.1:8790"), Some("https://evil.example")),
            8790
        ));
        assert!(!allowed(
            &headers(Some("127.0.0.1:8790"), Some("http://localhost:3000")),
            8790
        ));
        assert!(!allowed(
            &headers(Some("127.0.0.1:8790"), Some("null")),
            8790
        ));
        assert!(!allowed(&headers(Some("evil.example:8790"), None), 8790));
        assert!(!allowed(&headers(None, None), 8790));
    }
}
