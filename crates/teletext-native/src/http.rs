//! The HTTP side on the viewers' address (DEC-1467, REQ-1693 item 7): each
//! channel's small page, the redirect to it, its generated `channel.ttx`
//! and its push WebSocket's opening, after which the connection is served
//! on what the opening left: the raw socket after an HTTP/1.1 upgrade (RFC
//! 6455 section 4.2.2), or the stream of an extended CONNECT over HTTP/2
//! (RFC 8441) or HTTP/3 (RFC 9220), which REQ-6966 has checked exactly as
//! the upgrade is. [`decide`] is the one place a request is checked,
//! whichever version of HTTP carried it.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use hyper::body::{Body, Bytes, Frame, SizeHint};
use hyper::ext::Protocol;
use hyper::header::{
    AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HeaderValue, LOCATION, ORIGIN, REFERRER_POLICY, SEC_WEBSOCKET_PROTOCOL,
    SEC_WEBSOCKET_VERSION, UPGRADE, X_CONTENT_TYPE_OPTIONS,
};
use hyper::{Method, Request, Response, StatusCode, Version};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, oneshot};

use neoteletext_web::consts::PUSH_SUBPROTOCOL;
use teletext_runtime::ConnectionId;
use teletext_runtime::landing::{self, PushRefusal, Route, push_upgrade, route};
use teletext_runtime::protocol::PageNumber;

use crate::apps::{Claim, Door};
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

    /// Its bytes, for a host that writes a body itself.
    #[must_use]
    pub fn into_bytes(self) -> Bytes {
        self.0.unwrap_or_default()
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

/// The address a request comes with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// The address of the TCP or QUIC connection's peer: the viewer's,
    /// unless the peer is the trusted proxy, whose `X-Forwarded-For` then
    /// names the viewer's.
    Peer(IpAddr),
    /// The viewer's address as the PROXY protocol header of the request's
    /// connection gave it (REQ-6974 item 9). `X-Forwarded-For` is not
    /// read: the proxy relayed the request encrypted and wrote none of
    /// it, so the field is the viewer's own word, whatever the address.
    Viewer(IpAddr),
}

/// The viewer's network address (VS-304): the peer's, or, when the peer is
/// the trusted proxy, the last address its `X-Forwarded-For` names, the
/// one it appended itself. The rest of the header is the viewer's own
/// word, and is never read. Behind the PROXY protocol it is the address
/// the connection's header gave.
fn viewer_address<B>(routes: &Routes, source: Source, request: &Request<B>) -> Option<IpAddr> {
    let peer = match source {
        Source::Viewer(viewer) => return Some(viewer),
        Source::Peer(peer) => peer,
    };
    if Some(peer) != routes.trusted_proxy {
        return Some(peer);
    }
    let forwarded = request.headers().get_all(FORWARDED_FOR).iter().next_back()?.to_str().ok()?;
    forwarded.rsplit(',').next()?.trim().parse().ok()
}

/// What a WebSocket's `:protocol` says (RFC 8441 section 4, RFC 9220
/// section 3), the token HTTP/1.1's `Upgrade` says too.
const WEBSOCKET: &str = "websocket";

/// What [`decide`] made of a request.
#[derive(Debug)]
pub enum Answer {
    /// This response and nothing more.
    Response(Response<Whole>),
    /// The channel's push WebSocket was opened: after `response`, what
    /// carried the request is `connection`, to be served for `hub` with
    /// [`connection::serve_halves`].
    Push {
        /// 101 to an HTTP/1.1 upgrade, 200 to an extended CONNECT.
        response: Response<Whole>,
        /// The channel.
        hub: Hub,
        /// The connection's number.
        connection: ConnectionId,
        /// The viewer's network address (VS-304).
        address: Option<IpAddr>,
    },
    /// The channel's program's WebSocket was opened: after `response`,
    /// what carried the request goes in by `door`, under `claim`.
    Program {
        /// 101 to an HTTP/1.1 upgrade, 200 to an extended CONNECT.
        response: Response<Whole>,
        /// The way in to the channel's program task.
        door: Door,
        /// What keeps a second program out meanwhile.
        claim: Claim,
    },
}

/// Answers one request on the viewers' address over HTTP/1.1 or HTTP/2,
/// and serves the WebSocket it opened, if it opened one, on what hyper
/// makes of the connection or the stream.
pub async fn answer<B: Send + 'static>(routes: &Routes, request: Request<B>, source: Source) -> Response<Whole> {
    // An upgraded socket keeps its TCP connection's place for as long as
    // it is served (REQ-7004), after hyper has let the connection go.
    let place = request.extensions().get::<TcpPlace>().cloned();
    match decide(routes, &request, source).await {
        Answer::Response(response) => response,
        Answer::Push { response, hub, connection, address } => {
            let pending = hyper::upgrade::on(request);
            tokio::spawn(async move {
                let _place = place;
                let Ok(upgraded) = pending.await else { return };
                match upgraded.downcast::<TokioIo<TcpStream>>() {
                    // HTTP/1.1 with no TLS: the raw socket, so that no
                    // HTTP buffers stay with the connection.
                    Ok(parts) => connection::serve(parts.io.into_inner(), &parts.read_buf, hub, connection, address).await,
                    // A TLS session, or a stream of an HTTP/2 connection.
                    Err(upgraded) => {
                        let (reading, writing) = tokio::io::split(TokioIo::new(upgraded));
                        connection::serve_halves(reading, writing, &[], hub, connection, address).await;
                    }
                }
            });
            response
        }
        Answer::Program { response, door, claim } => {
            let pending = hyper::upgrade::on(request);
            tokio::spawn(async move {
                let _place = place;
                // An upgrade that fails drops the claim, which lets the next in.
                let Ok(upgraded) = pending.await else { return };
                match upgraded.downcast::<TokioIo<TcpStream>>() {
                    Ok(parts) => {
                        let (reading, writing) = parts.io.into_inner().into_split();
                        door.arrive(Box::new(reading), Box::new(writing), parts.read_buf.to_vec(), claim).await;
                    }
                    Err(upgraded) => {
                        let (reading, writing) = tokio::io::split(TokioIo::new(upgraded));
                        door.arrive(Box::new(reading), Box::new(writing), Vec::new(), claim).await;
                    }
                }
            });
            response
        }
    }
}

/// A TCP connection's place among `MAX_TCP_CONNECTIONS` (REQ-7004), which
/// each of its requests carries, so that a WebSocket it was upgraded to
/// holds the place until that WebSocket ends.
#[derive(Clone, Debug)]
pub struct TcpPlace(pub Arc<OwnedSemaphorePermit>);

/// Decides one request on the viewers' address (REQ-1693 item 7), and 404
/// for everything else, `/` included. The server serves no channel's files:
/// every page comes over the connection. The request's body is never read,
/// so any version of HTTP's request is decided here (REQ-6966).
pub async fn decide<B>(routes: &Routes, request: &Request<B>, source: Source) -> Answer {
    count(&routes.stats.requests);
    let connect = *request.method() == Method::CONNECT;
    // RFC 9220 section 3, RFC 8441 section 5: a `:protocol` the server
    // does not speak is answered 501, whatever it was asked of.
    if connect && request.extensions().get::<Protocol>().is_some_and(|protocol| !protocol.as_str().eq_ignore_ascii_case(WEBSOCKET)) {
        return Answer::Response(status(StatusCode::NOT_IMPLEMENTED));
    }
    let (folder, asked) = route(request.uri().path());
    let Some(hub) = routes.hubs.get(folder).cloned() else { return Answer::Response(status(StatusCode::NOT_FOUND)) };
    let reads = matches!(*request.method(), Method::GET | Method::HEAD);
    Answer::Response(match asked {
        Route::Push => return upgrade(routes, hub, request, viewer_address(routes, source, request)),
        Route::Program => return program_upgrade(hub, request),
        // A CONNECT's 2xx answer would open a tunnel, and only the two
        // routes above have one to open.
        _ if connect => status(StatusCode::NOT_FOUND),
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
        Route::Page | Route::Described(_) | Route::ToFolder | Route::ToPlace | Route::Missing => status(StatusCode::NOT_FOUND),
    })
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

/// How a WebSocket was asked for.
enum Opening {
    /// By an HTTP/1.1 upgrade, with this key (RFC 6455 section 4.1).
    Upgrade(Vec<u8>),
    /// By an extended CONNECT over HTTP/2 (RFC 8441 section 4) or HTTP/3
    /// (RFC 9220 section 3).
    Connect,
}

/// The answer to an upgrade [`push_upgrade`] refused, before anything of
/// the request is looked at: an upgrade to the wrong thing says what is
/// spoken. `Upgrade` is a field of an HTTP/1.1 connection, which HTTP/2
/// and HTTP/3 do not carry (RFC 9113 section 8.2.2, RFC 9114 section 4.2).
fn refused_upgrade(refusal: PushRefusal, opening: &Opening) -> Response<Whole> {
    match refusal {
        PushRefusal::Origin => status(StatusCode::FORBIDDEN),
        PushRefusal::Subprotocol => {
            let mut response = status(StatusCode::UPGRADE_REQUIRED);
            let headers = response.headers_mut();
            if matches!(opening, Opening::Upgrade(_)) {
                headers.insert(UPGRADE, HeaderValue::from_static(WEBSOCKET));
            }
            headers.insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static(PUSH_SUBPROTOCOL));
            response
        }
    }
}

/// How a WebSocket's opening handshake asks for it, or the answer to a
/// request that is not one.
fn handshake<B>(request: &Request<B>) -> Result<Opening, Box<Response<Whole>>> {
    // Over HTTP/2 and HTTP/3 the handshake is an extended CONNECT whose
    // `:protocol` is `websocket` (RFC 8441 section 4, RFC 9220 section 3);
    // `decide` has answered any other `:protocol`. Over HTTP/1.1 it is a
    // GET (RFC 6455 section 4.2.1). Anything else is not one, and is
    // answered 400. Neither the push address (DOC-1917) nor the program's
    // takes a query, so one with a query is not it either.
    let multiplexed = request.version() >= Version::HTTP_2;
    let asked =
        if multiplexed { request.method() == Method::CONNECT && request.extensions().get::<Protocol>().is_some() } else { request.method() == Method::GET };
    if !asked || request.version() < Version::HTTP_11 || request.uri().query().is_some() {
        return Err(Box::new(status(StatusCode::BAD_REQUEST)));
    }
    // RFC 6455 section 4.2.2 item 4, which RFC 8441 section 5 keeps:
    // another WebSocket version is answered 426 with the version spoken.
    let version = request.headers().get(SEC_WEBSOCKET_VERSION).map(HeaderValue::as_bytes);
    if version != Some(neoteletext_web::consts::WEBSOCKET_VERSION) {
        let mut response = status(StatusCode::UPGRADE_REQUIRED);
        if let Ok(spoken) = HeaderValue::from_bytes(neoteletext_web::consts::WEBSOCKET_VERSION) {
            response.headers_mut().insert(SEC_WEBSOCKET_VERSION, spoken);
        }
        return Err(Box::new(response));
    }
    if multiplexed {
        // RFC 8441 section 5: no `Upgrade`, `Connection` or key.
        return Ok(Opening::Connect);
    }
    let wants = request.headers().get(UPGRADE).is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(WEBSOCKET.as_bytes()));
    request
        .headers()
        .get("sec-websocket-key")
        .map(|value| Opening::Upgrade(value.as_bytes().to_vec()))
        .filter(|_| wants)
        .ok_or_else(|| Box::new(status(StatusCode::UPGRADE_REQUIRED)))
}

/// The answer that opens the WebSocket: 101 to a handshake with a key
/// (RFC 6455 section 4.2.2 item 5), and 200 to an extended CONNECT, with
/// no `Sec-WebSocket-Accept` (RFC 8441 section 5).
fn switching(opening: &Opening) -> Response<Whole> {
    let Opening::Upgrade(key) = opening else { return status(StatusCode::OK) };
    let mut response = status(StatusCode::SWITCHING_PROTOCOLS);
    let headers = response.headers_mut();
    headers.insert(UPGRADE, HeaderValue::from_static(WEBSOCKET));
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
/// or 200 to an extended CONNECT, and what carried the request goes to
/// the channel's program task.
fn program_upgrade<B>(hub: Hub, request: &Request<B>) -> Answer {
    let Some(door) = hub.door else { return Answer::Response(status(StatusCode::NOT_FOUND)) };
    if !door.admits(request.headers().get(AUTHORIZATION).map(HeaderValue::as_bytes)) {
        tracing::warn!(target: "program", channel = hub.folder, "a program without the channel's secret was refused");
        return Answer::Response(status(StatusCode::UNAUTHORIZED));
    }
    let opening = match handshake(request) {
        Ok(opening) => opening,
        Err(response) => return Answer::Response(*response),
    };
    let Some(claim) = door.claim() else {
        tracing::warn!(target: "program", channel = hub.folder, "a second program was refused: one at a time");
        return Answer::Response(status(StatusCode::CONFLICT));
    };
    Answer::Program { response: switching(&opening), door, claim }
}

/// The push WebSocket: answers 101, or 200 to an extended CONNECT, for
/// the connection then to be served. No `Cookie` is read and no
/// `Set-Cookie` sent (VS-201, VS-354).
fn upgrade<B>(routes: &Routes, hub: Hub, request: &Request<B>, address: Option<IpAddr>) -> Answer {
    let opening = match handshake(request) {
        Ok(opening) => opening,
        Err(response) => return Answer::Response(*response),
    };
    let offered = request.headers().get_all(SEC_WEBSOCKET_PROTOCOL).iter().map(HeaderValue::as_bytes);
    if let Err(refusal) = push_upgrade(request.headers().contains_key(ORIGIN), offered) {
        return Answer::Response(refused_upgrade(refusal, &opening));
    }
    let connection = routes.connections.fetch_add(1, Ordering::Relaxed);
    let mut response = switching(&opening);
    response.headers_mut().insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static(PUSH_SUBPROTOCOL));
    Answer::Push { response, hub, connection, address }
}

fn status(code: StatusCode) -> Response<Whole> {
    let mut response = Response::new(Whole(None));
    *response.status_mut() = code;
    response
}
