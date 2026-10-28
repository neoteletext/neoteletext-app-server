//! The server binary through its life: its log at start, its probes and
//! metrics, sessions that continue across connections, a clean stop, and
//! a restart that serves what it kept (requirements 5.1 to 5.5, 7.1, 7.2,
//! 8.1 and 8.4). Unix only: the stop is a `SIGTERM`.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use teletext_runtime::json::{self, Value};

/// How long the server and its program have to start, or to stop.
const WITHIN: Duration = Duration::from_secs(30);
/// Between tries.
const RETRY: Duration = Duration::from_millis(50);
/// The WebSocket key of RFC 6455 section 1.3's example.
const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
/// A Close frame's opcode (RFC 6455 section 5.2).
const CLOSE: u8 = 0x8;

/// The server, killed when the test ends however it ends.
struct Server {
    child: Child,
    address: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    /// Sends the server `SIGTERM` and waits for it to exit.
    fn terminate(&mut self) -> ExitStatus {
        let sent = Command::new("kill").args(["-TERM", &self.child.id().to_string()]).status().unwrap();
        assert!(sent.success());
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(started.elapsed() < WITHIN, "the server did not stop");
            std::thread::sleep(RETRY);
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("teletext-lifecycle-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A port nothing listens on now.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A channels file for one channel, `ch`, whose program runs `script` in `sh`.
fn channels(folder: &Path, script: &str) -> PathBuf {
    let file = folder.join("channels.json");
    std::fs::write(&file, format!(r#"{{"channels":[{{"folder":"ch","name":"Channel","command":["sh","-c",{script:?}]}}]}}"#)).unwrap();
    file
}

/// Declares page 100 and, once started, publishes it saying `text`.
fn publishing(text: &str) -> String {
    format!(
        r#"printf '%s\n' '{{"type":"pages","pages":[{{"page":"100","title":"Front"}}]}}'; read -r _; printf '%s\n' '{{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nOL,1,{text}\n"}}'; exec cat >/dev/null"#
    )
}

/// Starts the server with `arguments` beside its address and origin.
fn serve(arguments: &[&str], environment: &[(&str, &str)], stderr: Stdio) -> Server {
    let address = format!("127.0.0.1:{}", free_port());
    let mut command = Command::new(env!("CARGO_BIN_EXE_teletext-server"));
    command.args(["--listen", &address, "--origin", &format!("http://{address}")]).args(arguments).stdin(Stdio::null()).stdout(Stdio::null()).stderr(stderr);
    command.env_remove("RUST_LOG").env_remove("STATE_DIRECTORY");
    for (name, value) in environment {
        command.env(name, value);
    }
    Server { child: command.spawn().unwrap(), address }
}

/// GET `path`: the status and the body, or `None` while nothing answers.
fn get(address: &str, path: &str) -> Option<(u16, Vec<u8>)> {
    let mut stream = TcpStream::connect(address).ok()?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let end = response.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&response[..end]).into_owned();
    let status = head.split(' ').nth(1)?.parse().ok()?;
    Some((status, response[end + 4..].to_vec()))
}

/// GET `path` until it answers `status`.
fn until_status(address: &str, path: &str, status: u16) -> Vec<u8> {
    let started = Instant::now();
    loop {
        if let Some((answered, body)) = get(address, path)
            && answered == status
        {
            return body;
        }
        assert!(started.elapsed() < WITHIN, "{path} did not answer {status}");
        std::thread::sleep(RETRY);
    }
}

/// A push connection, open, and its response's `Set-Cookie`, if any.
fn push(address: &str, cookie: Option<&str>) -> (TcpStream, Option<String>) {
    let mut stream = TcpStream::connect(address).unwrap();
    let cookie = cookie.map(|cookie| format!("Cookie: {cookie}\r\n")).unwrap_or_default();
    write!(stream, "GET /ch/push HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Version: 13\r\n{cookie}\r\n").unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    let set_cookie = head.lines().find_map(|line| line.strip_prefix("set-cookie: ").or_else(|| line.strip_prefix("Set-Cookie: "))).map(str::to_owned);
    (stream, set_cookie)
}

/// The session a `Set-Cookie` names: the number before its first dot.
fn session_of(set_cookie: &str) -> u64 {
    set_cookie.strip_prefix("_session=").unwrap().split('.').next().unwrap().parse().unwrap()
}

/// Reads the server's frames until a Close, and answers its status code.
fn close_code(stream: &mut TcpStream) -> u16 {
    stream.set_read_timeout(Some(WITHIN)).unwrap();
    loop {
        let mut head = [0_u8; 2];
        stream.read_exact(&mut head).unwrap();
        let length = match head[1] & 0x7F {
            126 => {
                let mut bytes = [0_u8; 2];
                stream.read_exact(&mut bytes).unwrap();
                usize::from(u16::from_be_bytes(bytes))
            }
            127 => {
                let mut bytes = [0_u8; 8];
                stream.read_exact(&mut bytes).unwrap();
                usize::try_from(u64::from_be_bytes(bytes)).unwrap()
            }
            short => usize::from(short),
        };
        let mut payload = vec![0_u8; length];
        stream.read_exact(&mut payload).unwrap();
        if head[0] & 0x0F == CLOSE {
            return u16::from_be_bytes([payload[0], payload[1]]);
        }
    }
}

/// A stopped server closes its viewers' connections with 1012 and exits
/// with 0; started again with its state directory, it serves the page its
/// program published before, although the program has not published it
/// again (requirements 5.1, 5.2 and 5.3).
#[test]
fn a_restart_closes_viewers_as_a_restart_and_serves_what_was_kept() {
    let folder = scratch("restart");
    let state = folder.join("state");
    let state_argument = state.display().to_string();
    let first_channels = channels(&folder, &publishing("Kept across a restart"));
    let mut first = serve(&["--channels", &first_channels.display().to_string(), "--state", &state_argument], &[], Stdio::null());
    let published = until_status(&first.address, "/ch/100.t42", 200);
    let (mut viewer, _) = push(&first.address, None);
    assert!(first.terminate().success());
    assert_eq!(close_code(&mut viewer), 1012, "service restart");

    // A program that declares the page and never publishes it.
    let silent = r#"printf '%s\n' '{"type":"pages","pages":[{"page":"100","title":"Front"}]}'; exec cat >/dev/null"#;
    let second_channels = channels(&folder, silent);
    let second = serve(&["--channels", &second_channels.display().to_string(), "--state", &state_argument], &[], Stdio::null());
    assert_eq!(until_status(&second.address, "/ch/100.t42", 200), published, "the page as it was last published");
    let contents = String::from_utf8(until_status(&second.address, "/ch/contents.txt", 200)).unwrap();
    assert!(contents.contains("p\t100\tFront\n"), "{contents}");
    let _ = std::fs::remove_dir_all(&folder);
}

/// A connection presenting its session's cookie continues that session;
/// a second connection while the first is open is given a session of its
/// own and no cookie; a forged cookie is ignored (requirement 5.5).
#[test]
fn a_session_continues_with_its_cookie() {
    let folder = scratch("cookie");
    let file = channels(&folder, &publishing("Hello"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, "/ch/channel.json", 200);
    let (first, set_cookie) = push(&server.address, None);
    let set_cookie = set_cookie.expect("a new session is given a cookie");
    assert!(set_cookie.contains("; Path=/ch/; Max-Age=2592000; HttpOnly; SameSite=Strict"), "{set_cookie}");
    assert!(!set_cookie.contains("Secure"), "not over http");
    let session = session_of(&set_cookie);
    let cookie = set_cookie.split(';').next().unwrap().to_owned();

    let (_while_open, second_cookie) = push(&server.address, Some(&cookie));
    assert_eq!(second_cookie, None, "a session connected already is not shared");

    let forged = format!("_session={}.9999999999.{}", session + 1, "0".repeat(64));
    let (_forger, forged_cookie) = push(&server.address, Some(&forged));
    assert_ne!(forged_cookie.map(|cookie| session_of(&cookie)), Some(session + 1), "a forged cookie names nothing");

    drop(first);
    let started = Instant::now();
    loop {
        let (_again, renewed) = push(&server.address, Some(&cookie));
        if let Some(renewed) = renewed {
            assert_eq!(session_of(&renewed), session, "the session continues");
            break;
        }
        // The first connection's close has not reached the server yet.
        assert!(started.elapsed() < WITHIN, "the session was not continued");
        std::thread::sleep(RETRY);
    }
}

/// The administration listener answers `/healthz`; `/readyz` is 503 until
/// the channel's program declares its pages and 200 after; the channel is
/// served meanwhile; `/metrics` is there, and not on the viewers' listener
/// (requirements 5.3, 8.1 and 8.4).
#[test]
fn probes_and_metrics_are_on_the_administration_listener() {
    let folder = scratch("admin");
    let gate = folder.join("declare");
    let script = format!("while [ ! -e {gate:?} ]; do sleep 0.05; done; {}", publishing("Ready"));
    let file = channels(&folder, &script);
    let admin = format!("127.0.0.1:{}", free_port());
    let server = serve(&["--channels", &file.display().to_string(), "--admin-listen", &admin], &[], Stdio::null());
    assert_eq!(until_status(&admin, "/healthz", 200), b"ok\n");
    assert_eq!(until_status(&admin, "/readyz", 503), b"no pages yet: ch\n");
    assert_eq!(get(&server.address, "/ch/channel.json").map(|(status, _)| status), Some(200), "served before the program declares");
    std::fs::write(&gate, b"").unwrap();
    assert_eq!(until_status(&admin, "/readyz", 200), b"ready\n");
    let metrics = String::from_utf8(until_status(&admin, "/metrics", 200)).unwrap();
    assert!(metrics.contains("# TYPE teletext_connections gauge\n"), "{metrics}");
    assert!(metrics.contains("teletext_channel_ready{channel=\"ch\"} 1\n"), "{metrics}");
    assert_eq!(get(&server.address, "/metrics").map(|(status, _)| status), Some(404), "not where viewers reach");
    let _ = std::fs::remove_dir_all(&folder);
}

/// With standard error a pipe, the server writes one JSON object a line,
/// and its start is an `INFO` event; `RUST_LOG=warn` leaves it out
/// (requirements 7.1 and 7.2).
#[test]
fn the_log_is_json_lines_and_rust_log_chooses_the_level() {
    let mut server = serve(&[], &[], Stdio::piped());
    let mut stderr = BufReader::new(server.child.stderr.take().unwrap());
    let mut line = String::new();
    stderr.read_line(&mut line).unwrap();
    let event = json::parse(line.trim_end().as_bytes()).unwrap();
    assert_eq!(event.get("level").and_then(Value::text), Some("INFO"), "{line}");
    let fields = event.get("fields").unwrap();
    assert_eq!(fields.get("message").and_then(Value::text), Some("serving"), "{line}");
    assert_eq!(fields.get("listen").and_then(Value::text), Some(server.address.as_str()), "{line}");
    assert!(event.get("timestamp").and_then(Value::text).is_some(), "{line}");
    assert!(server.terminate().success());

    let mut quiet = serve(&[], &[("RUST_LOG", "warn")], Stdio::piped());
    until_status(&quiet.address, "/", 200);
    assert!(quiet.terminate().success());
    let mut written = String::new();
    quiet.child.stderr.take().unwrap().read_to_string(&mut written).unwrap();
    assert_eq!(written, "", "nothing below WARN");
}
