//! The HTTP side on the viewers' address (DEC-1467, REQ-1693 item 7): each
//! channel's small page, the redirect to it, its generated `channel.ttx`
//! and its push WebSocket's upgrade (RFC 6455 section 4.2.2), after which
//! the connection is served on the raw socket.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use hyper::body::{Body, Bytes, Frame, Incoming, SizeHint};
use hyper::header::{
    AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HeaderValue, LOCATION, ORIGIN, REFERRER_POLICY, SEC_WEBSOCKET_PROTOCOL,
    SEC_WEBSOCKET_VERSION, UPGRADE, X_CONTENT_TYPE_OPTIONS,
};
use hyper::{Method, Request, Response, StatusCode, Version};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio::sync::oneshot;

use neoteletext_web::consts::PUSH_SUBPROTOCOL;
use teletext_runtime::landing::{self, PushRefusal, Route, push_upgrade, route};
use teletext_runtime::protocol::PageNumber;

use crate::connection;
use crate::hub::{Command, Hub};
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

/// The server's channels by folder, and the next connection's number.
#[derive(Debug)]
pub struct Routes {
    /// The channels.
    pub hubs: BTreeMap<String, Hub>,
    /// Connections are numbered from 1, never reused while the server
    /// runs.
    pub connections: AtomicU64,
    /// The server's figures.
    pub stats: Arc<ServerStats>,
    /// The proxy in front of the server, whose `X-Forwarded-For` names the
    /// viewer's address (VS-304); none trusted when there is none.
    pub trusted_proxy: Option<IpAddr>,
}

/// The header a reverse proxy names the client's address in, appending it
/// to what it received (the de facto form RFC 7239 section 1 describes).
const FORWARDED_FOR: &str = "x-forwarded-for";

/// The viewer's network address (VS-304): the peer's, or, when the peer is
/// the trusted proxy, the last address its `X-Forwarded-For` names, the
/// one it appended itself. The rest of the header is the viewer's own
/// word, and is never read.
fn viewer_address(routes: &Routes, peer: Option<IpAddr>, request: &Request<Incoming>) -> Option<IpAddr> {
    if peer.is_none() || peer != routes.trusted_proxy {
        return peer;
    }
    let forwarded = request.headers().get_all(FORWARDED_FOR).iter().next_back()?.to_str().ok()?;
    forwarded.rsplit(',').next()?.trim().parse().ok()
}

/// Answers one request on the viewers' address (REQ-1693 item 7), and 404
/// for everything else, `/` included. The server serves no channel's files:
/// every page comes over the connection.
pub async fn answer(routes: &Routes, request: Request<Incoming>, peer: Option<IpAddr>) -> Response<Whole> {
    let (folder, asked) = route(request.uri().path());
    count(&routes.stats.requests);
    let Some(hub) = routes.hubs.get(folder).cloned() else { return status(StatusCode::NOT_FOUND) };
    let reads = matches!(*request.method(), Method::GET | Method::HEAD);
    match asked {
        Route::Page if reads => page(&hub, &landing::tail(request.uri().path(), request.uri().query())),
        Route::Described(number) if reads => described(&hub, number, &landing::tail(request.uri().path(), request.uri().query())).await,
        Route::ToFolder if reads => to_folder(&hub, request.uri().query()),
        Route::ToPlace if reads => to_place(request.uri().path(), request.uri().query()),
        Route::Descriptor => {
            let mut response = Response::new(Whole(Some(Bytes::copy_from_slice(&hub.descriptor))));
            response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            response
        }
        Route::Push => {
            let address = viewer_address(routes, peer, &request);
            upgrade(routes, hub, request, address)
        }
        Route::Program => program_upgrade(hub, request),
        Route::Page | Route::Described(_) | Route::ToFolder | Route::ToPlace | Route::Missing => status(StatusCode::NOT_FOUND),
    }
}

/// The small page a web browser shows at an address of the channel
/// (DEC-1669), naming what was asked for.
fn page(hub: &Hub, tail: &str) -> Response<Whole> {
    html(landing::page(&hub.name, &format!("{}{tail}", hub.address)), landing::CONTENT_SECURITY_POLICY)
}

/// What a page's address answers (REQ-1728). The channel's task answers at
/// once or within its bound, and a task that has gone means no answer: the
/// page is then not public. The answer is the same whoever asks: no request
/// header reaches here.
async fn described(hub: &Hub, number: PageNumber, tail: &str) -> Response<Whole> {
    let (asked, answered) = oneshot::channel();
    let answer = match hub.commands.try_send(Command::Describe { page: number, answer: asked }) {
        Ok(()) => answered.await.unwrap_or_default(),
        Err(_) => None,
    };
    let (body, policy) = landing::described(&hub.name, &hub.address, tail, number, &answer);
    html(body, policy)
}

/// A page of HTML, under `policy`.
fn html(body: String, policy: &'static str) -> Response<Whole> {
    let mut response = Response::new(Whole(Some(Bytes::from(body))));
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(policy));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    response
}

/// The folder without its final `/`, sent to `<channel>`. A fragment stays
/// with the viewer's browser (RFC 9110 section 10.2.2).
fn to_folder(hub: &Hub, query: Option<&str>) -> Response<Whole> {
    let Ok(location) = HeaderValue::from_str(&landing::folder_location(&hub.folder, query)) else { return status(StatusCode::NOT_FOUND) };
    let mut response = status(StatusCode::PERMANENT_REDIRECT);
    response.headers_mut().insert(LOCATION, location);
    response
}

/// A place with a final `/`, sent to its one spelling.
fn to_place(path: &str, query: Option<&str>) -> Response<Whole> {
    let Ok(location) = HeaderValue::from_str(&landing::place_location(path, query)) else {
        return status(StatusCode::BAD_REQUEST);
    };
    let mut response = status(StatusCode::PERMANENT_REDIRECT);
    response.headers_mut().insert(LOCATION, location);
    response
}

/// The answer to an upgrade [`push_upgrade`] refused, before anything of
/// the request is looked at: an upgrade to the wrong thing says what is spoken.
fn refused_upgrade(refusal: PushRefusal) -> Response<Whole> {
    match refusal {
        PushRefusal::Origin => status(StatusCode::FORBIDDEN),
        PushRefusal::Subprotocol => {
            let mut response = status(StatusCode::UPGRADE_REQUIRED);
            let headers = response.headers_mut();
            headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
            headers.insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static(PUSH_SUBPROTOCOL));
            response
        }
    }
}

/// The key of a WebSocket opening handshake, or the answer to a request
/// that is not one.
fn handshake(request: &Request<Incoming>) -> Result<Vec<u8>, Box<Response<Whole>>> {
    // RFC 6455 section 4.2.1: the opening handshake is a GET of HTTP/1.1 or
    // later; anything else is not one, and is answered 400. Neither the push
    // address (DOC-1917) nor the program's takes a query, so one with a
    // query is not it either.
    if request.method() != Method::GET || request.version() < Version::HTTP_11 || request.uri().query().is_some() {
        return Err(Box::new(status(StatusCode::BAD_REQUEST)));
    }
    // Section 4.2.2 item 4: another WebSocket version is answered 426 with
    // the version spoken.
    let version = request.headers().get(SEC_WEBSOCKET_VERSION).map(HeaderValue::as_bytes);
    if version != Some(neoteletext_web::consts::WEBSOCKET_VERSION) {
        let mut response = status(StatusCode::UPGRADE_REQUIRED);
        if let Ok(spoken) = HeaderValue::from_bytes(neoteletext_web::consts::WEBSOCKET_VERSION) {
            response.headers_mut().insert(SEC_WEBSOCKET_VERSION, spoken);
        }
        return Err(Box::new(response));
    }
    let wants = request.headers().get(UPGRADE).is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    request
        .headers()
        .get("sec-websocket-key")
        .map(|value| value.as_bytes().to_vec())
        .filter(|_| wants)
        .ok_or_else(|| Box::new(status(StatusCode::UPGRADE_REQUIRED)))
}

/// The answer 101 to a handshake whose key is `key` (RFC 6455 section
/// 4.2.2 item 5).
fn switching(key: &[u8]) -> Response<Whole> {
    let mut response = status(StatusCode::SWITCHING_PROTOCOLS);
    let headers = response.headers_mut();
    headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert(CONNECTION, HeaderValue::from_static("Upgrade"));
    if let Ok(value) = HeaderValue::from_bytes(&neoteletext_web::ws::accept_for(key)) {
        headers.insert("sec-websocket-accept", value);
    }
    response
}

/// The channel's program's WebSocket (REQ-1044): 404 for a channel whose
/// program does not connect this way; 401 without the channel's secret,
/// before anything else of the request is looked at, so that nothing is
/// told to whoever lacks it, as the Cloudflare host's Worker answers
/// (REQ-1024); 409 while a program is connected, one at a time; else 101,
/// and the upgraded socket goes to the channel's program task.
fn program_upgrade(hub: Hub, mut request: Request<Incoming>) -> Response<Whole> {
    let Some(door) = hub.door else { return status(StatusCode::NOT_FOUND) };
    if !door.admits(request.headers().get(AUTHORIZATION).map(HeaderValue::as_bytes)) {
        tracing::warn!(target: "program", channel = hub.folder, "a program without the channel's secret was refused");
        return status(StatusCode::UNAUTHORIZED);
    }
    let key = match handshake(&request) {
        Ok(key) => key,
        Err(response) => return *response,
    };
    let Some(claim) = door.claim() else {
        tracing::warn!(target: "program", channel = hub.folder, "a second program was refused: one at a time");
        return status(StatusCode::CONFLICT);
    };
    let pending = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        // An upgrade that fails drops the claim, which lets the next in.
        let Ok(upgraded) = pending.await else { return };
        let Ok(parts) = upgraded.downcast::<TokioIo<TcpStream>>() else { return };
        door.arrive(parts.io.into_inner(), parts.read_buf.to_vec(), claim).await;
    });
    switching(&key)
}

/// The push WebSocket: answers 101 and serves the upgraded socket. No
/// `Cookie` is read and no `Set-Cookie` sent (VS-201, VS-354).
fn upgrade(routes: &Routes, hub: Hub, mut request: Request<Incoming>, address: Option<IpAddr>) -> Response<Whole> {
    let key = match handshake(&request) {
        Ok(key) => key,
        Err(response) => return *response,
    };
    let offered = request.headers().get_all(SEC_WEBSOCKET_PROTOCOL).iter().map(HeaderValue::as_bytes);
    // The subprotocol answered names the version the connection speaks.
    let subprotocol = match push_upgrade(request.headers().contains_key(ORIGIN), offered) {
        Ok(subprotocol) => subprotocol,
        Err(refusal) => return refused_upgrade(refusal),
    };
    let connection = routes.connections.fetch_add(1, Ordering::Relaxed);
    let pending = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        let Ok(upgraded) = pending.await else { return };
        // The raw socket: no HTTP buffers stay with the connection.
        let Ok(parts) = upgraded.downcast::<TokioIo<TcpStream>>() else { return };
        connection::serve(parts.io.into_inner(), &parts.read_buf, hub, connection, address).await;
    });
    let mut response = switching(&key);
    response.headers_mut().insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static(subprotocol));
    response
}

fn status(code: StatusCode) -> Response<Whole> {
    let mut response = Response::new(Whole(None));
    *response.status_mut() = code;
    response
}
