//! Request guard against other websites talking to the local server.
//!
//! The server listens on 127.0.0.1, but any page open in the user's normal
//! browser can still send requests to it (CSRF), and DNS rebinding can make a
//! foreign hostname resolve to 127.0.0.1. So every request must:
//! - carry a `Host` of `127.0.0.1:<port>` or `localhost:<port>`, and
//! - if it carries an `Origin`, that origin must be this server itself, and
//! - if it carries `Sec-Fetch-Site`, that must be `same-origin` or `none` (typed URL,
//!   app start). This catches cross-site `<img>`/`<script>`/link GETs, which carry no
//!   `Origin`. WebView2/Chromium sends it on every request; clients without it (curl)
//!   are judged by Host/Origin alone, as before.
//!
//! Every response (including the 403) also gets `Cross-Origin-Resource-Policy:
//! same-origin` and `X-Content-Type-Options: nosniff`, so another origin can never
//! embed or sniff what the server returns.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const CROSS_ORIGIN_RESOURCE_POLICY: HeaderName =
    HeaderName::from_static("cross-origin-resource-policy");

/// The port the server is bound to (middleware state).
#[derive(Clone, Copy)]
pub struct BoundPort(pub u16);

pub async fn check(State(BoundPort(port)): State<BoundPort>, req: Request, next: Next) -> Response {
    let mut resp = if allowed(req.headers(), port) {
        next.run(req).await
    } else {
        crate::error::ApiError::new(StatusCode::FORBIDDEN, "forbidden").into_response()
    };
    let headers = resp.headers_mut();
    headers.insert(
        CROSS_ORIGIN_RESOURCE_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    resp
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
    // Absent = not a browser (or a very old one): Host/Origin decide, as before.
    let fetch_site_ok = match headers.get(SEC_FETCH_SITE) {
        None => true,
        Some(v) => matches!(v.to_str(), Ok("same-origin" | "none")),
    };
    host_ok && origin_ok && fetch_site_ok
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

    fn with_fetch_site(site: &str) -> HeaderMap {
        let mut h = headers(Some("127.0.0.1:8790"), None);
        h.insert(SEC_FETCH_SITE, HeaderValue::from_str(site).unwrap());
        h
    }

    #[test]
    fn sec_fetch_site() {
        // A cross-site <img>/<script> GET carries no Origin, only Sec-Fetch-Site.
        assert!(!allowed(&with_fetch_site("cross-site"), 8790));
        // Another port on 127.0.0.1 is "same-site", still a different origin.
        assert!(!allowed(&with_fetch_site("same-site"), 8790));
        assert!(!allowed(&with_fetch_site("bogus"), 8790));
        assert!(allowed(&with_fetch_site("same-origin"), 8790));
        assert!(allowed(&with_fetch_site("none"), 8790));
        // Missing header (curl, tools): unchanged behaviour.
        assert!(allowed(&headers(Some("127.0.0.1:8790"), None), 8790));
        // Sec-Fetch-Site never overrides a bad Origin.
        let mut h = headers(Some("127.0.0.1:8790"), Some("https://evil.example"));
        h.insert(SEC_FETCH_SITE, HeaderValue::from_static("same-origin"));
        assert!(!allowed(&h, 8790));
    }

    /// Through a real server: the 403 for cross-site, and the hardening headers on
    /// both allowed and rejected responses.
    #[tokio::test]
    async fn middleware_rejects_and_sets_headers() {
        use axum::{Router, middleware, routing::get};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = Router::new()
            .route("/x", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(BoundPort(port), check));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/x");
        let get = |site: Option<&'static str>| {
            let mut r = client.get(&url);
            if let Some(s) = site {
                r = r.header("sec-fetch-site", s);
            }
            r.send()
        };
        for (site, status) in [
            (None, 200),
            (Some("same-origin"), 200),
            (Some("none"), 200),
            (Some("cross-site"), 403),
        ] {
            let resp = get(site).await.unwrap();
            assert_eq!(resp.status().as_u16(), status, "{site:?}");
            let h = resp.headers();
            assert_eq!(h["cross-origin-resource-policy"], "same-origin", "{site:?}");
            assert_eq!(h["x-content-type-options"], "nosniff", "{site:?}");
        }
    }
}
