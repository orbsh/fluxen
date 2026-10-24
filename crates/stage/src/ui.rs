//! UI serving fallback: reverse-proxy to `trunk serve` when its port is up,
//! otherwise serve static files from a dist directory (trunk's output).
//!
//! One `stage serve` then covers the whole dev loop: open http://localhost:3002/
//! regardless of whether trunk happens to be running.
//!
//! The probe is TCP-only at startup (the choice is sticky); responses are
//! buffered in memory — acceptable for a dev gateway on static assets.

use axum::body::Body;
use axum::extract::{Extension, Request};
use axum::http::{Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct UiState {
    /// Some(addr) when proxy mode was chosen at startup (trunk up).
    pub proxy: Option<std::net::SocketAddr>,
    pub dist: Arc<PathBuf>,
}

/// Probe a TCP port with a short connect attempt.
pub async fn port_up(addr: std::net::SocketAddr) -> bool {
    tokio::time::timeout(
        std::time::Duration::from_millis(300),
        tokio::net::TcpStream::connect(addr),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

/// Map a URI path to a dist-relative asset path.
/// Returns None for anything that would escape the root (traversal,
/// absolute, backslash) or contains dot-dot/dot segments.
pub fn normalize_asset_path(path: &str) -> Option<String> {
    let path = path.split('?').next().unwrap_or(path);
    let path = path.strip_prefix('/').unwrap_or(path);
    if path.is_empty() {
        return Some("index.html".into());
    }
    if path.contains('\\') {
        return None;
    }
    let mut out = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." | ".." => return None, // strict: no empty/./.. segments
            s => out.push(s),
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out.join("/"))
}

pub fn content_type_for(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "wasm" => "application/wasm",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// Build a reply from a status and body. `Response::builder()` fails only on an
/// invalid status/header value, and every call site here passes constants — so
/// the failure arm degrades to a bare 500 instead of panicking mid-handler.
fn reply(status: StatusCode, body: impl Into<Body>) -> Response<Body> {
    let mut out = Response::new(body.into());
    *out.status_mut() = status;
    out
}

/// The same, plus one header (a `HeaderValue` is already validated, so the
/// insert cannot fail).
fn reply_typed(
    status: StatusCode,
    ct: axum::http::HeaderValue,
    body: impl Into<Body>,
) -> Response<Body> {
    let mut out = reply(status, body);
    out.headers_mut().insert(axum::http::header::CONTENT_TYPE, ct);
    out
}

/// axum fallback handler: routed paths (/channel, /cli, /send) never land here.
pub async fn ui_fallback(Extension(st): Extension<UiState>, req: Request) -> Response<Body> {
    match st.proxy {
        Some(trunk) => proxy_to(trunk, req).await,
        None => serve_static(&st, req).await,
    }
}

async fn proxy_to(trunk: std::net::SocketAddr, req: Request) -> Response<Body> {
    let (mut parts, body) = req.into_parts();
    let uri = parts.uri.clone();
    let origin = format!(
        "http://{trunk}{}{}",
        uri.path(),
        uri.query().map(|q| format!("?{q}")).unwrap_or_default()
    );
    let parsed: axum::http::Uri = match origin.parse() {
        Ok(u) => u,
        Err(_) => return reply(StatusCode::BAD_REQUEST, "bad uri"),
    };
    parts.uri = parsed;
    // A bad configured host is a 400, not a panic in the proxy: the value
    // comes from configuration, so its validity is data, not an invariant.
    let host: axum::http::HeaderValue = match trunk.to_string().parse() {
        Ok(v) => v,
        Err(_) => return reply(StatusCode::BAD_REQUEST, "bad trunk host"),
    };
    parts.headers.insert(axum::http::header::HOST, host);
    let client = Client::builder(TokioExecutor::new()).build_http();
    let fwd = Request::from_parts(parts, body);
    match client.request(fwd).await {
        Ok(resp) => {
            use http_body_util::BodyExt;
            let (p, b) = resp.into_parts();
            match b.collect().await {
                Ok(buf) => {
                    let bytes = buf.to_bytes();
                    // keep only content-type from trunk; avoid forwarding
                    // hop-by-hop / length-mismatched headers
                    let ct = p
                        .headers
                        .get(axum::http::header::CONTENT_TYPE)
                        .cloned()
                        .unwrap_or_else(|| {
                            axum::http::HeaderValue::from_static("application/octet-stream")
                        });
                    reply_typed(p.status, ct, Body::from(bytes))
                }
                Err(e) => reply(
                    StatusCode::BAD_GATEWAY,
                    format!("trunk body read failed: {e}"),
                ),
            }
        }
        Err(e) => reply(StatusCode::BAD_GATEWAY, format!("trunk proxy failed: {e}")),
    }
}

async fn serve_static(st: &UiState, req: Request) -> Response<Body> {
    let Some(rel) = normalize_asset_path(req.uri().path()) else {
        return not_found("no such path");
    };
    match tokio::fs::read(st.dist.join(&rel)).await {
        Ok(bytes) => reply_typed(
            StatusCode::OK,
            axum::http::HeaderValue::from_static(content_type_for(&rel)),
            Body::from(bytes),
        ),
        Err(_) => not_found(format!("not in dist: /{rel}")),
    }
}

fn not_found(msg: impl Into<Body>) -> Response<Body> {
    reply(StatusCode::NOT_FOUND, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_maps_to_index() {
        assert_eq!(normalize_asset_path("/"), Some("index.html".into()));
        assert_eq!(normalize_asset_path(""), Some("index.html".into()));
    }

    #[test]
    fn plain_asset_paths_pass_through() {
        assert_eq!(
            normalize_asset_path("/ui_leptos-abc_bg.wasm"),
            Some("ui_leptos-abc_bg.wasm".into())
        );
        assert_eq!(
            normalize_asset_path("/assets/main.css"),
            Some("assets/main.css".into())
        );
    }

    #[test]
    fn traversal_and_dot_segments_rejected() {
        assert_eq!(normalize_asset_path("/../etc/passwd"), None);
        assert_eq!(normalize_asset_path("/a/../../b"), None);
        assert_eq!(normalize_asset_path("//etc/passwd"), None);
        assert_eq!(normalize_asset_path("/a\\b"), None);
        assert_eq!(normalize_asset_path("/a/./b"), None);
        assert_eq!(normalize_asset_path("/a/"), None);
    }

    #[test]
    fn query_strings_stripped_before_matching() {
        assert_eq!(normalize_asset_path("/app.js?v=3"), Some("app.js".into()));
    }

    #[test]
    fn known_content_types() {
        assert_eq!(content_type_for("index.html"), "text/html; charset=utf-8");
        assert_eq!(content_type_for("app_bg.wasm"), "application/wasm");
        assert_eq!(content_type_for("a.js"), "text/javascript");
        assert_eq!(content_type_for("a.css"), "text/css");
        assert_eq!(content_type_for("nope"), "application/octet-stream");
    }
}
