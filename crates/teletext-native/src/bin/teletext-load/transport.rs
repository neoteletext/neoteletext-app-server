//! How a load client's viewer reaches the server (DEC-6964, DEC-6979):
//! over HTTP/1.1, each tab its own TCP connection and WebSocket upgrade,
//! as before; over HTTP/2 (RFC 8441) or HTTP/3 (RFC 9220), one connection
//! a viewer, each tab a stream of it opened by extended CONNECT, as a
//! browser sharing one connection among a channel's tabs does. Whatever
//! carries it, a tab's WebSocket is read and written as bytes, its RFC
//! 6455 frames unchanged.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use h2::client::SendRequest;
use hyper::body::Bytes;
use neoteletext_http3::client::{self, Exchange, ExchangeEvent, Incoming, IncomingEvent, WebSocketRequest};
use neoteletext_http3::stream::{Opened, Role, open_client_control};
use neoteletext_web::consts::PUSH_SUBPROTOCOL;
use quinn::crypto::rustls::QuicClientConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::watch;
use tokio_rustls::TlsConnector;

/// HTTP/2's and HTTP/3's protocol identifiers for TLS (RFC 9113 section
/// 3.2, RFC 9114 section 3.1).
const ALPN_H2: &[u8] = b"h2";
/// HTTP/1.1's (RFC 7301 section 6).
const ALPN_HTTP1: &[u8] = b"http/1.1";
const ALPN_H3: &[u8] = b"h3";
/// The bytes read from a stream at a time.
const READ_BYTES: usize = 16 * 1024;
/// The UDP sockets a load client's QUIC connections are spread over: one
/// endpoint's packets are handled by one task, which at tens of thousands
/// of connections would be the client's bottleneck and not the server's.
const QUIC_CLIENT_SOCKETS: usize = 16;
/// The browser's QUIC timings (the engine's `neoteletext-transport`, its
/// REQ-6985), so that a load client's viewer gives up on a handshake, and
/// learns of a silent connection, as a browser's does: a handshake not
/// answered within five seconds is tried again, and a connection silent
/// for fifteen, pinged every five, is lost. quinn's own client waits
/// thirty seconds for either, so that a handshake sent while a server
/// restarts, to a port with no socket, would hold a viewer that long.
const CLIENT_HANDSHAKE_WITHIN: std::time::Duration = std::time::Duration::from_secs(5);
const CLIENT_IDLE_AFTER: std::time::Duration = std::time::Duration::from_secs(15);
const CLIENT_KEEP_ALIVE_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// The bytes of received datagrams each client socket asks the system to
/// hold. At Linux's default 208 KiB, the server's answers to 10,000
/// viewers' handshakes after a restart overflowed the sixteen sockets
/// (16,643 datagrams dropped, counted by `UdpRcvbufErrors` on the client's
/// machine, 21 November 2026), and the viewers waited out their
/// handshakes' timeouts: the load client, not the server, set the pace.
const QUIC_CLIENT_RECEIVE_BUFFER_BYTES: usize = 8 * 1024 * 1024;

/// What the load client names itself as (RFC 9110 section 10.1.5).
const USER_AGENT: &[u8] = b"teletext-load";
/// How long an HTTP/2 viewer waits for the server's SETTINGS, and how
/// often it looks: they come in the server's first flight, a round trip
/// after the connection.
const SETTINGS_WITHIN: std::time::Duration = std::time::Duration::from_secs(5);
const SETTINGS_LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(5);
/// The longest a 101 response's head is read for, so that what a server
/// sends is bounded.
const MOST_HEAD_BYTES: usize = 16 * 1024;

/// Which version of HTTP carries the tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    /// A TCP connection and an upgrade a tab, without TLS.
    Http1,
    /// One TLS connection a viewer, a stream a tab.
    Http2,
    /// One QUIC connection a viewer, a stream a tab.
    Http3,
}

/// A tab's WebSocket, as bytes each way.
pub trait Duplex: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Duplex for T {}

/// Where the tabs go and how.
pub struct Target {
    /// `host:port`, as given.
    pub host: String,
    /// The push route's path, from its `/`.
    pub path: String,
    pub version: Version,
    /// The server's address, resolved once.
    address: SocketAddr,
    /// The TLS name of the server: the URL's host.
    name: ServerName<'static>,
    /// TLS for HTTP/2.
    tls: Option<TlsConnector>,
    /// QUIC's client sockets, for HTTP/3.
    quic: Vec<quinn::Endpoint>,
    /// With `--source-ports`, the local ports TCP connections are bound
    /// to, the first and how many.
    ports: Option<(u16, u16)>,
    /// How many TCP connections have been tried, which picks each one's
    /// port in turn: a port another program holds, or one still waiting
    /// out an old connection, is then passed by on the next try rather
    /// than tried by the same tab for ever.
    tried: std::sync::atomic::AtomicUsize,
    /// With `--sources`, how many loopback addresses TCP connections come
    /// from.
    sources: usize,
}

/// The loopback network's first address, as its four bytes.
const LOOPBACK: [u8; 4] = [127, 0, 0, 1];

impl Target {
    /// What `--url`, `--http`, `--ca`, `--sources` and `--source-ports`
    /// name.
    ///
    /// # Errors
    /// What is wrong, named.
    pub async fn new(
        host: String,
        path: String,
        version: Version,
        secure: bool,
        ca: Option<&str>,
        sources: usize,
        ports: Option<(u16, u16)>,
    ) -> Result<Self, String> {
        let address = tokio::net::lookup_host(&host).await.map_err(|error| format!("{host}: {error}"))?.next().ok_or_else(|| format!("{host}: no address"))?;
        let bare = host.rsplit_once(':').map_or(host.as_str(), |(name, _)| name).trim_start_matches('[').trim_end_matches(']').to_owned();
        let name = ServerName::try_from(bare).map_err(|error| format!("{host}: {error}"))?;
        let config = |alpn: &[u8]| -> Result<rustls::ClientConfig, String> {
            let file = ca.ok_or("--ca FILE, the certificate of the authority that signed the server's, is needed over TLS")?;
            let mut roots = rustls::RootCertStore::empty();
            for certificate in CertificateDer::pem_file_iter(file).map_err(|error| format!("{file}: {error}"))? {
                roots.add(certificate.map_err(|error| format!("{file}: {error}"))?).map_err(|error| format!("{file}: {error}"))?;
            }
            let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|error| error.to_string())?
                .with_root_certificates(roots)
                .with_no_client_auth();
            config.alpn_protocols = vec![alpn.to_vec()];
            Ok(config)
        };
        let (mut tls, mut quic) = (None, Vec::new());
        match version {
            Version::Http1 if secure => tls = Some(TlsConnector::from(Arc::new(config(ALPN_HTTP1)?))),
            Version::Http1 => {}
            Version::Http2 => tls = Some(TlsConnector::from(Arc::new(config(ALPN_H2)?))),
            Version::Http3 => {
                let crypto = QuicClientConfig::try_from(config(ALPN_H3)?).map_err(|error| error.to_string())?;
                let mut client = quinn::ClientConfig::new(Arc::new(crypto));
                let mut transport = quinn::TransportConfig::default();
                transport.max_idle_timeout(Some(CLIENT_IDLE_AFTER.try_into().map_err(|_| "the idle time")?)).keep_alive_interval(Some(CLIENT_KEEP_ALIVE_EVERY));
                client.transport_config(Arc::new(transport));
                let any = if address.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" };
                for _ in 0..QUIC_CLIENT_SOCKETS {
                    let socket = std::net::UdpSocket::bind(any).map_err(|error| format!("a QUIC socket: {error}"))?;
                    let state = quinn::udp::UdpSocketState::new((&socket).into()).map_err(|error| format!("a QUIC socket: {error}"))?;
                    // As much as the system allows: a storm's handshakes
                    // answer every viewer at once.
                    let _ = state.set_recv_buffer_size((&socket).into(), QUIC_CLIENT_RECEIVE_BUFFER_BYTES);
                    let runtime = quinn::default_runtime().ok_or("no async runtime")?;
                    let mut endpoint =
                        quinn::Endpoint::new(quinn::EndpointConfig::default(), None, socket, runtime).map_err(|error| format!("a QUIC socket: {error}"))?;
                    endpoint.set_default_client_config(client.clone());
                    quic.push(endpoint);
                }
            }
        }
        Ok(Self { host, path, version, address, name, tls, quic, ports, tried: std::sync::atomic::AtomicUsize::new(0), sources })
    }

    /// A TCP connection, from the source address and port `number` picks.
    async fn tcp(&self, number: usize) -> Option<TcpStream> {
        let socket = if self.address.is_ipv6() { TcpSocket::new_v6() } else { TcpSocket::new_v4() }.ok()?;
        let source_ip = if self.sources > 1 {
            let [first, second, third, fourth] = LOOPBACK;
            let offset = u8::try_from(number.checked_rem(self.sources)?).ok()?;
            Some(std::net::IpAddr::from([first, second, third, fourth.checked_add(offset)?]))
        } else {
            None
        };
        let port = match self.ports {
            Some((first, count)) => {
                let attempt = self.tried.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                first.checked_add(u16::try_from(attempt.checked_rem(usize::from(count))?).ok()?)?
            }
            None => 0,
        };
        if source_ip.is_some() || port != 0 {
            // A port this client used a moment ago may still be waiting out
            // its old connection.
            socket.set_reuseaddr(true).ok()?;
            let ip = source_ip.unwrap_or(if self.address.is_ipv6() { std::net::Ipv6Addr::UNSPECIFIED.into() } else { std::net::Ipv4Addr::UNSPECIFIED.into() });
            socket.bind(SocketAddr::new(ip, port)).ok()?;
        }
        let stream = socket.connect(self.address).await.ok()?;
        let _ = stream.set_nodelay(true);
        Some(stream)
    }

    /// A viewer's connection, `number` among them, over which its tabs
    /// are opened.
    pub async fn viewer(&self, number: usize) -> Option<Viewer> {
        match self.version {
            Version::Http1 => Some(Viewer::Http1),
            Version::Http2 => {
                let stream = self.tcp(number).await?;
                let session = self.tls.as_ref()?.connect(self.name.clone(), stream).await.ok()?;
                let (sender, connection) = h2::client::handshake(session).await.ok()?;
                let (gone, ended) = watch::channel(false);
                tokio::spawn(async move {
                    let _ = connection.await;
                    let _ = gone.send(true);
                });
                let sender = sender.ready().await.ok()?;
                // RFC 8441 section 3: extended CONNECT only once the
                // server's SETTINGS say it takes it, which the connection's
                // task reads as they come.
                let began = std::time::Instant::now();
                while !sender.is_extended_connect_protocol_enabled() {
                    if began.elapsed() > SETTINGS_WITHIN || *ended.borrow() {
                        return None;
                    }
                    tokio::time::sleep(SETTINGS_LOOK_EVERY).await;
                }
                Some(Viewer::Http2 { sender, ended })
            }
            Version::Http3 => {
                let endpoint = self.quic.get(number.checked_rem(self.quic.len())?)?;
                let name = match &self.name {
                    ServerName::DnsName(name) => name.as_ref().to_owned(),
                    ServerName::IpAddress(address) => std::net::IpAddr::from(*address).to_string(),
                    _ => return None,
                };
                let connection = tokio::time::timeout(CLIENT_HANDSHAKE_WITHIN, endpoint.connect(self.address, &name).ok()?).await.ok()?.ok()?;
                let mut control = connection.open_uni().await.ok()?;
                let mut opening = Vec::new();
                open_client_control(&mut opening);
                control.write_all(&opening).await.ok()?;
                let (told, mut settings) = watch::channel(None);
                tokio::spawn(server_streams(connection.clone(), told));
                // RFC 9220 section 3: extended CONNECT only once the
                // server's SETTINGS say it takes it.
                let connect = settings.wait_for(Option::is_some).await.ok()?.unwrap_or(false);
                connect.then_some(Viewer::Http3 { connection, _control: control })
            }
        }
    }

    /// Opens a tab's push WebSocket on `viewer`, `number` among the tabs:
    /// the WebSocket and what came after its answer's head.
    pub async fn tab(&self, viewer: &mut Viewer, number: usize) -> Option<(Box<dyn Duplex>, Vec<u8>)> {
        match viewer {
            Viewer::Http1 => {
                let stream = self.tcp(number).await?;
                let mut stream: Box<dyn Duplex> = match &self.tls {
                    Some(tls) => Box::new(tls.connect(self.name.clone(), stream).await.ok()?),
                    None => Box::new(stream),
                };
                let request = format!(
                    "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: {PUSH_SUBPROTOCOL}\r\n\r\n",
                    self.path, self.host
                );
                stream.write_all(request.as_bytes()).await.ok()?;
                let mut buffer: Vec<u8> = Vec::new();
                let mut chunk = vec![0_u8; READ_BYTES];
                loop {
                    let count = stream.read(&mut chunk).await.ok()?;
                    if count == 0 || buffer.len() > MOST_HEAD_BYTES {
                        return None;
                    }
                    buffer.extend_from_slice(chunk.get(..count).unwrap_or(&[]));
                    if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                        if !buffer.starts_with(b"HTTP/1.1 101") {
                            return None;
                        }
                        buffer.drain(..end.saturating_add(4));
                        return Some((stream, buffer));
                    }
                }
            }
            Viewer::Http2 { sender, .. } => {
                let mut request = hyper::Request::builder()
                    .method(hyper::Method::CONNECT)
                    .uri(format!("https://{}{}", self.host, self.path))
                    .header("sec-websocket-version", "13")
                    .header("sec-websocket-protocol", PUSH_SUBPROTOCOL)
                    .body(())
                    .ok()?;
                request.extensions_mut().insert(h2::ext::Protocol::from("websocket"));
                let mut ready = sender.clone().ready().await.ok()?;
                let (response, send) = ready.send_request(request, false).ok()?;
                let response = response.await.ok()?;
                (response.status() == hyper::StatusCode::OK)
                    .then(|| (Box::new(H2Tab { send, receive: response.into_body(), unread: Bytes::new() }) as Box<dyn Duplex>, Vec::new()))
            }
            Viewer::Http3 { connection, .. } => {
                let (mut send, mut receive) = connection.open_bi().await.ok()?;
                let mut head = Vec::new();
                let request = WebSocketRequest {
                    authority: self.host.as_bytes(),
                    path: self.path.as_bytes(),
                    subprotocol: PUSH_SUBPROTOCOL.as_bytes(),
                    user_agent: USER_AGENT,
                };
                client::open_websocket(&request, &mut head).ok()?;
                send.write_all(&head).await.ok()?;
                let mut exchange = Exchange::new();
                let mut after = Vec::new();
                let mut raw = vec![0_u8; READ_BYTES];
                loop {
                    let count = receive.read(&mut raw).await.ok()??;
                    let mut at = 0_usize;
                    let mut answered = None;
                    while at < count {
                        let (taken, event) = exchange.read(raw.get(at..count).unwrap_or(&[])).ok()?;
                        at = at.saturating_add(taken);
                        match event {
                            Some(ExchangeEvent::Answered(answer)) => answered = Some(answer.status),
                            Some(ExchangeEvent::Data(part)) => after.extend_from_slice(part),
                            Some(ExchangeEvent::Trailers) => return None,
                            None if taken == 0 => break,
                            None => {}
                        }
                    }
                    match answered {
                        Some(200) => {
                            return Some((Box::new(H3Tab { send, receive, exchange, raw, at: 0, filled: 0, unread: Vec::new(), pending: Vec::new() }), after));
                        }
                        Some(_) => return None,
                        None => {}
                    }
                }
            }
        }
    }
}

/// Reads the streams the server opens on an HTTP/3 connection, telling
/// `told` whether its SETTINGS take extended CONNECT.
async fn server_streams(connection: quinn::Connection, told: watch::Sender<Option<bool>>) {
    let opened = Arc::new(std::sync::Mutex::new(Opened::new(Role::Client)));
    let told = Arc::new(told);
    while let Ok(mut receive) = connection.accept_uni().await {
        let (opened, told) = (Arc::clone(&opened), Arc::clone(&told));
        tokio::spawn(async move {
            let mut incoming = Incoming::new();
            let mut raw = vec![0_u8; READ_BYTES];
            while let Ok(Some(count)) = receive.read(&mut raw).await {
                let mut at = 0_usize;
                while at < count {
                    let read = incoming.read(&mut opened.lock().unwrap_or_else(std::sync::PoisonError::into_inner), raw.get(at..count).unwrap_or(&[]));
                    let Ok((taken, event)) = read else { return };
                    at = at.saturating_add(taken);
                    match event {
                        Some(IncomingEvent::Settings(settings)) => {
                            let _ = told.send(Some(settings.connect_protocol));
                        }
                        Some(IncomingEvent::Refuse) => return,
                        Some(IncomingEvent::GoAway(_)) | None if taken > 0 => {}
                        _ => break,
                    }
                }
            }
        });
    }
}

/// A viewer's connection to the server.
pub enum Viewer {
    /// None: each tab makes its own.
    Http1,
    Http2 {
        sender: SendRequest<Bytes>,
        /// Set once the connection has ended.
        ended: watch::Receiver<bool>,
    },
    Http3 {
        connection: quinn::Connection,
        /// This side's control stream, open as long as the connection
        /// (RFC 9114 section 6.2.1).
        _control: quinn::SendStream,
    },
}

impl Viewer {
    /// How the viewer's QUIC connection ended, once it has: the server
    /// closed it (as at a stop, `H3_NO_ERROR`), it went silent past the
    /// idle timeout, or otherwise; `None` over TCP or while it lasts.
    pub fn ended_by(&self) -> Option<&'static str> {
        let Self::Http3 { connection, .. } = self else { return None };
        Some(match connection.close_reason()? {
            quinn::ConnectionError::ApplicationClosed(_) => "closed by the server",
            quinn::ConnectionError::TimedOut => "timed out",
            quinn::ConnectionError::ConnectionClosed(_) => "closed by the server's QUIC",
            quinn::ConnectionError::Reset => "reset",
            _ => "otherwise",
        })
    }

    /// Whether the viewer's connection may still carry a new tab: always
    /// over HTTP/1.1, where each tab makes its own.
    pub fn alive(&self) -> bool {
        match self {
            Self::Http1 => true,
            Self::Http2 { ended, .. } => !*ended.borrow(),
            Self::Http3 { connection, .. } => connection.close_reason().is_none(),
        }
    }
}

/// A tab's WebSocket on an HTTP/2 stream.
struct H2Tab {
    send: h2::SendStream<Bytes>,
    receive: h2::RecvStream,
    unread: Bytes,
}

fn broken(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, error.to_string())
}

impl AsyncRead for H2Tab {
    fn poll_read(self: Pin<&mut Self>, context: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.unread.is_empty() {
            match ready!(this.receive.poll_data(context)) {
                None => return Poll::Ready(Ok(())),
                Some(Err(error)) => return Poll::Ready(Err(broken(error))),
                Some(Ok(bytes)) => {
                    let _ = this.receive.flow_control().release_capacity(bytes.len());
                    this.unread = bytes;
                }
            }
        }
        let count = this.unread.len().min(buffer.remaining());
        buffer.put_slice(this.unread.split_to(count).as_ref());
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for H2Tab {
    fn poll_write(self: Pin<&mut Self>, context: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        this.send.reserve_capacity(bytes.len());
        loop {
            match ready!(this.send.poll_capacity(context)) {
                None => return Poll::Ready(Err(broken("the stream is closed"))),
                Some(Err(error)) => return Poll::Ready(Err(broken(error))),
                Some(Ok(0)) => {}
                Some(Ok(room)) => {
                    let count = room.min(bytes.len());
                    this.send.send_data(Bytes::copy_from_slice(bytes.get(..count).unwrap_or(&[])), false).map_err(broken)?;
                    return Poll::Ready(Ok(count));
                }
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.get_mut().send.send_data(Bytes::new(), true).map_err(broken))
    }
}

/// A tab's WebSocket on an HTTP/3 stream, its bytes in DATA frames.
struct H3Tab {
    send: quinn::SendStream,
    receive: quinn::RecvStream,
    exchange: Exchange,
    raw: Vec<u8>,
    at: usize,
    filled: usize,
    /// DATA payload read and not yet given.
    unread: Vec<u8>,
    /// A DATA frame accepted and not yet all given to QUIC.
    pending: Vec<u8>,
}

impl H3Tab {
    fn poll_pending(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        while !self.pending.is_empty() {
            let taken = ready!(Pin::new(&mut self.send).poll_write(context, &self.pending))?;
            if taken == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            self.pending.drain(..taken.min(self.pending.len()));
        }
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for H3Tab {
    fn poll_read(self: Pin<&mut Self>, context: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if !this.unread.is_empty() {
                let count = this.unread.len().min(buffer.remaining());
                buffer.put_slice(this.unread.get(..count).unwrap_or(&[]));
                this.unread.drain(..count);
                return Poll::Ready(Ok(()));
            }
            while this.at < this.filled {
                let (taken, event) = this.exchange.read(this.raw.get(this.at..this.filled).unwrap_or(&[])).map_err(|_| broken("the stream broke HTTP/3"))?;
                this.at = this.at.saturating_add(taken);
                if let Some(ExchangeEvent::Data(part)) = event {
                    this.unread.extend_from_slice(part);
                    break;
                }
                if taken == 0 {
                    break;
                }
            }
            if !this.unread.is_empty() {
                continue;
            }
            let mut raw = ReadBuf::new(&mut this.raw);
            ready!(Pin::new(&mut this.receive).poll_read(context, &mut raw))?;
            let count = raw.filled().len();
            if count == 0 {
                return Poll::Ready(Ok(()));
            }
            (this.at, this.filled) = (0, count);
        }
    }
}

impl AsyncWrite for H3Tab {
    /// Takes `bytes` whole as one DATA frame once the frame before has
    /// gone to QUIC: a tab writes one WebSocket frame at a time.
    fn poll_write(self: Pin<&mut Self>, context: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        ready!(this.poll_pending(context))?;
        client::data(bytes, &mut this.pending);
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().poll_pending(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.poll_pending(context))?;
        Poll::Ready(this.send.finish().map_err(broken))
    }
}
