//! The viewers' address, served (REQ-6965): plain HTTP/1.1 on TCP for a
//! proxy in front, as before; or, given a certificate and key, TLS on TCP
//! with HTTP/2 or HTTP/1.1 as the client chooses, and HTTP/3 over QUIC on
//! UDP at the same address and port, which every answer over TCP then
//! names in `Alt-Svc` at the port viewers reach the server at. Behind a
//! proxy that relays TLS without ending it, each TCP connection first
//! gives a PROXY protocol header, which names the viewer's address
//! (REQ-6974).

use std::convert::Infallible;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use hyper::Uri;
use hyper::header::{ALT_SVC, HeaderValue};
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, watch};
use tokio_rustls::TlsAcceptor;

use crate::http::{Routes, Source, TcpPlace, answer};
use crate::limits::{
    H2_DEAD_AFTER_PING, H2_MAX_FRAME_BYTES, H2_PING_AFTER, HANDSHAKE_WITHIN, MAX_CONCURRENT_STREAMS, MAX_HEADER_LIST_BYTES, MAX_PROXY_HEADER_BYTES,
    MAX_QUIC_CONNECTIONS, MAX_TCP_CONNECTIONS, MAX_TLS_HANDSHAKES, PROXY_HEADER_WITHIN,
};
use crate::proxy_protocol::{self, Header, Refused};
use crate::tls::{ALPN_H2, Configured};
use crate::{http3, tls};
use teletext_runtime::HandleSecret;

/// TLS on TCP, and what answers over it say of HTTP/3.
#[derive(Clone)]
struct Secured {
    acceptor: TlsAcceptor,
    /// `Alt-Svc: h3=":<port>"` (RFC 7838 section 3), the port viewers
    /// reach QUIC at ([`Direct::public_port`]).
    alt_svc: HeaderValue,
    /// [`Direct::proxy_protocol`].
    proxy: Option<IpAddr>,
}

/// What the server serves TLS with, and where viewers reach it.
#[derive(Clone, Copy, Debug)]
pub struct Direct<'tls> {
    /// The certificate and key, read.
    pub tls: &'tls Configured,
    /// The port viewers reach the server at, which `Alt-Svc` names
    /// (REQ-6965): [`public_port`] of the server's origin. It is the
    /// listening port only when nothing relays connections to the server
    /// from another port (DEC-6972).
    pub public_port: u16,
    /// The proxy that relays viewers' TCP connections still encrypted,
    /// when there is one (REQ-6974): every TCP connection must then come
    /// from this address and start with a PROXY protocol version 2
    /// header, read before TLS, which names the viewer's address. The
    /// specification's section 2: "The receiver MUST be configured to
    /// only receive the protocol described in this specification and MUST
    /// not try to guess whether the protocol header is present or not".
    pub proxy_protocol: Option<IpAddr>,
    /// The secret QUIC's stateless reset tokens are derived from
    /// (REQ-7002; RFC 9000 section 10.3): the server's handle secret,
    /// which a state directory keeps across restarts, so that the next
    /// process can reset a connection the last one held. `None` makes a
    /// key for this run alone.
    pub reset_secret: Option<&'tls HandleSecret>,
}

/// The default port of `http` (RFC 9110 section 4.2.1).
const HTTP_PORT: u16 = 80;
/// The default port of `https` (RFC 9110 section 4.2.2).
const HTTPS_PORT: u16 = 443;

/// The port viewers reach the server at (REQ-6965): the one `origin`
/// names, or its scheme's default when it names none.
///
/// # Errors
/// An origin that is not an `http` or `https` address, naming it.
pub fn public_port(origin: &str) -> Result<u16, String> {
    let address: Uri = origin.parse().map_err(|error| format!("cannot read the public port from the origin {origin}: {error}"))?;
    let default = match address.scheme_str() {
        Some("https") => HTTPS_PORT,
        Some("http") => HTTP_PORT,
        _ => return Err(format!("cannot read the public port from the origin {origin}: it must start http:// or https://")),
    };
    match address.authority().map(|authority| (authority.port_u16(), authority.as_str() == authority.host())) {
        Some((Some(port), _)) => Ok(port),
        // The host alone: no port is named.
        Some((None, true)) => Ok(default),
        // More than the host, and not a port: one over 65,535, or a user.
        _ => Err(format!("cannot read the public port from the origin {origin}: what follows its host is not a port")),
    }
}

/// How many TCP connections, and TLS handshakes among them, the server
/// serves at once: [`MAX_TCP_CONNECTIONS`] and [`MAX_TLS_HANDSHAKES`], or
/// smaller figures a test can reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcpBounds {
    /// TCP connections at once; one more is closed when it is accepted.
    pub connections: usize,
    /// TLS handshakes in progress at once; a connection that would be one
    /// more is closed before its handshake is read.
    pub handshakes: usize,
}

impl TcpBounds {
    /// The server's own bounds.
    pub const SERVED: Self = Self { connections: MAX_TCP_CONNECTIONS, handshakes: MAX_TLS_HANDSHAKES };
}

/// The viewers' address, listening.
pub struct Server {
    listener: TcpListener,
    secured: Option<Secured>,
    /// The QUIC endpoints, one a socket on the one port; none without TLS.
    quic: Vec<quinn::Endpoint>,
    /// What the system batches of QUIC's datagrams: the segments one send
    /// and one receive may carry.
    offload: Option<(usize, usize)>,
    /// The bytes the system holds of QUIC's received datagrams.
    receive_buffer: Option<usize>,
    routes: Arc<Routes>,
    bounds: TcpBounds,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Server").field("listener", &self.listener).field("quic", &self.quic).finish_non_exhaustive()
    }
}

impl Server {
    /// Takes `listener`, and with `direct` binds UDP at its address and
    /// port for QUIC. Without `direct` no UDP socket is made.
    ///
    /// # Errors
    /// An address that cannot be read, or a UDP socket that cannot be
    /// bound there.
    pub fn start(listener: TcpListener, direct: Option<Direct<'_>>, routes: Arc<Routes>) -> Result<Self, String> {
        Self::start_bounded(listener, direct, routes, TcpBounds::SERVED)
    }

    /// [`Server::start`] with `bounds` in place of the server's own.
    ///
    /// # Errors
    /// As [`Server::start`].
    pub fn start_bounded(listener: TcpListener, direct: Option<Direct<'_>>, routes: Arc<Routes>, bounds: TcpBounds) -> Result<Self, String> {
        let address = listener.local_addr().map_err(|error| format!("cannot read the address listened on: {error}"))?;
        let Some(Direct { tls, public_port, proxy_protocol, reset_secret }) = direct else {
            return Ok(Self { listener, secured: None, quic: Vec::new(), offload: None, receive_buffer: None, routes, bounds });
        };
        let listening = http3::endpoint(tls, address, reset_secret)?;
        let alt_svc = HeaderValue::from_str(&format!("h3=\":{public_port}\"")).map_err(|error| format!("cannot name the QUIC port: {error}"))?;
        Ok(Self {
            listener,
            secured: Some(Secured { acceptor: TlsAcceptor::from(Arc::clone(&tls.tcp)), alt_svc, proxy: proxy_protocol.map(|proxy| proxy.to_canonical()) }),
            quic: listening.endpoints,
            offload: Some((listening.send_segments, listening.receive_segments)),
            receive_buffer: Some(listening.receive_buffer),
            routes,
            bounds,
        })
    }

    /// How many datagrams one send and one receive of QUIC's may carry,
    /// when QUIC is served: 1 each where the system batches none.
    #[must_use]
    pub const fn quic_offload(&self) -> Option<(usize, usize)> {
        self.offload
    }

    /// The bytes the system holds of QUIC's received datagrams until they
    /// are read, when QUIC is served.
    #[must_use]
    pub const fn quic_receive_buffer(&self) -> Option<usize> {
        self.receive_buffer
    }

    /// How many UDP sockets share the QUIC port, each with an endpoint of
    /// its own; none without TLS.
    #[must_use]
    pub fn quic_sockets(&self) -> usize {
        self.quic.len()
    }

    /// The UDP address QUIC is served on, when it is.
    #[must_use]
    pub fn quic_address(&self) -> Option<SocketAddr> {
        self.quic.first().and_then(|endpoint| endpoint.local_addr().ok())
    }

    /// Serves until `stopped` is ready, and then listens on TCP no more
    /// and tells every connection the server is going away: GOAWAY on
    /// HTTP/2 (RFC 9113 section 6.8) and HTTP/3 (RFC 9114 section 5.2), and
    /// on HTTP/1.1 a connection between requests is closed. Requests and
    /// WebSocket connections already open go on; what is returned ends QUIC's
    /// connections, once the channels have closed their own.
    pub async fn run(self, stopped: impl Future<Output = ()>) -> Stopped {
        let (stopping, told) = watch::channel(false);
        // One count of QUIC connections, however many sockets share the
        // port.
        let quic_places = Arc::new(Semaphore::new(MAX_QUIC_CONNECTIONS));
        for endpoint in &self.quic {
            tokio::spawn(http3::accept(endpoint.clone(), Arc::clone(&self.routes), told.clone(), Arc::clone(&quic_places)));
        }
        let open = Arc::new(Semaphore::new(self.bounds.connections));
        let handshakes = Arc::new(Semaphore::new(self.bounds.handshakes));
        tokio::pin!(stopped);
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let Ok((stream, peer)) = accepted else { continue };
                    let Ok(place) = Arc::clone(&open).try_acquire_owned() else {
                        // Closed unread; no address is logged (REQ-1049).
                        tracing::debug!(target: "serving", "closed a TCP connection: every place is taken");
                        continue;
                    };
                    let _ = stream.set_nodelay(true);
                    let (routes, secured, told, handshakes) = (Arc::clone(&self.routes), self.secured.clone(), told.clone(), Arc::clone(&handshakes));
                    tokio::spawn(connection(stream, peer, routes, secured, told, handshakes, TcpPlace(Arc::new(place))));
                }
                () = &mut stopped => break,
            }
        }
        stopping.send_replace(true);
        Stopped { quic: self.quic, _stopping: stopping }
    }
}

/// Ready once the server is stopping: once `told` says so, or its sender,
/// held until the server has stopped, is gone.
pub(crate) async fn until_stopping(mut told: watch::Receiver<bool>) {
    let _ = told.wait_for(|stopping| *stopping).await;
}

/// The viewers' address after it stopped listening on TCP.
#[derive(Debug)]
pub struct Stopped {
    quic: Vec<quinn::Endpoint>,
    /// What told every connection the server is going away, kept so that
    /// none takes its end for the stop again.
    _stopping: watch::Sender<bool>,
}

impl Stopped {
    /// Closes every QUIC connection with `H3_NO_ERROR`, waiting until each
    /// has been told, at most [`crate::limits::QUIC_CLOSED_WITHIN`].
    pub async fn close(self) {
        http3::close(&self.quic).await;
    }
}

/// Why a connection that had to start with a PROXY protocol header was
/// closed (REQ-6974 items 3 and 4).
enum Unheaded {
    /// Its peer is not the proxy.
    Peer,
    /// What it sent is not a header the server takes.
    Refused(Refused),
    /// It ended, or failed, before its header was whole.
    Ended,
    /// Its header was not whole within [`PROXY_HEADER_WITHIN`].
    Late,
}

impl std::fmt::Display for Unheaded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer => formatter.write_str("its peer is not the trusted proxy"),
            Self::Refused(refused) => write!(formatter, "its first bytes are not a header taken: {refused:?}"),
            Self::Ended => formatter.write_str("it ended before its header was whole"),
            Self::Late => formatter.write_str("its header was not whole in time"),
        }
    }
}

/// Reads the PROXY protocol header `stream` starts with, and no byte
/// past it: what follows is the TLS handshake's.
async fn proxy_header(stream: &mut TcpStream) -> Result<Header, Unheaded> {
    let mut bytes = [0_u8; MAX_PROXY_HEADER_BYTES];
    let mut come = 0_usize;
    loop {
        let needed = match proxy_protocol::read(bytes.get(..come).unwrap_or_default()) {
            Ok(proxy_protocol::Read::Whole { header, .. }) => return Ok(header),
            Ok(proxy_protocol::Read::More { needed }) => needed,
            Err(refused) => return Err(Unheaded::Refused(refused)),
        };
        // The reader asks for more than has come and never past the
        // header's end, so this reads into the header alone; anything
        // else would be its fault, and ends the connection.
        let wanted = bytes.get_mut(come..needed).filter(|wanted| !wanted.is_empty()).ok_or(Unheaded::Ended)?;
        match stream.read(wanted).await {
            Ok(count) if count > 0 => come = come.saturating_add(count),
            _ => return Err(Unheaded::Ended),
        }
    }
}

/// The viewer's address on a connection relayed by `proxy` (REQ-6974):
/// the source its PROXY protocol header names, or for the proxy's own
/// connection (LOCAL) the proxy's. `Err` when the connection is to be
/// closed, with nothing sent on it.
async fn relayed(stream: &mut TcpStream, peer: SocketAddr, proxy: IpAddr) -> Result<IpAddr, Unheaded> {
    // The specification's section 5: "accepting the header only from a
    // number of known sources should be safe". Nothing of anyone else's
    // connection is read.
    if peer.ip().to_canonical() != proxy {
        return Err(Unheaded::Peer);
    }
    match tokio::time::timeout(PROXY_HEADER_WITHIN, proxy_header(stream)).await {
        Ok(Ok(Header::Proxied(source))) => Ok(source.ip()),
        Ok(Ok(Header::Local)) => Ok(peer.ip()),
        Ok(Err(unheaded)) => Err(unheaded),
        Err(_) => Err(Unheaded::Late),
    }
}

/// Serves `$connection`, one of hyper's, until it ends; once the server is
/// stopping (`$told`), it is shut down gracefully (hyper's
/// `graceful_shutdown`: GOAWAY over HTTP/2, RFC 9113 section 6.8, and over
/// HTTP/1.1 no further request), and served until what it has open ends.
macro_rules! served {
    ($connection:expr, $told:expr) => {{
        let connection = $connection;
        tokio::pin!(connection);
        tokio::select! {
            _ = connection.as_mut() => {}
            () = until_stopping($told) => {
                connection.as_mut().graceful_shutdown();
                let _ = connection.await;
            }
        }
    }};
}

/// Serves one TCP connection: HTTP/1.1 as it is, or, under TLS, what the
/// client chose of HTTP/2 and HTTP/1.1 (RFC 7301), after the PROXY
/// protocol header when a proxy relays the connection.
async fn connection(
    mut stream: TcpStream,
    peer: SocketAddr,
    routes: Arc<Routes>,
    secured: Option<Secured>,
    told: watch::Receiver<bool>,
    handshakes: Arc<Semaphore>,
    place: TcpPlace,
) {
    let proxy = secured.as_ref().and_then(|secured| secured.proxy);
    let source = match proxy {
        None => Source::Peer(peer.ip()),
        Some(proxy) => match relayed(&mut stream, peer, proxy).await {
            Ok(viewer) => Source::Viewer(viewer),
            Err(unheaded) => {
                // No address: the log names no viewer (REQ-1049).
                tracing::debug!(target: "proxy_protocol", reason = %unheaded, "closed a connection with no PROXY protocol header taken");
                return;
            }
        },
    };
    let alt_svc = secured.as_ref().map(|secured| secured.alt_svc.clone());
    let service = service_fn(move |mut request: hyper::Request<hyper::body::Incoming>| {
        let routes = Arc::clone(&routes);
        let alt_svc = alt_svc.clone();
        request.extensions_mut().insert(place.clone());
        async move {
            let mut response = answer(&routes, request, source).await;
            if let Some(alt_svc) = alt_svc {
                response.headers_mut().insert(ALT_SVC, alt_svc);
            }
            Ok::<_, Infallible>(response)
        }
    });
    let Some(secured) = secured else {
        served!(http1::Builder::new().serve_connection(TokioIo::new(stream), service).with_upgrades(), told);
        return;
    };
    let Ok(handshaking) = handshakes.try_acquire_owned() else {
        tracing::debug!(target: "serving", "closed a TCP connection: every TLS handshake place is taken");
        return;
    };
    let Ok(Ok(session)) = tokio::time::timeout(HANDSHAKE_WITHIN, secured.acceptor.accept(stream)).await else { return };
    drop(handshaking);
    if session.get_ref().1.alpn_protocol() == Some(ALPN_H2) {
        let mut builder = http2::Builder::new(TokioExecutor::new());
        builder
            .timer(TokioTimer::new())
            // RFC 8441 section 3: SETTINGS_ENABLE_CONNECT_PROTOCOL = 1.
            .enable_connect_protocol()
            .max_concurrent_streams(MAX_CONCURRENT_STREAMS)
            .max_header_list_size(MAX_HEADER_LIST_BYTES)
            .max_frame_size(H2_MAX_FRAME_BYTES)
            .keep_alive_interval(H2_PING_AFTER)
            .keep_alive_timeout(H2_DEAD_AFTER_PING);
        served!(builder.serve_connection(TokioIo::new(session), service), told);
    } else {
        served!(http1::Builder::new().serve_connection(TokioIo::new(session), service).with_upgrades(), told);
    }
}

/// What `--tls-certificate` and `--tls-key` name, read: see [`tls::load`].
///
/// # Errors
/// As [`tls::load`].
pub fn secure(files: Option<&tls::Files>) -> Result<Option<Configured>, String> {
    files.map(tls::load).transpose()
}

#[cfg(test)]
mod tests {
    use super::public_port;

    /// REQ-6965: the public port is the origin's, or its scheme's default
    /// (RFC 9110 sections 4.2.1 and 4.2.2), whatever the host is written
    /// as; and text that is no `http` or `https` origin is refused, named.
    #[test]
    fn the_public_port_is_the_origins_or_its_schemes_default() {
        assert_eq!(public_port("https://app.neottx.com"), Ok(443));
        assert_eq!(public_port("http://app.neottx.com"), Ok(80));
        assert_eq!(public_port("https://app.neottx.com:9443"), Ok(9443));
        assert_eq!(public_port("http://127.0.0.1:8090"), Ok(8090));
        assert_eq!(public_port("https://[2001:db8::1]"), Ok(443));
        assert_eq!(public_port("https://[2001:db8::1]:8443"), Ok(8443));
        for refused in ["app.neottx.com:9443", "wss://app.neottx.com", "", "https://app.neottx.com:99999", "https://user@app.neottx.com"] {
            let error = public_port(refused).unwrap_err();
            assert!(error.starts_with(&format!("cannot read the public port from the origin {refused}: ")), "{error}");
        }
    }
}
