//! The administration listener (decision 19): `/metrics` in the
//! Prometheus text format (requirement 8.1), `/healthz` and `/readyz`
//! (requirement 8.4), on an address of their own, never the one viewers
//! reach.

use std::sync::Arc;

use hyper::body::{Bytes, Incoming};
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue};
use hyper::{Request, Response, StatusCode};

use crate::http::Whole;
use crate::stats::ServerStats;

/// The Prometheus text exposition format's media type, version 0.0.4.
pub const METRICS_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Answers one request to the administration listener.
#[must_use]
pub fn answer(stats: &Arc<ServerStats>, request: &Request<Incoming>) -> Response<Whole> {
    match request.uri().path() {
        "/metrics" => text(StatusCode::OK, METRICS_TYPE, stats.metrics()),
        "/healthz" => text(StatusCode::OK, "text/plain", "ok\n".to_owned()),
        "/readyz" => {
            let waiting = stats.not_ready();
            if waiting.is_empty() {
                text(StatusCode::OK, "text/plain", "ready\n".to_owned())
            } else {
                text(StatusCode::SERVICE_UNAVAILABLE, "text/plain", format!("no pages yet: {}\n", waiting.join(", ")))
            }
        }
        _ => text(StatusCode::NOT_FOUND, "text/plain", "not found\n".to_owned()),
    }
}

fn text(status: StatusCode, content_type: &'static str, body: String) -> Response<Whole> {
    let mut response = Response::new(Whole::of(Bytes::from(body)));
    *response.status_mut() = status;
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
