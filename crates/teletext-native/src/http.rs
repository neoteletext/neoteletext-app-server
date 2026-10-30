//! The HTTP side on the viewers' address (DEC-1467): each channel's
//! generated `channel.json` and its push WebSocket's upgrade (RFC 6455
//! section 4.2.2), after which the connection is served on the raw socket.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use hyper::body::{Body, Bytes, Frame, Incoming, SizeHint};
use hyper::header::{CACHE_CONTROL, CONNECTION, CONTENT_TYPE, COOKIE, HeaderValue, SET_COOKIE, UPGRADE};
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

/// The server's channels by folder, and the next session's and
/// connection's numbers.
#[derive(Debug)]
pub struct Routes {
    /// The channels.
    pub hubs: BTreeMap<String, Hub>,
    /// Sessions are numbered from here, never reused, across restarts too
    /// (requirement 5.4: `session_cookie::first_session`).
    pub sessions: AtomicU64,
    /// Connections are numbered from 1, never reused while the server
    /// runs.
    pub connections: AtomicU64,
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

/// The push WebSocket's path within a channel's folder.
pub const PUSH: &str = "push";
/// The channel's one file (DEC-1467 item 1).
pub const DESCRIPTOR: &str = "channel.json";

/// Answers one request on the viewers' address (DEC-1467): a channel's
/// `channel.json` and its push WebSocket, and 404 for everything else, `/`
/// included. The server serves no channel's files: every page comes over
/// the connection.
#[must_use]
pub fn answer(routes: &Routes, request: Request<Incoming>) -> Response<Whole> {
    let path = request.uri().path().trim_start_matches('/').to_owned();
    let (folder, file) = path.split_once('/').unwrap_or((path.as_str(), ""));
    count(&routes.stats.requests);
    let Some(hub) = routes.hubs.get(folder).cloned() else { return status(StatusCode::NOT_FOUND) };
    match file {
        PUSH => upgrade(routes, hub, request),
        DESCRIPTOR => {
            let mut response = Response::new(Whole(Some(Bytes::copy_from_slice(&hub.descriptor))));
            response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            response
        }
        _ => status(StatusCode::NOT_FOUND),
    }
}

/// The push WebSocket: answers 101 and serves the upgraded socket.
fn upgrade(routes: &Routes, hub: Hub, mut request: Request<Incoming>) -> Response<Whole> {
    let wants = request.headers().get(UPGRADE).is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let Some(key) = request.headers().get("sec-websocket-key").map(|value| value.as_bytes().to_vec()).filter(|_| wants) else {
        return status(StatusCode::UPGRADE_REQUIRED);
    };
    let accept = neoteletext_web::ws::accept_for(&key);
    let (session, set_cookie) = session_for(routes, &hub, &request);
    let connection = routes.connections.fetch_add(1, Ordering::Relaxed);
    let pending = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        let Ok(upgraded) = pending.await else { return };
        // The raw socket: no HTTP buffers stay with the connection.
        let Ok(parts) = upgraded.downcast::<TokioIo<TcpStream>>() else { return };
        connection::serve(parts.io.into_inner(), &parts.read_buf, hub, connection, session).await;
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

/// The session a push connection continues or starts, and the cookie to
/// set (requirement 5.5): the session its cookie names, else a new one.
/// Several connections, one a tab, may share a session, as a web server's
/// tabs of one site do (DEC-1527).
fn session_for(routes: &Routes, hub: &Hub, request: &Request<Incoming>) -> (SessionId, Option<String>) {
    let new = || routes.sessions.fetch_add(1, Ordering::Relaxed);
    let Some(cookies) = routes.cookies.as_ref() else { return (new(), None) };
    let now = now_s();
    let keys: Vec<&Key> = std::iter::once(&cookies.key).chain(cookies.previous.as_ref()).collect();
    let named = request.headers().get_all(COOKIE).iter().find_map(|header| session_cookie::session_of(header.as_bytes(), &hub.folder, now, &keys));
    let session = named.unwrap_or_else(new);
    (session, Some(session_cookie::set_cookie(&cookies.key, &hub.folder, session, now, cookies.secure)))
}

fn status(code: StatusCode) -> Response<Whole> {
    let mut response = Response::new(Whole(None));
    *response.status_mut() = code;
    response
}
