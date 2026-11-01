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

use neoteletext_core::consts::LINK_SCHEME;
use neoteletext_input::caps::SELECT_CAP;
use neoteletext_input::event::{Event, Item, Select, write as write_item};
use neoteletext_web::channel::SETTINGS_FILE;
use teletext_runtime::json::{self, Value};

/// How long the server and its program have to start, or to stop.
const WITHIN: Duration = Duration::from_secs(30);
/// Between tries.
const RETRY: Duration = Duration::from_millis(50);
/// The WebSocket key of RFC 6455 section 1.3's example.
const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
/// A final binary frame's first byte (RFC 6455 section 5.2).
const BINARY_FINAL: u8 = 0x82;
/// The mask bit with the length 126, which says two length bytes follow.
const MASKED_LONG: u8 = 0xFE;
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

/// The path of the entry file of the channel in `folder`.
fn settings(folder: &str) -> String {
    format!("/{folder}/{SETTINGS_FILE}")
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
    channel_in(folder, "ch", script)
}

/// A channels file for one channel in `channel`.
fn channel_in(folder: &Path, channel: &str, script: &str) -> PathBuf {
    let file = folder.join("channels.json");
    std::fs::write(&file, format!(r#"{{"channels":[{{"folder":"{channel}","name":"Channel","command":["sh","-c",{script:?}]}}]}}"#)).unwrap();
    file
}

/// A program that logs `text` once started, and reads what it is sent,
/// its output kept open: a program whose output closes has ended.
fn publishing(text: &str) -> String {
    format!(r#"read -r _; printf '%s\n' '{{"type":"log","level":"info","event":"started","text":"{text}"}}'; while read -r _; do :; done"#)
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

/// GET `target`: the whole head of the response and its body.
fn fetched(address: &str, target: &str) -> (String, Vec<u8>) {
    let mut stream = TcpStream::connect(address).unwrap();
    write!(stream, "GET {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let end = response.windows(4).position(|window| window == b"\r\n\r\n").unwrap();
    (String::from_utf8_lossy(&response[..end]).into_owned(), response[end + 4..].to_vec())
}

/// A header of a response's head.
fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| line.split_once(": ").filter(|(found, _)| found.eq_ignore_ascii_case(name)).map(|(_, value)| value))
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
    push_to(address, "/ch/push", cookie)
}

/// A push connection to `target`, open, and its response's `Set-Cookie`.
fn push_to(address: &str, target: &str, cookie: Option<&str>) -> (TcpStream, Option<String>) {
    let mut stream = TcpStream::connect(address).unwrap();
    let cookie = cookie.map(|cookie| format!("Cookie: {cookie}\r\n")).unwrap_or_default();
    write!(stream, "GET {target} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Version: 13\r\n{cookie}\r\n").unwrap();
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
/// with 0 (requirement 5.1).
#[test]
fn a_restart_closes_viewers_as_a_restart() {
    let folder = scratch("restart");
    let state = folder.join("state");
    let state_argument = state.display().to_string();
    let first_channels = channels(&folder, &publishing("Closed as a restart"));
    let mut first = serve(&["--channels", &first_channels.display().to_string(), "--state", &state_argument], &[], Stdio::null());
    until_status(&first.address, &settings("ch"), 200);
    let (mut viewer, _) = push(&first.address, None);
    assert!(first.terminate().success());
    assert_eq!(close_code(&mut viewer), 1012, "service restart");
    let _ = std::fs::remove_dir_all(&folder);
}

/// The items as one masked binary WebSocket message (RFC 6455 section 5.2),
/// with a key of zeros, so that the masked payload is the payload.
fn send_input(stream: &mut TcpStream, items: &[Item<'_>]) {
    let mut payload = Vec::new();
    for item in items {
        let mut one = [0_u8; neoteletext_input::MAX_MESSAGE];
        let length = write_item(item, &mut one).unwrap();
        payload.extend_from_slice(&one[..length]);
    }
    let length = u16::try_from(payload.len()).unwrap().to_be_bytes();
    stream.write_all(&[BINARY_FINAL, MASKED_LONG, length[0], length[1], 0, 0, 0, 0]).unwrap();
    stream.write_all(&payload).unwrap();
}

/// REQ-1693 item 6: page parameters of the longest length reach the program
/// through a real socket, and the next selection leaves them behind.
#[test]
fn page_parameters_reach_the_program_in_selected() {
    let folder = scratch("parameters");
    let heard = folder.join("heard");
    let script = format!(r#"read -r _; while read -r line; do printf '%s\n' "$line" >> {heard:?}; done"#);
    let file = channels(&folder, &script);
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("ch"), 200);
    let (mut viewer, _) = push(&server.address, None);
    let parameters = format!("a={}", "x".repeat(neoteletext_input::MAX_PAGE_PARAMETERS - 2));
    let select = |page, parameters| Item::Event(Event::Select(Select::new(page, 0x3F7F).unwrap(), parameters));
    let hello = Item::Event(Event::Hello { version: neoteletext_input::VERSION, caps: SELECT_CAP, agent: None });
    send_input(&mut viewer, &[hello, select(0x302, &parameters)]);
    send_input(&mut viewer, &[select(0x303, "")]);
    let started = Instant::now();
    let told = loop {
        let told = std::fs::read_to_string(&heard).unwrap_or_default();
        if told.matches("\"selected\"").count() == 2 {
            break told;
        }
        assert!(started.elapsed() < WITHIN, "the program heard: {told:.300}");
        std::thread::sleep(RETRY);
    };
    let selected: Vec<Value> =
        told.lines().map(|line| json::parse(line.as_bytes()).unwrap()).filter(|line| line.get("type").and_then(Value::text) == Some("selected")).collect();
    assert_eq!(selected[0].get("query").and_then(Value::text), Some(parameters.as_str()));
    assert_eq!(selected[1].get("query").and_then(Value::text), None);
}

/// REQ-1693's Verification: a running server against the one table.
#[test]
fn a_channels_address_answers_as_the_conformance_table_says() {
    let folder = scratch("map");
    let file = channel_in(&folder, "home", &publishing("Hello"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("home"), 200);
    let mut rows = 0;
    for line in neoteletext_web::map::CONFORMANCE.lines().filter(|line| line.starts_with("answer\t")) {
        let columns: Vec<&str> = line.split('\t').collect();
        let [_, target, status, location] = columns[..] else { panic!("{line}") };
        let (head, _) = fetched(&server.address, target);
        assert_eq!(head.split(' ').nth(1), Some(status), "{line}: {head}");
        assert_eq!(header(&head, "location"), Some(location).filter(|location| *location != "-"), "{line}: {head}");
        rows += 1;
    }
    assert!(rows >= 18, "{rows} rows");
}

/// DEC-1726: the entry file is JSON whatever its extension says, so the server
/// says so, and a receiver reads it the same under any type. The deploy
/// script names the file literally, so it must agree with the constant.
#[test]
fn the_entry_file_is_served_as_json_and_the_old_name_is_not_found() {
    let folder = scratch("entry");
    let file = channel_in(&folder, "home", &publishing("Hello"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("home"), 200);
    let (head, _) = fetched(&server.address, &settings("home"));
    assert_eq!(header(&head, "content-type"), Some("application/json"), "{head}");
    assert_eq!(get(&server.address, "/home/channel.json").map(|(status, _)| status), Some(404));
    assert!(include_str!("../../../deploy/ionos/deploy.sh").contains(&format!("/$c/{SETTINGS_FILE}")));
}

/// DEC-1669: the small page names the channel, offers the browser, and may
/// run only its own script; REQ-1693 item 2: `channel.ttx` names no address.
#[test]
fn a_web_browser_is_offered_the_channel() {
    let folder = scratch("page");
    let file = channel_in(&folder, "home", &publishing("Hello"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("home"), 200);
    let (head, body) = fetched(&server.address, "/home/");
    let page = String::from_utf8(body).unwrap();
    assert!(page.contains(&format!(r#"href="{LINK_SCHEME}http://{}/home/""#, server.address)), "{page}");
    assert!(header(&head, "content-type").is_some_and(|kind| kind.starts_with("text/html")), "{head}");
    assert!(header(&head, "content-security-policy").is_some_and(|policy| policy.starts_with("default-src 'none'")), "{head}");
    assert_eq!(header(&head, "strict-transport-security"), None);
    let (_, descriptor) = fetched(&server.address, &settings("home"));
    let descriptor = String::from_utf8(descriptor).unwrap();
    assert!(!descriptor.contains("address"), "{descriptor}");
}

/// A web browser's page is for reading: nothing else reaches it.
#[test]
fn only_a_read_reaches_the_page() {
    let folder = scratch("post");
    let file = channel_in(&folder, "home", &publishing("Hello"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("home"), 200);
    let mut stream = TcpStream::connect(&server.address).unwrap();
    write!(stream, "POST /home/ HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", server.address).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert_eq!(response.split(' ').nth(1), Some("404"), "{response}");
}

/// REQ-1661 item 4: a request target of 8,000 octets is answered, and hyper,
/// which holds 65,534 octets at most, answers a longer one with 414.
#[test]
fn a_long_request_target_is_answered() {
    let folder = scratch("target");
    let file = channels(&folder, &publishing("Hello"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("ch"), 200);
    let path = |length: usize| format!("/ch/{}", "a".repeat(length - "/ch/".len()));
    assert_eq!(get(&server.address, &path(8_000)).map(|(status, _)| status), Some(404), "no such file, but answered");
    assert_eq!(get(&server.address, &path(65_000)).map(|(status, _)| status), Some(404));
    assert_eq!(get(&server.address, &path(66_000)).map(|(status, _)| status), Some(414));
}

/// A connection presenting its session's cookie continues that session,
/// a second connection, another tab, while the first is open included
/// (DEC-1527); a forged cookie is ignored (requirement 5.5).
#[test]
fn a_session_continues_with_its_cookie() {
    let folder = scratch("cookie");
    let file = channels(&folder, &publishing("Hello"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("ch"), 200);
    let (first, set_cookie) = push(&server.address, None);
    let set_cookie = set_cookie.expect("a new session is given a cookie");
    assert!(set_cookie.contains("; Path=/ch/; Max-Age=2592000; HttpOnly; SameSite=Strict"), "{set_cookie}");
    assert!(!set_cookie.contains("Secure"), "not over http");
    let session = session_of(&set_cookie);
    let cookie = set_cookie.split(';').next().unwrap().to_owned();

    let (_while_open, second_cookie) = push(&server.address, Some(&cookie));
    assert_eq!(second_cookie.map(|cookie| session_of(&cookie)), Some(session), "another tab continues the session");

    // A session number the server would never give next.
    let claimed = session + 1_000;
    let forged = format!("_session={claimed}.9999999999.{}", "0".repeat(64));
    let (_forger, forged_cookie) = push(&server.address, Some(&forged));
    assert_ne!(forged_cookie.map(|cookie| session_of(&cookie)), Some(claimed), "a forged cookie names nothing");

    drop(first);
    let (_again, renewed) = push(&server.address, Some(&cookie));
    assert_eq!(renewed.map(|cookie| session_of(&cookie)), Some(session), "the session continues");
    let _ = std::fs::remove_dir_all(&folder);
}

/// The administration listener answers `/healthz`, `/readyz` (200 once
/// the channel's program runs) and `/metrics`, which is not on the
/// viewers' listener (requirements 8.1 and 8.4).
#[test]
fn probes_and_metrics_are_on_the_administration_listener() {
    let folder = scratch("admin");
    let file = channels(&folder, &publishing("Ready"));
    let admin = format!("127.0.0.1:{}", free_port());
    let server = serve(&["--channels", &file.display().to_string(), "--admin-listen", &admin], &[], Stdio::null());
    assert_eq!(until_status(&admin, "/healthz", 200), b"ok\n");
    assert_eq!(get(&server.address, &settings("ch")).map(|(status, _)| status), Some(200));
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
    until_status(&quiet.address, "/", 404);
    assert!(quiet.terminate().success());
    let mut written = String::new();
    quiet.child.stderr.take().unwrap().read_to_string(&mut written).unwrap();
    assert_eq!(written, "", "nothing below WARN");
}
