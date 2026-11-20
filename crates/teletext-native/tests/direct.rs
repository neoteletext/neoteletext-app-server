//! TLS, HTTP/2 and HTTP/3 served by the server itself, with no proxy in
//! front (DEC-6964): the certificate and key it is given (REQ-6965), a
//! channel's push connections opened by extended CONNECT over HTTP/2 (RFC
//! 8441) and HTTP/3 (RFC 9220), several on one connection (REQ-6966), and
//! the connection errors HTTP/3 input over its bounds ends in (REQ-6967).
//! Before DEC-6964 the server spoke plain HTTP/1.1 alone, so none of this
//! could be asked of it.
#![allow(clippy::arithmetic_side_effects, reason = "a test")]

use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use hyper::body::Bytes;
use hyper::header::{ALT_SVC, ORIGIN, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION};
use hyper::{Method, Request, StatusCode};
use neoteletext_web::consts::PUSH_SUBPROTOCOL;
use quinn::crypto::rustls::{HandshakeData, QuicClientConfig};
use rustls::pki_types::{CertificateDer, ServerName};
use teletext_http3::limits::MAX_FIELD_SECTION_BYTES;
use teletext_http3::qpack::{self, Field};
use teletext_http3::settings::Settings;
use teletext_http3::stream::{Control, ControlEvent, Message, MessageEvent, Role, open_control};
use teletext_http3::{Code, frame, varint};
use teletext_native::http::Routes;
use teletext_native::hub;
use teletext_native::serving::{Direct, Server, public_port};
use teletext_native::state::SystemRandomSource;
use teletext_native::stats::ServerStats;
use teletext_native::tls::{self, ALPN_H2, ALPN_H3, ALPN_HTTP1, Files};
use teletext_runtime::{Channel, ChannelConfig, HandleSecret, InitialPage};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Instant, timeout};
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// The longest any step here may take.
const WITHIN: Duration = Duration::from_secs(20);
/// The name the test certificate is for.
const HOST: &str = "localhost";
/// Lets the kernel choose the port.
const ANY_LOCAL_PORT: &str = "127.0.0.1:0";
/// The channel's folder.
const FOLDER: &str = "test";
/// A masked Ping of no payload, with the key 0 (RFC 6455 sections 5.2 and
/// 5.3), and the server's Pong to it.
const PING: [u8; 6] = [0x89, 0x80, 0, 0, 0, 0];
const PONG_OPCODE: u8 = 0x8A;
/// `Sec-WebSocket-Version`'s one value (RFC 6455 section 4.1).
const VERSION: &str = "13";

/// A certificate for [`HOST`] and its key, made for this test, in files.
struct Certificate {
    files: Files,
    der: CertificateDer<'static>,
}

fn certificate(name: &str) -> Certificate {
    let made = rcgen::generate_simple_self_signed(vec![HOST.to_owned()]).unwrap();
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("direct-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let files = Files { certificate: folder.join("certificate.pem"), key: folder.join("key.pem") };
    std::fs::write(&files.certificate, made.cert.pem()).unwrap();
    std::fs::write(&files.key, made.signing_key.serialize_pem()).unwrap();
    Certificate { files, der: made.cert.der().clone() }
}

/// The server, in this process, on a channel with no program.
struct Running {
    address: SocketAddr,
    quic: Option<SocketAddr>,
    descriptor: Arc<[u8]>,
    serving: tokio::task::JoinHandle<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

async fn running(certificate: Option<&Certificate>) -> Running {
    running_at(certificate, |port| format!(":{port}")).await
}

/// The server, reached by viewers at the origin of [`HOST`] with
/// `public`'s port text, given the port it listens on.
async fn running_at(certificate: Option<&Certificate>, public: impl Fn(u16) -> String) -> Running {
    let listener = TcpListener::bind(ANY_LOCAL_PORT).await.unwrap();
    let address = listener.local_addr().unwrap();
    let scheme = if certificate.is_some() { "https" } else { "http" };
    let origin = format!("{scheme}://{HOST}{}", public(address.port()));
    let config = ChannelConfig {
        name: "Test".to_owned(),
        address: format!("{origin}/{FOLDER}/"),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        default_rules: teletext_runtime::channel::DefaultRules::None,
    };
    let stats = Arc::new(ServerStats::default());
    let channel = Channel::new(config, &HandleSecret::new([0; 32]), Box::new(SystemRandomSource::new()));
    let hub = hub::spawn(FOLDER, channel, Instant::now(), None, Arc::clone(&stats));
    let descriptor = Arc::clone(&hub.descriptor);
    let routes = Arc::new(Routes { hubs: BTreeMap::from([(FOLDER.to_owned(), hub)]), connections: AtomicU64::new(1), stats, trusted_proxy: None });
    let secured = certificate.map(|certificate| tls::load(&certificate.files).unwrap());
    let direct = secured.as_ref().map(|tls| Direct { tls, public_port: public_port(&origin).unwrap() });
    let server = Server::start(listener, direct, routes).unwrap();
    let quic = server.quic_address();
    let serving = tokio::spawn(async move {
        server.run(std::future::pending()).await;
    });
    Running { address, quic, descriptor, serving }
}

/// A client's TLS that trusts `certificate` alone and offers `alpn`.
fn client_tls(certificate: &Certificate, alpn: &[&[u8]]) -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate.der.clone()).unwrap();
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
    config
}

/// A TLS session to the server over TCP, offering `alpn`.
async fn tls_to(running: &Running, certificate: &Certificate, alpn: &[&[u8]]) -> TlsStream<TcpStream> {
    let connector = TlsConnector::from(Arc::new(client_tls(certificate, alpn)));
    let stream = TcpStream::connect(running.address).await.unwrap();
    timeout(WITHIN, connector.connect(ServerName::try_from(HOST).unwrap(), stream)).await.expect("a TLS handshake in time").unwrap()
}

/// An HTTP/1.1 GET of `path` on `stream`, and the whole answer.
async fn http1_get<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(mut stream: S, path: &str) -> String {
    stream.write_all(format!("GET {path} HTTP/1.1\r\nHost: {HOST}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    let mut answer = Vec::new();
    // A TLS session the server closes without a notice still gave its answer.
    let _ = timeout(WITHIN, stream.read_to_end(&mut answer)).await.expect("an answer in time");
    String::from_utf8(answer).unwrap()
}

/// The server's WebSocket frames, from bytes as they come.
#[derive(Default)]
struct Frames(Vec<u8>);

impl Frames {
    /// The next whole frame's first byte and payload (RFC 6455 section
    /// 5.2; the server's frames are not masked).
    fn next(&mut self) -> Option<(u8, Vec<u8>)> {
        let (&first, &second) = (self.0.first()?, self.0.get(1)?);
        let (length, header) = match second {
            126 => (usize::from(u16::from_be_bytes(self.0.get(2..4)?.try_into().unwrap())), 4),
            127 => (usize::try_from(u64::from_be_bytes(self.0.get(2..10)?.try_into().unwrap())).unwrap(), 10),
            short => (usize::from(short), 2),
        };
        let payload = self.0.get(header..header + length)?.to_vec();
        self.0.drain(..header + length);
        Some((first, payload))
    }
}

/// The path of the channel's `name`.
fn path(name: &str) -> String {
    format!("/{FOLDER}/{name}")
}

// ---------------------------------------------------------------- HTTP/2

/// An HTTP/2 connection to the server.
async fn h2_to(running: &Running, certificate: &Certificate) -> h2::client::SendRequest<Bytes> {
    let session = tls_to(running, certificate, &[ALPN_H2]).await;
    assert_eq!(session.get_ref().1.alpn_protocol(), Some(ALPN_H2));
    let (sender, connection) = h2::client::handshake(session).await.unwrap();
    tokio::spawn(connection);
    sender
}

/// A request for `path` of the channel over HTTP/2: an extended CONNECT
/// with `protocol` (RFC 8441 section 4), or a GET without.
fn h2_request(running: &Running, method: Method, name: &str, protocol: Option<&str>, headers: &[(&str, &str)]) -> Request<()> {
    let mut request = Request::builder().method(method).uri(format!("https://{HOST}:{}{}", running.address.port(), path(name)));
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut request = request.body(()).unwrap();
    if let Some(protocol) = protocol {
        request.extensions_mut().insert(h2::ext::Protocol::from(protocol));
    }
    request
}

/// The status and headers answered to `request`, and the stream's two
/// directions.
async fn h2_ask(sender: &mut h2::client::SendRequest<Bytes>, request: Request<()>, end: bool) -> (hyper::Response<h2::RecvStream>, h2::SendStream<Bytes>) {
    let (response, stream) = sender.send_request(request, end).unwrap();
    (timeout(WITHIN, response).await.expect("an answer in time").unwrap(), stream)
}

/// The whole body of an HTTP/2 answer.
async fn h2_body(mut body: h2::RecvStream) -> Vec<u8> {
    let mut whole = Vec::new();
    while let Some(part) = timeout(WITHIN, body.data()).await.expect("a body in time") {
        let part = part.unwrap();
        body.flow_control().release_capacity(part.len()).unwrap();
        whole.extend_from_slice(&part);
    }
    whole
}

/// Pings the server on a WebSocket's HTTP/2 stream and waits for the Pong.
async fn h2_ping_pong(send: &mut h2::SendStream<Bytes>, receive: &mut h2::RecvStream) {
    send.send_data(Bytes::from_static(&PING), false).unwrap();
    let mut frames = Frames::default();
    loop {
        while let Some((first, payload)) = frames.next() {
            if first == PONG_OPCODE {
                assert_eq!(payload, [] as [u8; 0]);
                return;
            }
        }
        let part = timeout(WITHIN, receive.data()).await.expect("a pong in time").expect("the stream stays open").unwrap();
        receive.flow_control().release_capacity(part.len()).unwrap();
        frames.0.extend_from_slice(&part);
    }
}

const PUSH_HEADERS: [(&str, &str); 2] = [("sec-websocket-version", VERSION), ("sec-websocket-protocol", PUSH_SUBPROTOCOL)];

/// REQ-6966: over HTTP/2 the server says it takes extended CONNECT (RFC
/// 8441 section 3), and two push connections on one connection are each
/// answered 200 with the subprotocol and no `Sec-WebSocket-Accept`, and
/// each exchanges a ping and a pong with the server.
#[tokio::test]
async fn two_push_websockets_share_one_http2_connection() {
    let certificate = certificate("h2-two");
    let running = running(Some(&certificate)).await;
    let mut sender = h2_to(&running, &certificate).await;
    // An answer has come once this returns, and SETTINGS before it.
    let (described, _) = h2_ask(&mut sender, h2_request(&running, Method::GET, "channel.ttx", None, &[]), true).await;
    assert_eq!(described.status(), StatusCode::OK);
    assert!(sender.is_extended_connect_protocol_enabled(), "SETTINGS_ENABLE_CONNECT_PROTOCOL = 1");
    let mut open = Vec::new();
    for _ in 0..2 {
        let (response, send) = h2_ask(&mut sender, h2_request(&running, Method::CONNECT, "push", Some("websocket"), &PUSH_HEADERS), false).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(SEC_WEBSOCKET_PROTOCOL).unwrap(), PUSH_SUBPROTOCOL);
        assert!(!response.headers().contains_key("sec-websocket-accept"), "RFC 8441 section 5");
        open.push((send, response.into_body()));
    }
    // Both are open at once; each is answered on its own stream.
    for (send, receive) in open.iter_mut().rev() {
        h2_ping_pong(send, receive).await;
    }
    for (send, receive) in &mut open {
        h2_ping_pong(send, receive).await;
    }
}

/// REQ-6966: over HTTP/2 a push CONNECT is checked as the HTTP/1.1 upgrade
/// is, `Origin` 403 and no subprotocol 426 naming it; another `:protocol`
/// is 501; a CONNECT to a route with no WebSocket 404; and `channel.ttx`
/// is served as over HTTP/1.1.
#[tokio::test]
async fn http2_checks_a_connect_as_an_upgrade_and_serves_the_descriptor() {
    let certificate = certificate("h2-checks");
    let running = running(Some(&certificate)).await;
    let mut sender = h2_to(&running, &certificate).await;
    let (described, _) = h2_ask(&mut sender, h2_request(&running, Method::GET, "channel.ttx", None, &[]), true).await;
    assert_eq!(described.status(), StatusCode::OK);
    assert_eq!(described.headers().get(ALT_SVC).unwrap().to_str().unwrap(), format!("h3=\":{}\"", running.address.port()), "REQ-6965");
    assert_eq!(h2_body(described.into_body()).await, &*running.descriptor);

    let with_origin = [PUSH_HEADERS[0], PUSH_HEADERS[1], (ORIGIN.as_str(), "https://elsewhere.example")];
    let (refused, _) = h2_ask(&mut sender, h2_request(&running, Method::CONNECT, "push", Some("websocket"), &with_origin), false).await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);

    let (refused, _) = h2_ask(&mut sender, h2_request(&running, Method::CONNECT, "push", Some("websocket"), &PUSH_HEADERS[..1]), false).await;
    assert_eq!(refused.status(), StatusCode::UPGRADE_REQUIRED);
    assert_eq!(refused.headers().get(SEC_WEBSOCKET_PROTOCOL).unwrap(), PUSH_SUBPROTOCOL);

    let (refused, _) = h2_ask(&mut sender, h2_request(&running, Method::CONNECT, "push", Some("webtransport"), &PUSH_HEADERS), false).await;
    assert_eq!(refused.status(), StatusCode::NOT_IMPLEMENTED);

    let (refused, _) = h2_ask(&mut sender, h2_request(&running, Method::CONNECT, "channel.ttx", Some("websocket"), &PUSH_HEADERS), false).await;
    assert_eq!(refused.status(), StatusCode::NOT_FOUND, "a 2xx would have opened a tunnel");

    let (refused, _) = h2_ask(&mut sender, h2_request(&running, Method::CONNECT, "push", Some("websocket"), &PUSH_HEADERS[1..]), false).await;
    assert_eq!(refused.status(), StatusCode::UPGRADE_REQUIRED, "no Sec-WebSocket-Version");
    assert_eq!(refused.headers().get(SEC_WEBSOCKET_VERSION).unwrap(), VERSION);

    let (refused, _) = h2_ask(&mut sender, h2_request(&running, Method::GET, "push", None, &PUSH_HEADERS), true).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "HTTP/2 has no upgrade by GET");
}

// ---------------------------------------------------------------- HTTP/3

/// An HTTP/3 connection to the server: QUIC's, the server's settings as
/// its control stream gave them, and this side's control stream, kept
/// open as RFC 9114 section 6.2.1 requires.
struct H3 {
    connection: quinn::Connection,
    settings: Settings,
    _endpoint: quinn::Endpoint,
    _control: quinn::SendStream,
}

async fn h3_to(running: &Running, certificate: &Certificate) -> H3 {
    let mut endpoint = quinn::Endpoint::client(ANY_LOCAL_PORT.parse().unwrap()).unwrap();
    let crypto = QuicClientConfig::try_from(client_tls(certificate, &[ALPN_H3])).unwrap();
    endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(crypto)));
    let connecting = endpoint.connect(running.quic.expect("QUIC is served"), HOST).unwrap();
    let connection = timeout(WITHIN, connecting).await.expect("a QUIC handshake in time").unwrap();
    let mut control = connection.open_uni().await.unwrap();
    let mut opening = Vec::new();
    open_control(&mut opening);
    control.write_all(&opening).await.unwrap();
    // The server's control stream: its type, then SETTINGS.
    let mut theirs = timeout(WITHIN, connection.accept_uni()).await.expect("the server's control stream in time").unwrap();
    let mut kind = [0_u8; 1];
    theirs.read_exact(&mut kind).await.unwrap();
    assert_eq!(u64::from(kind[0]), teletext_http3::stream::CONTROL);
    let mut reader = Control::new(Role::Client);
    let settings = loop {
        let mut byte = [0_u8; 1];
        timeout(WITHIN, theirs.read_exact(&mut byte)).await.expect("SETTINGS in time").unwrap();
        if let (_, Some(ControlEvent::Settings(settings))) = reader.read(&byte).unwrap() {
            break settings;
        }
    };
    H3 { connection, settings, _endpoint: endpoint, _control: control }
}

/// The fields of a request for the channel's `name`: a GET, or with
/// `protocol` an extended CONNECT (RFC 9220 section 3).
fn h3_fields(running: &Running, method: &str, name: &str, protocol: Option<&str>, more: &[(&str, &str)]) -> Vec<Field> {
    let authority = format!("{HOST}:{}", running.address.port());
    let mut fields = vec![Field::new(b":method", method.as_bytes())];
    fields.extend(protocol.map(|protocol| Field::new(b":protocol", protocol.as_bytes())));
    fields.extend([Field::new(b":scheme", b"https"), Field::new(b":authority", authority.as_bytes()), Field::new(b":path", path(name).as_bytes())]);
    fields.extend(more.iter().map(|(name, value)| Field::new(name.as_bytes(), value.as_bytes())));
    fields
}

/// An HTTP/3 request stream after its answer's head.
struct H3Stream {
    send: quinn::SendStream,
    receive: quinn::RecvStream,
    message: Message,
    /// What was read past the head.
    read: Vec<u8>,
}

impl H3Stream {
    /// The next piece of the answer's content, or `None` at its end.
    async fn content(&mut self) -> Option<Vec<u8>> {
        loop {
            while !self.read.is_empty() {
                let (taken, event) = self.message.read(&self.read).unwrap();
                let part = match event {
                    Some(MessageEvent::Data(part)) => Some(part.to_vec()),
                    _ => None,
                };
                self.read.drain(..taken);
                if part.is_some() {
                    return part;
                }
            }
            let mut buffer = [0_u8; 1_024];
            let count = timeout(WITHIN, self.receive.read(&mut buffer)).await.expect("content in time").unwrap()?;
            self.read.extend_from_slice(&buffer[..count]);
        }
    }

    /// The whole of the answer's content.
    async fn body(&mut self) -> Vec<u8> {
        let mut whole = Vec::new();
        while let Some(part) = self.content().await {
            whole.extend(part);
        }
        whole
    }

    /// Pings the server on this WebSocket's stream, in a DATA frame, and
    /// waits for the Pong.
    async fn ping_pong(&mut self) {
        let mut ping = Vec::new();
        frame::write(frame::DATA, &PING, &mut ping);
        self.send.write_all(&ping).await.unwrap();
        let mut frames = Frames::default();
        loop {
            while let Some((first, payload)) = frames.next() {
                if first == PONG_OPCODE {
                    assert_eq!(payload, [] as [u8; 0]);
                    return;
                }
            }
            frames.0.extend(self.content().await.expect("the stream stays open"));
        }
    }
}

/// Sends a request of `fields` on a new stream, ended there when `end`,
/// and reads its answer's head: the status, the other fields, and the
/// stream.
async fn h3_ask(h3: &H3, fields: &[Field], end: bool) -> (u16, Vec<Field>, H3Stream) {
    let (mut send, mut receive) = h3.connection.open_bi().await.unwrap();
    let mut section = Vec::new();
    qpack::encode(fields.iter().map(|field| (field.name.as_slice(), field.value.as_slice())), &mut section);
    let mut head = Vec::new();
    frame::write(frame::HEADERS, &section, &mut head);
    send.write_all(&head).await.unwrap();
    if end {
        send.finish().unwrap();
    }
    let mut message = Message::new();
    let mut read = Vec::new();
    let section = 'head: loop {
        let mut buffer = [0_u8; 1_024];
        let count = timeout(WITHIN, receive.read(&mut buffer)).await.expect("an answer in time").unwrap().expect("an answer before the stream ends");
        read.extend_from_slice(&buffer[..count]);
        while !read.is_empty() {
            let (taken, event) = message.read(&read).unwrap();
            let head = match event {
                Some(MessageEvent::Head(section)) => Some(section),
                _ => None,
            };
            read.drain(..taken);
            if let Some(section) = head {
                break 'head section;
            }
        }
    };
    let mut answered = qpack::decode(&section, u64::from(MAX_FIELD_SECTION_BYTES)).unwrap();
    assert_eq!(answered[0].name, b":status");
    let status = String::from_utf8(answered.remove(0).value).unwrap().parse().unwrap();
    (status, answered, H3Stream { send, receive, message, read })
}

/// The value of the field `name` among `fields`.
fn field<'fields>(fields: &'fields [Field], name: &str) -> Option<&'fields str> {
    fields.iter().find(|field| field.name == name.as_bytes()).map(|field| std::str::from_utf8(&field.value).unwrap())
}

/// REQ-6966, REQ-6967: over HTTP/3 the server's SETTINGS say extended
/// CONNECT (RFC 9220 section 3) and no QPACK dynamic table (RFC 9204
/// section 5), and two push connections on one QUIC connection are each
/// answered 200 with the subprotocol and exchange a ping and a pong with
/// the server, their bytes in DATA frames.
#[tokio::test]
async fn two_push_websockets_share_one_http3_connection() {
    let certificate = certificate("h3-two");
    let running = running(Some(&certificate)).await;
    let h3 = h3_to(&running, &certificate).await;
    assert!(h3.settings.connect_protocol, "SETTINGS_ENABLE_CONNECT_PROTOCOL = 1");
    assert_eq!((h3.settings.qpack_max_table_capacity, h3.settings.qpack_blocked_streams), (0, 0));
    assert_eq!(h3.settings.max_field_section_size, Some(u64::from(MAX_FIELD_SECTION_BYTES)));
    let mut open = Vec::new();
    for _ in 0..2 {
        let (status, fields, stream) = h3_ask(&h3, &h3_fields(&running, "CONNECT", "push", Some("websocket"), &PUSH_HEADERS), false).await;
        assert_eq!(status, 200);
        assert_eq!(field(&fields, "sec-websocket-protocol"), Some(PUSH_SUBPROTOCOL));
        assert_eq!(field(&fields, "sec-websocket-accept"), None, "RFC 8441 section 5");
        assert_eq!(field(&fields, "content-length"), None, "a tunnel has no length");
        open.push(stream);
    }
    for stream in open.iter_mut().rev() {
        stream.ping_pong().await;
    }
    for stream in &mut open {
        stream.ping_pong().await;
    }
}

/// REQ-6966: over HTTP/3 a push CONNECT is checked as the HTTP/1.1 upgrade
/// is, `Origin` 403 and no subprotocol 426 naming it; another `:protocol`
/// is 501; a CONNECT to a route with no WebSocket 404; and `channel.ttx`
/// is served as over HTTP/1.1, to GET with its content and to HEAD with
/// its length alone.
#[tokio::test]
async fn http3_checks_a_connect_as_an_upgrade_and_serves_the_descriptor() {
    let certificate = certificate("h3-checks");
    let running = running(Some(&certificate)).await;
    let h3 = h3_to(&running, &certificate).await;
    let (status, fields, mut stream) = h3_ask(&h3, &h3_fields(&running, "GET", "channel.ttx", None, &[]), true).await;
    assert_eq!(status, 200);
    assert_eq!(field(&fields, "content-type"), Some("application/json"));
    assert_eq!(field(&fields, "content-length"), Some(running.descriptor.len().to_string().as_str()));
    assert!(field(&fields, "date").is_some_and(|date| date.ends_with(" GMT")));
    assert_eq!(stream.body().await, &*running.descriptor);

    let (status, fields, mut stream) = h3_ask(&h3, &h3_fields(&running, "HEAD", "channel.ttx", None, &[]), true).await;
    assert_eq!((status, field(&fields, "content-length")), (200, Some(running.descriptor.len().to_string().as_str())));
    assert_eq!(stream.body().await, [] as [u8; 0]);

    let with_origin = [PUSH_HEADERS[0], PUSH_HEADERS[1], ("origin", "https://elsewhere.example")];
    let (status, _, _) = h3_ask(&h3, &h3_fields(&running, "CONNECT", "push", Some("websocket"), &with_origin), false).await;
    assert_eq!(status, 403);

    let (status, fields, _) = h3_ask(&h3, &h3_fields(&running, "CONNECT", "push", Some("websocket"), &PUSH_HEADERS[..1]), false).await;
    assert_eq!((status, field(&fields, "sec-websocket-protocol")), (426, Some(PUSH_SUBPROTOCOL)));
    assert_eq!(field(&fields, "upgrade"), None, "RFC 9114 section 4.2: not a field HTTP/3 carries");

    let (status, _, _) = h3_ask(&h3, &h3_fields(&running, "CONNECT", "push", Some("webtransport"), &PUSH_HEADERS), false).await;
    assert_eq!(status, 501);

    let (status, _, _) = h3_ask(&h3, &h3_fields(&running, "CONNECT", "channel.ttx", Some("websocket"), &PUSH_HEADERS), false).await;
    assert_eq!(status, 404, "a 2xx would have opened a tunnel");

    let (status, _, _) = h3_ask(&h3, &h3_fields(&running, "GET", "nothing-here", None, &[]), true).await;
    assert_eq!(status, 404);
}

/// The application error code the server closed `h3`'s connection with.
async fn closed_with(h3: &H3) -> u64 {
    match timeout(WITHIN, h3.connection.closed()).await.expect("the connection closed in time") {
        quinn::ConnectionError::ApplicationClosed(close) => close.error_code.into_inner(),
        other => panic!("closed by the server's HTTP/3, not {other:?}"),
    }
}

/// A HEADERS frame of `section` on a new request stream.
async fn h3_send_section(h3: &H3, section: &[u8]) -> (quinn::SendStream, quinn::RecvStream) {
    let (mut send, receive) = h3.connection.open_bi().await.unwrap();
    let mut head = Vec::new();
    frame::write(frame::HEADERS, section, &mut head);
    send.write_all(&head).await.unwrap();
    (send, receive)
}

/// REQ-6967: a field section that refers to the QPACK dynamic table ends
/// the connection with `QPACK_DECOMPRESSION_FAILED` (RFC 9204 section 6).
#[tokio::test]
async fn a_dynamic_table_reference_ends_the_http3_connection() {
    let certificate = certificate("h3-dynamic");
    let running = running(Some(&certificate)).await;
    let h3 = h3_to(&running, &certificate).await;
    // Required Insert Count 0, Base 0, then an indexed field line of the
    // dynamic table (RFC 9204 section 4.5.2, T = 0).
    let _stream = h3_send_section(&h3, &[0x00, 0x00, 0x80]).await;
    assert_eq!(closed_with(&h3).await, u64::from(Code::QpackDecompressionFailed.value()));
}

/// REQ-6967: a HEADERS frame over the field section bound ends the
/// connection with `H3_EXCESSIVE_LOAD`, on its length alone; a second
/// control stream, and a push stream from a client, end it with
/// `H3_STREAM_CREATION_ERROR` (RFC 9114 sections 6.2.1 and 6.2.2); and a
/// frame of HTTP/2 with `H3_FRAME_UNEXPECTED` (section 7.2.8).
#[tokio::test]
async fn http3_input_over_its_bounds_is_a_connection_error() {
    let certificate = certificate("h3-bounds");
    let running = running(Some(&certificate)).await;

    let h3 = h3_to(&running, &certificate).await;
    let (mut send, _receive) = h3.connection.open_bi().await.unwrap();
    let mut over = Vec::new();
    frame::header(frame::HEADERS, usize::try_from(MAX_FIELD_SECTION_BYTES).unwrap() + 1, &mut over);
    send.write_all(&over).await.unwrap();
    assert_eq!(closed_with(&h3).await, u64::from(Code::ExcessiveLoad.value()));

    for kind in [teletext_http3::stream::CONTROL, teletext_http3::stream::PUSH] {
        let h3 = h3_to(&running, &certificate).await;
        let mut second = h3.connection.open_uni().await.unwrap();
        let mut opening = Vec::new();
        varint::encode(kind, &mut opening);
        second.write_all(&opening).await.unwrap();
        assert_eq!(closed_with(&h3).await, u64::from(Code::StreamCreationError.value()), "stream type {kind}");
    }

    let h3 = h3_to(&running, &certificate).await;
    let (mut send, _receive) = h3.connection.open_bi().await.unwrap();
    // PING, type 0x06 in HTTP/2, of no payload.
    send.write_all(&[0x06, 0x00]).await.unwrap();
    assert_eq!(closed_with(&h3).await, u64::from(Code::FrameUnexpected.value()));
}

/// REQ-6967: a malformed request, here one with a field of an HTTP/1.1
/// connection (RFC 9114 section 4.2), ends its stream with
/// `H3_MESSAGE_ERROR` and leaves the connection serving.
#[tokio::test]
async fn a_malformed_http3_request_ends_its_stream_alone() {
    let certificate = certificate("h3-malformed");
    let running = running(Some(&certificate)).await;
    let h3 = h3_to(&running, &certificate).await;
    let mut section = Vec::new();
    let fields = h3_fields(&running, "GET", "channel.ttx", None, &[("connection", "keep-alive")]);
    qpack::encode(fields.iter().map(|field| (field.name.as_slice(), field.value.as_slice())), &mut section);
    let (_send, mut receive) = h3_send_section(&h3, &section).await;
    let mut buffer = [0_u8; 16];
    match timeout(WITHIN, receive.read(&mut buffer)).await.expect("the stream ended in time") {
        Err(quinn::ReadError::Reset(code)) => assert_eq!(code.into_inner(), u64::from(Code::MessageError.value())),
        other => panic!("reset with H3_MESSAGE_ERROR, not {other:?}"),
    }
    let (status, _, _) = h3_ask(&h3, &h3_fields(&running, "GET", "channel.ttx", None, &[]), true).await;
    assert_eq!(status, 200, "the connection goes on");
}

// ------------------------------------------------------------------- TLS

/// REQ-6965: given a certificate and key the server terminates TLS on
/// TCP, taking `h2` or `http/1.1` by ALPN as the client offers, `h2` when
/// it offers both; and every answer over TCP names HTTP/3's port in
/// `Alt-Svc`.
#[tokio::test]
async fn tls_offers_http2_and_http1_and_names_http3() {
    let certificate = certificate("tls-alpn");
    let running = running(Some(&certificate)).await;
    let both = tls_to(&running, &certificate, &[ALPN_HTTP1, ALPN_H2]).await;
    assert_eq!(both.get_ref().1.alpn_protocol(), Some(ALPN_H2), "the server's order: h2 first");
    assert_eq!(both.get_ref().1.protocol_version(), Some(rustls::ProtocolVersion::TLSv1_3));
    let http1 = tls_to(&running, &certificate, &[ALPN_HTTP1]).await;
    assert_eq!(http1.get_ref().1.alpn_protocol(), Some(ALPN_HTTP1));
    let answer = http1_get(http1, &path("channel.ttx")).await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.to_ascii_lowercase().contains(&format!("alt-svc: h3=\":{}\"\r\n", running.address.port())), "{answer}");
    assert!(answer.ends_with(std::str::from_utf8(&running.descriptor).unwrap()), "{answer}");
    let missing = http1_get(tls_to(&running, &certificate, &[ALPN_HTTP1]).await, "/nothing/here").await;
    assert!(missing.starts_with("HTTP/1.1 404") && missing.to_ascii_lowercase().contains("alt-svc: h3="), "every answer: {missing}");
}

/// REQ-6965: `Alt-Svc` names the port viewers reach the server at, the
/// origin's, and not the port it listens on: 443 for an `https` origin
/// that names none, and the origin's own when it names another, as behind
/// a proxy that relays connections to the server (DEC-6972). The server
/// named its listening port before.
#[tokio::test]
async fn alt_svc_names_the_origins_port_and_not_the_listening_port() {
    const ELSEWHERE: u16 = 8443;
    let certificate = certificate("tls-public-port");
    for (public, port) in [(String::new(), 443), (format!(":{ELSEWHERE}"), ELSEWHERE)] {
        let running = running_at(Some(&certificate), |_| public.clone()).await;
        assert_ne!(running.address.port(), port, "the kernel chose another port to listen on");
        let answer = http1_get(tls_to(&running, &certificate, &[ALPN_HTTP1]).await, &path("channel.ttx")).await;
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        assert!(answer.to_ascii_lowercase().contains(&format!("alt-svc: h3=\":{port}\"\r\n")), "{answer}");
        assert_eq!(running.quic, Some(running.address), "QUIC is still served at the listening port");
    }
}

/// REQ-6965, REQ-6966: over TLS an HTTP/1.1 upgrade still opens the push
/// WebSocket, answered 101 with the key's accept value (RFC 6455 section
/// 1.3's example), and the session then carries the WebSocket's frames.
#[tokio::test]
async fn an_http1_upgrade_over_tls_opens_the_push_websocket() {
    let certificate = certificate("tls-upgrade");
    let running = running(Some(&certificate)).await;
    let mut session = tls_to(&running, &certificate, &[ALPN_HTTP1]).await;
    let upgrade = format!(
        "GET {} HTTP/1.1\r\nHost: {HOST}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: {VERSION}\r\nSec-WebSocket-Protocol: {PUSH_SUBPROTOCOL}\r\n\r\n",
        path("push")
    );
    session.write_all(upgrade.as_bytes()).await.unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(timeout(WITHIN, session.read_u8()).await.expect("an answer in time").unwrap());
    }
    let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
    assert!(head.starts_with("http/1.1 101"), "{head}");
    assert!(head.contains("sec-websocket-accept: s3pplmbitxaq9kygzzhzrbk+xoo=\r\n"), "{head}");
    assert!(head.contains("alt-svc: h3="), "{head}");
    session.write_all(&PING).await.unwrap();
    let mut frames = Frames::default();
    loop {
        if let Some((first, _)) = frames.next() {
            if first == PONG_OPCODE {
                break;
            }
            continue;
        }
        frames.0.push(timeout(WITHIN, session.read_u8()).await.expect("a pong in time").unwrap());
    }
}

/// REQ-6965: TLS 1.2 is taken on TCP as well as TLS 1.3.
#[tokio::test]
async fn tls_1_2_is_taken_on_tcp() {
    let certificate = certificate("tls-12");
    let running = running(Some(&certificate)).await;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate.der.clone()).unwrap();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS12])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let stream = TcpStream::connect(running.address).await.unwrap();
    let session = timeout(WITHIN, TlsConnector::from(Arc::new(config)).connect(ServerName::try_from(HOST).unwrap(), stream)).await.unwrap().unwrap();
    assert_eq!(session.get_ref().1.protocol_version(), Some(rustls::ProtocolVersion::TLSv1_2));
    assert!(http1_get(session, &path("channel.ttx")).await.starts_with("HTTP/1.1 200"));
}

/// REQ-6965: with TLS the server listens for QUIC on UDP at the TCP
/// listener's address and port, and answers with ALPN `h3`.
#[tokio::test]
async fn quic_answers_h3_on_the_tcp_port() {
    let certificate = certificate("quic");
    let running = running(Some(&certificate)).await;
    assert_eq!(running.quic, Some(running.address), "the same address and port");
    let h3 = h3_to(&running, &certificate).await;
    let data = h3.connection.handshake_data().unwrap().downcast::<HandshakeData>().unwrap();
    assert_eq!(data.protocol.as_deref(), Some(ALPN_H3));
    // The port is taken on UDP: no other socket can have it.
    assert_eq!(UdpSocket::bind(running.address).unwrap_err().kind(), std::io::ErrorKind::AddrInUse);
}

/// REQ-6965: without a certificate and key the server serves plain
/// HTTP/1.1 as before, names no HTTP/3, and has no UDP socket: the port
/// is free on UDP.
#[tokio::test]
async fn without_tls_it_is_plain_http1_and_no_udp_socket() {
    let running = running(None).await;
    assert_eq!(running.quic, None);
    let answer = http1_get(TcpStream::connect(running.address).await.unwrap(), &path("channel.ttx")).await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(!answer.to_ascii_lowercase().contains("alt-svc"), "{answer}");
    UdpSocket::bind(running.address).expect("nothing holds the port on UDP");
}

/// REQ-6965: a key that is not the certificate's, a file that is not
/// there, and a file that holds no certificate or no key each refuse,
/// naming the file.
#[test]
fn a_mismatched_or_unreadable_certificate_or_key_refuses_naming_the_file() {
    let ours = certificate("mismatch-ours");
    let other = certificate("mismatch-other");
    assert!(tls::load(&ours.files).is_ok());
    let named = |path: &PathBuf| path.display().to_string();
    let mismatched = tls::load(&Files { certificate: ours.files.certificate.clone(), key: other.files.key.clone() }).unwrap_err();
    assert_eq!(mismatched, format!("the private key {} is not the key of the certificate {}", named(&other.files.key), named(&ours.files.certificate)));
    let absent = ours.files.key.with_file_name("absent.pem");
    let error = tls::load(&Files { certificate: ours.files.certificate.clone(), key: absent.clone() }).unwrap_err();
    assert!(error.starts_with(&format!("cannot read the private key {}", named(&absent))), "{error}");
    let error = tls::load(&Files { certificate: absent.clone(), key: ours.files.key.clone() }).unwrap_err();
    assert!(error.starts_with(&format!("cannot read the certificate chain {}", named(&absent))), "{error}");
    // Each file given as the other: a key file holds no certificate, and
    // a certificate file no key.
    let error = tls::load(&Files { certificate: ours.files.key.clone(), key: ours.files.key.clone() }).unwrap_err();
    assert_eq!(error, format!("the certificate chain {} holds no certificate", named(&ours.files.key)));
    let error = tls::load(&Files { certificate: ours.files.certificate.clone(), key: ours.files.certificate.clone() }).unwrap_err();
    assert!(error.starts_with(&format!("cannot read the private key {}", named(&ours.files.certificate))), "{error}");
}

/// REQ-6965: the server binary itself stops at start, naming the file,
/// when its key is not its certificate's; and when given one of the two
/// options without the other.
#[test]
fn the_server_stops_at_start_with_a_mismatched_key() {
    let ours = certificate("binary-ours");
    let other = certificate("binary-other");
    let run = |arguments: &[&std::ffi::OsStr]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_teletext-server"))
            .args(["--listen", ANY_LOCAL_PORT, "--log-format", "text"])
            .args(arguments)
            .env_remove("STATE_DIRECTORY")
            .output()
            .unwrap();
        (output.status.success(), String::from_utf8(output.stderr).unwrap())
    };
    let (started, log) = run(&["--tls-certificate".as_ref(), ours.files.certificate.as_os_str(), "--tls-key".as_ref(), other.files.key.as_os_str()]);
    assert!(!started, "{log}");
    assert!(log.contains(&format!("the private key {} is not the key of the certificate", other.files.key.display())), "{log}");
    let (started, log) = run(&["--tls-certificate".as_ref(), ours.files.certificate.as_os_str()]);
    assert!(!started && log.contains("--tls-certificate needs --tls-key"), "{log}");
}
