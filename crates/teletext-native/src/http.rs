//! The HTTP side: each channel's files in the engine's layout (decision
//! E.70) with `ETag` and conditional requests, and the push WebSocket's
//! upgrade (RFC 6455 section 4.2.2), after which the connection is served
//! on the raw socket.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt::Write as _;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError};
use std::task::{Context, Poll};

use hyper::body::{Body, Bytes, Frame, Incoming, SizeHint};
use hyper::header::{CACHE_CONTROL, CONNECTION, CONTENT_TYPE, COOKIE, ETAG, HeaderValue, IF_NONE_MATCH, SET_COOKIE, UPGRADE};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use teletext_runtime::SessionId;
use teletext_runtime::session_cookie::{self, Key};

use crate::connection;
use crate::hub::Hub;
use crate::stats::{ServerStats, count};

/// A whole response body.
#[derive(Debug, Default)]
pub struct Whole(Option<Bytes>);

impl Whole {
    /// A body of `bytes`.
    #[must_use]
    pub const fn of(bytes: Bytes) -> Self {
        Self(Some(bytes))
    }
}

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
    /// Sessions are numbered from here, never reused, across restarts too
    /// (requirement 5.4: `session_cookie::first_session`).
    pub sessions: AtomicU64,
    /// The server's figures.
    pub stats: Arc<ServerStats>,
    /// How sessions continue across connections, where they can.
    pub cookies: Option<Cookies>,
}

/// The session cookie's keys and attributes (requirement 5.5).
#[derive(Debug)]
pub struct Cookies {
    /// The key cookies are signed with.
    pub key: Key,
    /// The key before it, still accepted.
    pub previous: Option<Key>,
    /// Whether viewers reach the server over `https`: the cookie is then
    /// `Secure`.
    pub secure: bool,
}

/// Seconds since the Unix epoch.
fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// Where the server's figures are served, beside the channels' folders.
pub const STATS: &str = "stats.json";

/// The push WebSocket's path within a channel's folder.
pub const PUSH: &str = "push";
const T42_TYPE: &str = "application/x-teletext-t42";

/// Answers one request.
#[must_use]
pub fn answer(routes: &Routes, request: Request<Incoming>) -> Response<Whole> {
    let path = request.uri().path().trim_start_matches('/').to_owned();
    let (folder, file) = path.split_once('/').unwrap_or((path.as_str(), ""));
    count(&routes.stats.requests);
    if folder.is_empty() {
        return listing(routes);
    }
    if folder == STATS && file.is_empty() {
        let mut response = Response::new(Whole(Some(Bytes::from(routes.stats.document()))));
        response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return response;
    }
    let Some(hub) = routes.hubs.get(folder).cloned() else { return status(StatusCode::NOT_FOUND) };
    if file == PUSH {
        return upgrade(routes, hub, request);
    }
    let (content_type, body): (&str, Arc<[u8]>) = match file {
        "channel.json" => ("application/json", Arc::clone(&hub.descriptor)),
        "contents.txt" => ("text/plain", Arc::clone(&hub.contents.read().unwrap_or_else(PoisonError::into_inner))),
        "index.t42" => (T42_TYPE, hub.pages.bundle().into()),
        other => match hub.pages.file(other) {
            Some(resource) => (resource.content_type, resource.body.into()),
            None => return status(StatusCode::NOT_FOUND),
        },
    };
    let response = file_response(&request, content_type, &body);
    if response.status() == StatusCode::NOT_MODIFIED {
        count(&routes.stats.not_modified);
    }
    response
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
    let (session, set_cookie) = session_for(routes, &hub, &request);
    let pending = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        let Ok(upgraded) = pending.await else {
            hub.release(session);
            return;
        };
        // The raw socket: no HTTP buffers stay with the connection.
        let Ok(parts) = upgraded.downcast::<TokioIo<TcpStream>>() else {
            hub.release(session);
            return;
        };
        connection::serve(parts.io.into_inner(), &parts.read_buf, hub, session).await;
    });
    let mut response = status(StatusCode::SWITCHING_PROTOCOLS);
    let headers = response.headers_mut();
    if let Some(value) = set_cookie.and_then(|cookie| HeaderValue::from_str(&cookie).ok()) {
        headers.insert(SET_COOKIE, value);
    }
    headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert(CONNECTION, HeaderValue::from_static("Upgrade"));
    if let Ok(value) = HeaderValue::from_bytes(&accept) {
        headers.insert("sec-websocket-accept", value);
    }
    response
}

/// The session a push connection continues or starts, claimed for it,
/// and the cookie to set (requirement 5.5): the session its cookie names,
/// unless another connection has it; else a new one. A session another
/// connection has is not given a cookie, so the browser keeps the one it
/// has.
fn session_for(routes: &Routes, hub: &Hub, request: &Request<Incoming>) -> (SessionId, Option<String>) {
    let Some(cookies) = routes.cookies.as_ref() else { return (new_session(routes, hub), None) };
    let now = now_s();
    let keys: Vec<&Key> = std::iter::once(&cookies.key).chain(cookies.previous.as_ref()).collect();
    let named = request.headers().get_all(COOKIE).iter().find_map(|header| session_cookie::session_of(header.as_bytes(), &hub.folder, now, &keys));
    match named {
        Some(session) if hub.claim(session) => (session, Some(session_cookie::set_cookie(&cookies.key, &hub.folder, session, now, cookies.secure))),
        Some(_) => (new_session(routes, hub), None),
        None => {
            let session = new_session(routes, hub);
            (session, Some(session_cookie::set_cookie(&cookies.key, &hub.folder, session, now, cookies.secure)))
        }
    }
}

/// A session never given before, claimed.
fn new_session(routes: &Routes, hub: &Hub) -> SessionId {
    loop {
        let session = routes.sessions.fetch_add(1, Ordering::Relaxed);
        if hub.claim(session) {
            return session;
        }
    }
}

fn status(code: StatusCode) -> Response<Whole> {
    let mut response = Response::new(Whole(None));
    *response.status_mut() = code;
    response
}

/// FNV-1a over `bytes`, for an `ETag`: a change of content changes it.
fn fnv1a(bytes: &[u8]) -> u64 {
    // FNV-1a's 64-bit offset basis and prime (Fowler, Noll and Vo).
    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(BASIS, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(PRIME))
}
