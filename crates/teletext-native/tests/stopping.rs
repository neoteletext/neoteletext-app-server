//! The server binary stopping while it serves TLS, HTTP/2 and HTTP/3
//! itself (DEC-6964), and taking a new certificate without stopping. At
//! `SIGTERM` every connection is told the server is going away, GOAWAY
//! over HTTP/2 (RFC 9113 section 6.8) and over HTTP/3 (RFC 9114 section
//! 5.2); the channels close each WebSocket with the code that says try
//! again (DOC-1919); and every QUIC connection is closed with
//! `H3_NO_ERROR` (RFC 9000 section 10.2), so that a client learns of the
//! stop at once and not at its idle timeout, since a process started
//! after it cannot reset a QUIC connection the stopped one held (in
//! production on 20 November 2026 the browser's tabs on HTTP/3 took 15.2
//! seconds, its idle timeout, to learn of a restart). Before GOAWAY a
//! client could go on opening requests on a stopping server's connections
//! while its channels stopped. Unix only: the stop is a `SIGTERM`.
#![cfg(unix)]
#![allow(clippy::arithmetic_side_effects, reason = "a test")]

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode};
use neoteletext_http3::qpack;
use neoteletext_http3::stream::{CONTROL, Control, ControlEvent, Message, MessageEvent, Role, open_control};
use neoteletext_http3::{Code, frame};
use neoteletext_web::consts::{CLOSE_RESTARTING, PUSH_SUBPROTOCOL};
use quinn::crypto::rustls::QuicClientConfig;
use rustls::pki_types::{CertificateDer, ServerName};
use teletext_native::tls::{ALPN_H2, ALPN_H3};
use teletext_runtime::json::{self, Value};
use tokio::net::TcpStream;
use tokio::time::{Instant, timeout};
use tokio_rustls::TlsConnector;

/// The longest any step here may take.
const WITHIN: Duration = Duration::from_secs(20);
/// The name the test certificate is for.
const HOST: &str = "localhost";
/// The channel's folder.
const FOLDER: &str = "ch";
/// Lets the kernel choose the port.
const ANY_LOCAL_PORT: &str = "127.0.0.1:0";
/// The idle timeout of the browser's QUIC connections (the engine's
/// `neoteletext-transport`, its REQ-6985): a client that learns of a stop
/// only by its connection going idle learns of it this late.
const CLIENT_IDLE_AFTER: Duration = Duration::from_secs(15);
/// What "at once" is held to: a third of [`CLIENT_IDLE_AFTER`], so that a
/// close learnt only at the idle timeout fails, with room for a slow
/// machine's stop of its channels.
const PROMPTLY: Duration = Duration::from_secs(5);
/// A masked Ping of no payload, with the key 0 (RFC 6455 sections 5.2 and
/// 5.3), and the opcodes of the server's Pong and Close.
const PING: [u8; 6] = [0x89, 0x80, 0, 0, 0, 0];
const PONG_OPCODE: u8 = 0xA;
const CLOSE_OPCODE: u8 = 0x8;
/// `Sec-WebSocket-Version`'s one value (RFC 6455 section 4.1).
const VERSION: &str = "13";
const PUSH_HEADERS: [(&str, &str); 2] = [("sec-websocket-version", VERSION), ("sec-websocket-protocol", PUSH_SUBPROTOCOL)];

/// A folder of this test's own.
fn scratch(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("stopping-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A certificate for [`HOST`] and its key, written to `folder`, and the
/// certificate for a client to trust.
fn certificate(folder: &Path, name: &str) -> (PathBuf, PathBuf, CertificateDer<'static>) {
    let made = rcgen::generate_simple_self_signed(vec![HOST.to_owned()]).unwrap();
    let (certificate, key) = (folder.join(format!("{name}.crt")), folder.join(format!("{name}.key")));
    std::fs::write(&certificate, made.cert.pem()).unwrap();
    std::fs::write(&key, made.signing_key.serialize_pem()).unwrap();
    (certificate, key, made.cert.der().clone())
}

/// The server binary with TLS from the files named, and one channel whose
/// program logs once started and reads what it is sent.
struct Server {
    child: Child,
    address: SocketAddr,
    /// Its log, read up to its `serving` event.
    _stderr: BufReader<ChildStderr>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An event of the server's log in JSON, and its message.
fn message(event: &Value) -> Option<&str> {
    event.get("fields").and_then(|fields| fields.get("message")).and_then(Value::text)
}

impl Server {
    fn start(folder: &Path, certificate: &Path, key: &Path) -> Self {
        let channels = folder.join("channels.json");
        let program = r#"read -r _; printf '%s\n' '{"type":"log","level":"info","event":"started","text":"stopping"}'; while read -r _; do :; done"#;
        std::fs::write(&channels, format!(r#"{{"channels":[{{"folder":"{FOLDER}","name":"Channel","command":["sh","-c",{program:?}]}}]}}"#)).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_teletext-server"))
            .args(["--listen", ANY_LOCAL_PORT, "--origin", &format!("https://{HOST}"), "--log-format", "json", "--channels"])
            .arg(&channels)
            .arg("--tls-certificate")
            .arg(certificate)
            .arg("--tls-key")
            .arg(key)
            .env("RUST_LOG", "info")
            .env_remove("STATE_DIRECTORY")
            .env_remove("LISTEN_FDS")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        let serving = Self::until(&mut stderr, "serving");
        let address = serving.get("fields").and_then(|fields| fields.get("listen")).and_then(Value::text).unwrap().parse().unwrap();
        Self { child, address, _stderr: stderr }
    }

    /// The next event of the log whose message is `wanted`.
    fn until(stderr: &mut BufReader<ChildStderr>, wanted: &str) -> Value {
        loop {
            let mut line = String::new();
            assert_ne!(stderr.read_line(&mut line).unwrap(), 0, "the server's log ended before {wanted:?}");
            let event = json::parse(line.trim_end().as_bytes()).unwrap();
            if message(&event) == Some(wanted) {
                return event;
            }
        }
    }

    fn signal(&self, name: &str) {
        assert!(Command::new("kill").args([&format!("-{name}"), &self.child.id().to_string()]).status().unwrap().success());
    }

    /// Waits for the server to exit, and answers whether it exited 0.
    async fn exited(&mut self) -> bool {
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.success();
            }
            assert!(started.elapsed() < WITHIN, "the server did not stop");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// A client's TLS that trusts `trusted` alone and offers `alpn`.
fn client_tls(trusted: &CertificateDer<'static>, alpn: &[u8]) -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(trusted.clone()).unwrap();
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![alpn.to_vec()];
    config
}

/// The WebSocket frames from the server, from bytes as they come: the
/// next whole frame's opcode and payload (RFC 6455 section 5.2; the
/// server's frames are short and unmasked here).
fn next_frame(bytes: &mut Vec<u8>) -> Option<(u8, Vec<u8>)> {
    let (&first, &second) = (bytes.first()?, bytes.get(1)?);
    let (length, header) = match second {
        126 => (usize::from(u16::from_be_bytes(bytes.get(2..4)?.try_into().unwrap())), 4),
        127 => (usize::try_from(u64::from_be_bytes(bytes.get(2..10)?.try_into().unwrap())).unwrap(), 10),
        short => (usize::from(short), 2),
    };
    let payload = bytes.get(header..header + length)?.to_vec();
    bytes.drain(..header + length);
    Some((first & 0x0F, payload))
}

/// The status code of a Close frame's payload (RFC 6455 section 5.5.1).
fn close_code(payload: &[u8]) -> u16 {
    u16::from_be_bytes([payload[0], payload[1]])
}

// ---------------------------------------------------------------- HTTP/3

/// An HTTP/3 connection: QUIC's, this side's control stream, kept open
/// (RFC 9114 section 6.2.1), and the server's, read past its SETTINGS.
struct H3 {
    connection: quinn::Connection,
    endpoint: quinn::Endpoint,
    _control: quinn::SendStream,
    theirs: quinn::RecvStream,
    reader: Control,
}

async fn h3_to(address: SocketAddr, trusted: &CertificateDer<'static>) -> H3 {
    let mut endpoint = quinn::Endpoint::client(ANY_LOCAL_PORT.parse().unwrap()).unwrap();
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(CLIENT_IDLE_AFTER.try_into().unwrap()));
    let mut config = quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(client_tls(trusted, ALPN_H3)).unwrap()));
    config.transport_config(Arc::new(transport));
    endpoint.set_default_client_config(config);
    let connection = timeout(WITHIN, endpoint.connect(address, HOST).unwrap()).await.expect("a QUIC handshake in time").unwrap();
    let mut control = connection.open_uni().await.unwrap();
    let mut opening = Vec::new();
    open_control(&mut opening);
    control.write_all(&opening).await.unwrap();
    let mut theirs = timeout(WITHIN, connection.accept_uni()).await.expect("the server's control stream in time").unwrap();
    let mut kind = [0_u8; 1];
    theirs.read_exact(&mut kind).await.unwrap();
    assert_eq!(u64::from(kind[0]), CONTROL);
    let mut h3 = H3 { connection, endpoint, _control: control, theirs, reader: Control::new(Role::Client) };
    assert!(matches!(h3.control_event().await, Some(ControlEvent::Settings(_))), "SETTINGS first");
    h3
}

impl H3 {
    /// The next event of the server's control stream, a byte at a time so
    /// that nothing past it is read; `None` when the stream ends.
    async fn control_event(&mut self) -> Option<ControlEvent> {
        loop {
            let mut byte = [0_u8; 1];
            if self.theirs.read_exact(&mut byte).await.is_err() {
                return None;
            }
            if let (_, Some(event)) = self.reader.read(&byte).unwrap() {
                return Some(event);
            }
        }
    }

    /// A push WebSocket opened by extended CONNECT (RFC 9220), answered
    /// 200, which has answered a ping.
    async fn push(&self, address: SocketAddr) -> Open {
        let (mut send, mut receive) = self.connection.open_bi().await.unwrap();
        let authority = format!("{HOST}:{}", address.port());
        let path = format!("/{FOLDER}/push");
        let fields = [(":method", "CONNECT"), (":protocol", "websocket"), (":scheme", "https"), (":authority", &authority), (":path", &path)];
        let mut section = Vec::new();
        qpack::encode(fields.iter().chain(&PUSH_HEADERS).map(|(name, value)| (name.as_bytes(), value.as_bytes())), &mut section);
        let mut head = Vec::new();
        frame::write(frame::HEADERS, &section, &mut head);
        send.write_all(&head).await.unwrap();
        let mut socket = H3WebSocket { send, message: Message::new(), raw: Vec::new(), bytes: Vec::new() };
        let status = loop {
            let mut buffer = [0_u8; 1_024];
            let count = timeout(WITHIN, receive.read(&mut buffer)).await.expect("an answer in time").unwrap().expect("an answer");
            socket.raw.extend_from_slice(&buffer[..count]);
            if let Some(section) = socket.head() {
                break qpack::decode(&section, u64::MAX).unwrap().remove(0);
            }
        };
        assert_eq!((status.name.as_slice(), status.value.as_slice()), (b":status".as_slice(), b"200".as_slice()));
        let mut socket = socket.with(receive);
        let mut ping = Vec::new();
        frame::write(frame::DATA, &PING, &mut ping);
        socket.socket.send.write_all(&ping).await.unwrap();
        assert_eq!(socket.next_frame().await.map(|(opcode, _)| opcode), Some(PONG_OPCODE));
        socket
    }
}

/// A WebSocket on an HTTP/3 request stream, before its answer's head is
/// whole.
struct H3WebSocket {
    send: quinn::SendStream,
    message: Message,
    /// What came of the stream, not yet read as frames.
    raw: Vec<u8>,
    /// The WebSocket's bytes, the DATA frames' payload, not yet read.
    bytes: Vec<u8>,
}

impl H3WebSocket {
    /// Reads what has come as HTTP/3 frames: the head's field section once
    /// it is whole, and DATA's payload into `bytes`.
    fn head(&mut self) -> Option<Vec<u8>> {
        let mut head = None;
        while !self.raw.is_empty() {
            let (taken, event) = self.message.read(&self.raw).unwrap();
            match event {
                Some(MessageEvent::Head(section)) => head = Some(section),
                Some(MessageEvent::Data(part)) => self.bytes.extend_from_slice(part),
                _ => {}
            }
            self.raw.drain(..taken);
            if taken == 0 {
                break;
            }
        }
        head
    }

    fn with(self, receive: quinn::RecvStream) -> Open {
        Open { socket: self, receive }
    }
}

/// A WebSocket on an HTTP/3 request stream, open.
struct Open {
    socket: H3WebSocket,
    receive: quinn::RecvStream,
}

impl Open {
    /// The server's next frame: its opcode and payload; `None` when the
    /// stream ends.
    async fn next_frame(&mut self) -> Option<(u8, Vec<u8>)> {
        loop {
            if let Some(frame) = next_frame(&mut self.socket.bytes) {
                return Some(frame);
            }
            let mut buffer = [0_u8; 1_024];
            let count = timeout(WITHIN, self.receive.read(&mut buffer)).await.expect("a frame in time").ok()??;
            self.socket.raw.extend_from_slice(&buffer[..count]);
            let _ = self.socket.head();
        }
    }
}

/// When each thing a stop tells a client came, from the `SIGTERM`.
#[derive(Debug, Default)]
struct Told {
    goaway: Option<Duration>,
    websocket_closed: Option<Duration>,
    connection_closed: Option<Duration>,
}

/// REQ-7002, REQ-6966: at `SIGTERM` an HTTP/3 client is sent GOAWAY on the
/// server's control stream naming the first request stream not yet
/// accepted (here 4, after the one push WebSocket's stream 0), then its
/// push WebSocket's Close with the code that says try again, and then the
/// QUIC connection's close with `H3_NO_ERROR`, in that order and each
/// within [`PROMPTLY`] of the signal, well before the client's idle
/// timeout, [`CLIENT_IDLE_AFTER`]; and the server exits 0. Before REQ-7002
/// no GOAWAY was sent.
#[tokio::test]
async fn a_stop_tells_an_http3_client_at_once() {
    let folder = scratch("h3");
    let (certificate, key, trusted) = certificate(&folder, "server");
    let mut server = Server::start(&folder, &certificate, &key);
    let mut h3 = h3_to(server.address, &trusted).await;
    let mut socket = h3.push(server.address).await;
    let connection = h3.connection.clone();

    let signalled = Instant::now();
    server.signal("TERM");
    let control = tokio::spawn(async move {
        let event = timeout(WITHIN, h3.control_event()).await.expect("the control stream in time");
        (event, signalled.elapsed(), h3)
    });
    let websocket = tokio::spawn(async move {
        while let Some((opcode, payload)) = socket.next_frame().await {
            if opcode == CLOSE_OPCODE {
                return Some((close_code(&payload), signalled.elapsed()));
            }
        }
        None
    });
    let closed = timeout(WITHIN, connection.closed()).await.expect("the connection closed in time");
    let connection_closed = signalled.elapsed();
    let (event, goaway_at, h3) = control.await.unwrap();
    let closed_websocket = websocket.await.unwrap();
    eprintln!(
        "HTTP/3, from SIGTERM: the control stream's next event {event:?} at {goaway_at:?}, the WebSocket's Close {closed_websocket:?}, the connection's close {closed:?} at {connection_closed:?}"
    );
    assert!(matches!(event, Some(ControlEvent::GoAway(4))), "GOAWAY naming stream 4, not {event:?}");
    let (code, websocket_at) = closed_websocket.expect("the WebSocket's Close before its stream ended");
    assert_eq!(code, CLOSE_RESTARTING, "try again");
    match closed {
        quinn::ConnectionError::ApplicationClosed(close) => assert_eq!(close.error_code.into_inner(), u64::from(Code::NoError.value())),
        other => panic!("closed by the server with H3_NO_ERROR, not {other:?}"),
    }
    let told = Told { goaway: Some(goaway_at), websocket_closed: Some(websocket_at), connection_closed: Some(connection_closed) };
    let (goaway, websocket_closed, connection_closed) = (told.goaway.unwrap(), told.websocket_closed.unwrap(), told.connection_closed.unwrap());
    assert!(goaway <= websocket_closed && websocket_closed <= connection_closed, "in order: {told:?}");
    assert!(connection_closed < PROMPTLY, "at once, not at the idle timeout: {told:?}");
    assert!(server.exited().await, "exit status 0");
    h3.endpoint.wait_idle().await;
    let _ = std::fs::remove_dir_all(&folder);
}

// ---------------------------------------------------------------- HTTP/2

/// REQ-7002: at `SIGTERM` an HTTP/2 client is sent GOAWAY with
/// `NO_ERROR` (RFC 9113 section 6.8), after which it can open no stream,
/// and its push WebSocket, opened by extended CONNECT before, is still
/// sent its Close with the code that says try again.
#[tokio::test]
async fn a_stop_sends_an_http2_client_goaway() {
    let folder = scratch("h2");
    let (certificate, key, trusted) = certificate(&folder, "server");
    let mut server = Server::start(&folder, &certificate, &key);
    let stream = TcpStream::connect(server.address).await.unwrap();
    let session = TlsConnector::from(Arc::new(client_tls(&trusted, ALPN_H2))).connect(ServerName::try_from(HOST).unwrap(), stream).await.unwrap();
    let (sender, connection) = h2::client::handshake(session).await.unwrap();
    tokio::spawn(connection);
    let mut request = Request::builder().method(Method::CONNECT).uri(format!("https://{HOST}:{}/{FOLDER}/push", server.address.port()));
    for (name, value) in PUSH_HEADERS {
        request = request.header(name, value);
    }
    let mut request = request.body(()).unwrap();
    request.extensions_mut().insert(h2::ext::Protocol::from("websocket"));
    let mut ready = timeout(WITHIN, sender.clone().ready()).await.unwrap().unwrap();
    let (response, mut send) = ready.send_request(request, false).unwrap();
    let response = timeout(WITHIN, response).await.unwrap().unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut receive = response.into_body();
    send.send_data(Bytes::from_static(&PING), false).unwrap();

    let mut bytes = Vec::new();
    let mut signalled = None;
    let code = loop {
        if let Some((opcode, payload)) = next_frame(&mut bytes) {
            match opcode {
                // The ping answered: the WebSocket is open, so stop now.
                PONG_OPCODE => {
                    signalled = Some(Instant::now());
                    server.signal("TERM");
                }
                CLOSE_OPCODE => break close_code(&payload),
                _ => {}
            }
            continue;
        }
        let part = timeout(WITHIN, receive.data()).await.expect("a frame in time").expect("the stream stays open until its Close").unwrap();
        receive.flow_control().release_capacity(part.len()).unwrap();
        bytes.extend_from_slice(&part);
    };
    assert_eq!(code, CLOSE_RESTARTING, "try again");
    let refused = match timeout(WITHIN, sender.ready()).await.expect("an answer in time") {
        Ok(mut ready) => ready.send_request(Request::get(format!("https://{HOST}/{FOLDER}/channel.ttx")).body(()).unwrap(), true).map(|_| ()).unwrap_err(),
        Err(error) => error,
    };
    eprintln!("HTTP/2, from SIGTERM to the WebSocket's Close: {:?}", signalled.map(|at| at.elapsed()));
    assert!(refused.is_go_away() && refused.is_remote(), "GOAWAY from the server, not {refused:?}");
    assert_eq!(refused.reason(), Some(h2::Reason::NO_ERROR));
    assert!(server.exited().await, "exit status 0");
    let _ = std::fs::remove_dir_all(&folder);
}
