//! The HTTP side: each channel's files in the engine's layout (decision
//! E.70) with `ETag` and conditional requests, and the push WebSocket's
//! upgrade (RFC 6455 section 4.2.2), after which the connection is served
//! on the raw socket.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt::Write as _;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use hyper::body::{Body, Bytes, Frame, Incoming, SizeHint};
use hyper::header::{CACHE_CONTROL, CONNECTION, CONTENT_TYPE, ETAG, HeaderValue, IF_NONE_MATCH, UPGRADE};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::connection;
use crate::hub::Hub;

/// A whole response body.
#[derive(Debug, Default)]
pub struct Whole(Option<Bytes>);

impl Body for Whole {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        Poll::Ready(self.0.take().map(|bytes| Ok(Frame::data(bytes))))
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_none()
    }

    /// The exact length, so the response carries `Content-Length`.
    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.0.as_ref().map_or(0, |bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX)))
    }
}

/// The server's channels by folder, and the next session's number.
#[derive(Debug)]
pub struct Routes {
    /// The channels.
    pub hubs: BTreeMap<String, Hub>,
    /// Sessions are numbered from here, never reused.
    pub sessions: AtomicU64,
}

/// The push WebSocket's path within a channel's folder.
pub const PUSH: &str = "push";
const T42_TYPE: &str = "application/x-teletext-t42";

/// Answers one request.
#[must_use]
pub fn answer(routes: &Routes, request: Request<Incoming>) -> Response<Whole> {
    let path = request.uri().path().trim_start_matches('/').to_owned();
    let (folder, file) = path.split_once('/').unwrap_or((path.as_str(), ""));
    if folder.is_empty() {
        return listing(routes);
    }
    let Some(hub) = routes.hubs.get(folder).cloned() else { return status(StatusCode::NOT_FOUND) };
    if file == PUSH {
        return upgrade(routes, hub, request);
    }
    let (content_type, body): (&str, Arc<[u8]>) = match file {
        "channel.json" => ("application/json", Arc::clone(&hub.descriptor)),
        "contents.txt" => ("text/plain", Arc::clone(&hub.contents)),
        "index.t42" => {
            let mut bytes = Vec::new();
            for (_, frame) in hub.pages.all() {
                bytes.extend_from_slice(&frame);
            }
            (T42_TYPE, bytes.into())
        }
        other => match page_of(other).and_then(|page| hub.pages.frame(page)) {
            Some(frame) => (T42_TYPE, frame),
            None => return status(StatusCode::NOT_FOUND),
        },
    };
    file_response(&request, content_type, &body)
}

/// A file, or 304 when the client holds it already.
fn file_response(request: &Request<Incoming>, content_type: &str, body: &[u8]) -> Response<Whole> {
    let tag = format!("\"{:016x}\"", fnv1a(body));
    let held = request.headers().get(IF_NONE_MATCH).is_some_and(|value| value.as_bytes() == tag.as_bytes());
    let mut response = if held { status(StatusCode::NOT_MODIFIED) } else { Response::new(Whole(Some(Bytes::copy_from_slice(body)))) };
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&tag) {
        headers.insert(ETAG, value);
    }
    headers.insert(CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// The channels this server has, as a plain list.
fn listing(routes: &Routes) -> Response<Whole> {
    let mut text = String::from("NeoTeletext server\n\n");
    for folder in routes.hubs.keys() {
        let _ = writeln!(text, "/{folder}/");
    }
    let mut response = Response::new(Whole(Some(Bytes::from(text))));
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    response
}

/// The push WebSocket: answers 101 and serves the upgraded socket.
fn upgrade(routes: &Routes, hub: Hub, mut request: Request<Incoming>) -> Response<Whole> {
    let wants = request.headers().get(UPGRADE).is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let Some(key) = request.headers().get("sec-websocket-key").map(|value| value.as_bytes().to_vec()).filter(|_| wants) else {
        return status(StatusCode::UPGRADE_REQUIRED);
    };
    let accept = neoteletext_web::ws::accept_for(&key);
    let session = routes.sessions.fetch_add(1, Ordering::Relaxed);
    let pending = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        let Ok(upgraded) = pending.await else { return };
        // The raw socket: no HTTP buffers stay with the connection.
        let Ok(parts) = upgraded.downcast::<TokioIo<TcpStream>>() else { return };
        connection::serve(parts.io.into_inner(), &parts.read_buf, hub, session).await;
    });
    let mut response = status(StatusCode::SWITCHING_PROTOCOLS);
    let headers = response.headers_mut();
    headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert(CONNECTION, HeaderValue::from_static("Upgrade"));
    if let Ok(value) = HeaderValue::from_bytes(&accept) {
        headers.insert("sec-websocket-accept", value);
    }
    response
}

fn status(code: StatusCode) -> Response<Whole> {
    let mut response = Response::new(Whole(None));
    *response.status_mut() = code;
    response
}

/// `NNN.t42` as a page number: three capital hexadecimal digits,
/// magazine 1 to 8.
fn page_of(name: &str) -> Option<u16> {
    let digits = name.strip_suffix(".t42")?;
    if digits.len() != 3 || !digits.bytes().all(|digit| digit.is_ascii_digit() || (b'A'..=b'F').contains(&digit)) {
        return None;
    }
    u16::from_str_radix(digits, 16).ok().filter(|page| (0x100..=0x8FF).contains(page))
}

/// FNV-1a over `bytes`, for an `ETag`: a change of content changes it.
fn fnv1a(bytes: &[u8]) -> u64 {
    // FNV-1a's 64-bit offset basis and prime (Fowler, Noll and Vo).
    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(BASIS, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(PRIME))
}
