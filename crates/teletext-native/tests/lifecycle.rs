//! The server binary through its life: its log at start, its probes and
//! metrics, sessions that continue across connections, a clean stop, and
//! a restart that serves what it kept, and systemd's socket activation
//! (requirements 5.1 to 5.6, 7.1, 7.2, 8.1 and 8.4). Unix only: the stop is
//! a `SIGTERM`.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use neoteletext_core::consts::{LINK_SCHEME, PACKET_BYTES};
use neoteletext_core::packet::Packet;
use neoteletext_input::caps::SELECT_CAP;
use neoteletext_input::event::{Event, Item, Select, write as write_item};
use neoteletext_web::channel::SETTINGS_FILE;
use neoteletext_web::consts::{CLOSE_RESTARTING, PUSH_SUBPROTOCOL};
use neoteletext_web::records::{ChannelRecord, Read as RecordRead, RecordReader};
use teletext_runtime::json::{self, Value};

/// How long the server and its program have to start, or to stop.
const WITHIN: Duration = Duration::from_secs(30);
/// Between tries.
const RETRY: Duration = Duration::from_millis(50);
/// Longer than the server's second between recording a channel's figures
/// (`RECORD_EVERY_S`).
const FIGURES_RECORDED: Duration = Duration::from_millis(1_300);
/// The WebSocket key of RFC 6455 section 1.3's example.
const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
/// A final binary frame's first byte (RFC 6455 section 5.2).
const BINARY_FINAL: u8 = 0x82;
/// The mask bit with the length 126, which says two length bytes follow.
const MASKED_LONG: u8 = 0xFE;
/// A Close frame's opcode (RFC 6455 section 5.2).
const CLOSE: u8 = 0x8;
/// Lets the kernel choose the port.
const ANY_LOCAL_PORT: &str = "127.0.0.1:0";
/// What stands where systemd does (`sd_listen_fds(3)`): standard input is
/// the one place safe Rust can put a descriptor in a child, so the shell
/// moves it to descriptor 3, the first systemd passes, and names itself in
/// `LISTEN_PID`, the process `exec` then keeps.
const AS_SYSTEMD: &str = r#"count=$1; shift; exec 3<&0 0</dev/null; export LISTEN_FDS=$count LISTEN_PID=$$; exec "$@""#;

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
        self.exited()
    }

    /// Waits for the server to exit.
    fn exited(&mut self) -> ExitStatus {
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

/// Starts the server with `arguments`, on a listening socket this test made.
///
/// A port found by binding and closing, for the server to bind again, is not
/// the server's. On macOS a socket is made close-on-exec only after it is made,
/// so a sibling test starting a process in that moment hands it the listener
/// for as long as it runs: the port then stays listening with nothing
/// accepting, and the server cannot bind it (`Address already in use`, in 11
/// of 11 failures seen on 1 November 2026). Passing the socket leaves no moment
/// when the port is nobody's, and a copy in another process takes nothing from
/// this server's queue.
fn serve(arguments: &[&str], environment: &[(&str, &str)], stderr: Stdio) -> Server {
    activated(&TcpListener::bind(ANY_LOCAL_PORT).unwrap(), 1, arguments, environment, stderr)
}

/// Starts the server as systemd starts a socket-activated service, on
/// `socket` with `count` descriptors announced. `--listen` names `socket`'s
/// own address, which stays bound, so a server that bound `--listen` as well
/// as or instead of taking the socket could not start.
fn activated(socket: &TcpListener, count: usize, arguments: &[&str], environment: &[(&str, &str)], stderr: Stdio) -> Server {
    let address = socket.local_addr().unwrap().to_string();
    let descriptor = OwnedFd::from(socket.try_clone().unwrap());
    let mut command = Command::new("sh");
    command.args(["-c", AS_SYSTEMD, "sh", &count.to_string(), env!("CARGO_BIN_EXE_teletext-server")]);
    command.args(["--listen", &address, "--origin", &format!("http://{address}")]).args(arguments);
    command.stdin(Stdio::from(descriptor)).stdout(Stdio::null()).stderr(stderr);
    command.env_remove("RUST_LOG").env_remove("STATE_DIRECTORY");
    command.envs(environment.iter().copied());
    Server { child: command.spawn().unwrap(), address }
}

/// The server's `serving` line, which its start writes once it listens. A
/// program's start may be logged first.
fn serving_line(stderr: &mut BufReader<ChildStderr>) -> Value {
    loop {
        let mut line = String::new();
        assert_ne!(stderr.read_line(&mut line).unwrap(), 0, "the server ended without serving");
        let event = json::parse(line.trim_end().as_bytes()).unwrap();
        if event.get("fields").and_then(|fields| fields.get("message")).and_then(Value::text) == Some("serving") {
            return event;
        }
    }
}

/// Connects and asks for `path`, whether or not anything is accepting: a
/// listening socket's queue holds the connection until something does.
fn ask(address: &str, path: &str) -> TcpStream {
    let mut stream = TcpStream::connect(address).unwrap();
    stream.set_read_timeout(Some(WITHIN)).unwrap();
    write!(stream, "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").unwrap();
    stream
}

/// The whole response to what [`ask`] asked.
fn response_to(mut stream: TcpStream) -> String {
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("no answer on the socket that was passed");
    response
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

/// An upgrade of `target` with these header lines added, but for its end.
fn upgrade_request(address: &str, target: &str, headers: &str) -> String {
    format!(
        "GET {target} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Version: 13\r\n{headers}"
    )
}

/// A response's head, read a byte at a time so nothing after it is taken.
fn response_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    String::from_utf8(head).unwrap()
}

/// The server's answer to an upgrade with these header lines added.
fn upgrade_answer(address: &str, headers: &str) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    write!(stream, "{}\r\n", upgrade_request(address, "/ch/push", headers)).unwrap();
    response_head(&mut stream)
}

/// A push connection, open, and its response's `Set-Cookie`, if any.
fn push(address: &str, cookie: Option<&str>) -> (TcpStream, Option<String>) {
    push_to(address, "/ch/push", cookie)
}

/// A push connection to `target`, open, and its response's `Set-Cookie`.
fn push_to(address: &str, target: &str, cookie: Option<&str>) -> (TcpStream, Option<String>) {
    let mut stream = TcpStream::connect(address).unwrap();
    let cookie = cookie.map(|cookie| format!("Cookie: {cookie}\r\n")).unwrap_or_default();
    write!(stream, "{}{cookie}\r\n", upgrade_request(address, target, &format!("Sec-WebSocket-Protocol: {PUSH_SUBPROTOCOL}\r\n"))).unwrap();
    let head = response_head(&mut stream);
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    assert_eq!(header(&head, "sec-websocket-protocol"), Some(PUSH_SUBPROTOCOL), "{head}");
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

/// A stopped server closes its viewers' connections with the code that says try again and exits
/// with 0 (requirement 5.1).
/// REQ-1036: the close code that says try again, and status 0.
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
    assert_eq!(close_code(&mut viewer), CLOSE_RESTARTING, "try again");
    let _ = std::fs::remove_dir_all(&folder);
}

/// REQ-1041: with the listening socket passed as systemd passes it, a
/// connection made before the server runs, and one made while it restarts,
/// wait in the socket's queue and are answered; `--listen` is left alone.
/// The socket held here is the copy systemd keeps across the restart.
#[test]
fn connections_made_during_a_restart_wait_on_the_passed_socket() {
    let folder = scratch("activation");
    let file = channels(&folder, &publishing("Hello")).display().to_string();
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap().to_string();

    let before_the_server = ask(&address, &settings("ch"));
    let mut first = activated(&socket, 1, &["--channels", &file], &[], Stdio::null());
    let response = response_to(before_the_server);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");

    let (mut viewer, _) = push(&address, None);
    assert!(first.terminate().success());
    assert_eq!(close_code(&mut viewer), CLOSE_RESTARTING, "try again");

    let during_the_restart = ask(&address, &settings("ch"));
    let _second = activated(&socket, 1, &["--channels", &file], &[], Stdio::null());
    let response = response_to(during_the_restart);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let _ = std::fs::remove_dir_all(&folder);
}

/// REQ-1041: a channel's program never holds the passed socket, which would
/// keep the address open, and unanswered, after the server has gone.
#[test]
fn a_program_does_not_inherit_the_passed_socket() {
    let folder = scratch("inherit");
    let told = folder.join("descriptor");
    let script = format!(r"if (: <&3) 2>/dev/null; then echo open; else echo closed; fi > {told:?}; {}", publishing("Hello"));
    let file = channels(&folder, &script).display().to_string();
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let _server = activated(&socket, 1, &["--channels", &file], &[], Stdio::null());
    let started = Instant::now();
    let descriptor = loop {
        let descriptor = std::fs::read_to_string(&told).unwrap_or_default();
        if descriptor.ends_with('\n') {
            break descriptor;
        }
        assert!(started.elapsed() < WITHIN, "the program did not start");
        std::thread::sleep(RETRY);
    };
    assert_eq!(descriptor, "closed\n");
    let _ = std::fs::remove_dir_all(&folder);
}

/// REQ-1041: the server takes one socket. A second would go unserved, and,
/// never marked close-on-exec, on into every program it starts.
#[test]
fn a_second_passed_socket_is_refused() {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut server = activated(&socket, 2, &[], &[], Stdio::piped());
    assert!(!server.exited().success());
    let mut written = String::new();
    server.child.stderr.take().unwrap().read_to_string(&mut written).unwrap();
    assert!(written.contains("2 sockets"), "{written}");
}

/// REQ-1041: the deploy script installs the socket unit and starts it, the
/// service requires it, and the two name one address.
#[test]
fn the_socket_unit_is_installed_and_names_the_services_address() {
    let unit = include_str!("../../../deploy/ionos/neoteletext.socket");
    let deploy = include_str!("../../../deploy/ionos/deploy.sh");
    let address = unit.lines().find_map(|line| line.strip_prefix("ListenStream=")).unwrap();
    assert!(deploy.contains(&format!("--listen {address} ")), "the service listens on another address than {address}");
    assert!(deploy.contains(r#"/etc/systemd/system/neoteletext.socket >/dev/null" < "$here/neoteletext.socket""#));
    assert!(deploy.contains("Requires=neoteletext.socket"));
    assert!(deploy.contains("enable --now neoteletext.socket"));
}

/// REQ-1040, TODO-1301: a channel keeps as many sessions waiting for their
/// viewer as the service may hold open files, so that every viewer it can
/// hold connected can come back after a restart; the bound is derived from
/// the unit's limit, and the two must agree.
#[test]
fn the_waiting_sessions_bound_is_the_units_open_files() {
    let deploy = include_str!("../../../deploy/ionos/deploy.sh");
    let limit = deploy.lines().find_map(|line| line.strip_prefix("LimitNOFILE=")).unwrap();
    assert_eq!(limit.parse::<usize>().unwrap(), teletext_runtime::channel::MOST_WAITING_SESSIONS);
}

/// DEC-1759 lesson 18: an upgrade that carries `Origin` is a web page's
/// script, and is refused, whatever it offers. Lesson 14: an upgrade that does
/// not offer this version's subprotocol is refused and told what is spoken.
#[test]
fn only_a_receiver_of_this_version_is_upgraded() {
    let folder = scratch("upgrade");
    let file = channels(&folder, &publishing("Refusing"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("ch"), 200);
    let named = format!("Sec-WebSocket-Protocol: {PUSH_SUBPROTOCOL}\r\n");
    let origin = format!("{named}Origin: https://elsewhere.example\r\n");
    assert!(upgrade_answer(&server.address, &origin).starts_with("HTTP/1.1 403"));
    let null = format!("{named}Origin: null\r\n");
    assert!(upgrade_answer(&server.address, &null).starts_with("HTTP/1.1 403"), "a sandboxed page's");
    for offered in ["", "Sec-WebSocket-Protocol: neottx.6\r\n", "Sec-WebSocket-Protocol: chat\r\n"] {
        let answer = upgrade_answer(&server.address, offered);
        assert!(answer.starts_with("HTTP/1.1 426"), "{offered:?}: {answer}");
        assert_eq!(header(&answer, "sec-websocket-protocol"), Some(PUSH_SUBPROTOCOL), "{answer}");
    }
    let several = format!("Sec-WebSocket-Protocol: chat, {PUSH_SUBPROTOCOL}\r\n");
    let answer = upgrade_answer(&server.address, &several);
    assert!(answer.starts_with("HTTP/1.1 101"), "{answer}");
    assert_eq!(header(&answer, "sec-websocket-protocol"), Some(PUSH_SUBPROTOCOL), "{answer}");
    let _ = std::fs::remove_dir_all(&folder);
}

/// REQ-1339: an upgrade offering permessage-deflate is answered with no
/// extension: frames go uncompressed whatever a receiver offers.
#[test]
fn permessage_deflate_is_never_agreed() {
    let folder = scratch("deflate");
    let file = channels(&folder, &publishing("Plain"));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("ch"), 200);
    let offered = format!("Sec-WebSocket-Protocol: {PUSH_SUBPROTOCOL}\r\nSec-WebSocket-Extensions: permessage-deflate; client_max_window_bits\r\n");
    let answer = upgrade_answer(&server.address, &offered);
    assert!(answer.starts_with("HTTP/1.1 101"), "{answer}");
    assert_eq!(header(&answer, "sec-websocket-extensions"), None, "{answer}");
    let _ = std::fs::remove_dir_all(&folder);
}

/// One server frame's payload, skipping nothing.
fn server_frame(stream: &mut TcpStream) -> Vec<u8> {
    stream.set_read_timeout(Some(WITHIN)).unwrap();
    let mut head = [0_u8; 2];
    stream.read_exact(&mut head).unwrap();
    let length = match head[1] & 0x7F {
        126 => {
            let mut bytes = [0_u8; 2];
            stream.read_exact(&mut bytes).unwrap();
            usize::from(u16::from_be_bytes(bytes))
        }
        short => usize::from(short),
    };
    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload).unwrap();
    payload
}

/// The `Set-Cookie` value of the next `SET_COOKIE` record on `stream`.
fn set_cookie_record(stream: &mut TcpStream) -> String {
    let mut reader = RecordReader::new();
    loop {
        let message = server_frame(stream);
        for packet in message.as_chunks::<PACKET_BYTES>().0 {
            if let RecordRead::Record(ChannelRecord::SetCookie(line)) = reader.push(&Packet { bytes: *packet }) {
                return String::from_utf8(line.to_vec()).unwrap();
            }
        }
    }
}

/// REQ-1471 item 9: a program sets a cookie on one connection, and the same
/// viewer's next connection's `connected` carries it, and not the session
/// cookie: through a real socket, the cookie read back from the `SET_COOKIE`
/// record as a receiver's jar would.
#[test]
fn a_cookie_a_program_sets_reaches_it_on_the_next_connection() {
    let folder = scratch("cookies");
    let heard = folder.join("heard");
    let cookie =
        r#"{"type":"cookie","connection":1,"name":"theme","value":"dark","path":null,"max_age":null,"same_site":null,"secure":false,"http_only":false}"#;
    // The cookie goes out once the first connection is told of, whatever
    // else the server sent first (its `describe`, DEC-2007, comes in no
    // fixed place before it).
    let script = format!(
        "read -r _; set=; while read -r line; do printf '%s\\n' \"$line\" >> {heard:?}; case \"$line\" in *'\"connected\"'*) [ -n \"$set\" ] || {{ printf '%s\\n' '{cookie}'; set=1; }};; esac; done"
    );
    let file = channels(&folder, &script);
    let state = folder.join("state").display().to_string();
    let server = serve(&["--channels", &file.display().to_string(), "--state", &state], &[], Stdio::null());
    until_status(&server.address, &settings("ch"), 200);
    let hello = Item::Event(Event::Hello { version: neoteletext_input::VERSION, caps: 0, agent: None });
    let (mut viewer, session) = push(&server.address, None);
    send_input(&mut viewer, &[hello]);
    let set = set_cookie_record(&mut viewer);
    assert!(set.starts_with("theme=dark"), "{set}");
    let session = session.unwrap();
    let session = session.split(';').next().unwrap();
    let (mut again, _) = push(&server.address, Some(&format!("{session}; theme=dark")));
    send_input(&mut again, &[hello]);
    let told = second_connected_line(&heard);
    let line = json::parse(told.as_bytes()).unwrap();
    let cookies = line.get("cookies").unwrap().items().unwrap();
    assert_eq!(cookies.len(), 1, "{told}");
    assert_eq!(cookies[0].get("name").and_then(Value::text), Some("theme"), "{told}");
    assert_eq!(cookies[0].get("value").and_then(Value::text), Some("dark"), "{told}");
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

/// What the program has written to `heard`, once `wanted` holds of it.
fn until_heard(heard: &Path, wanted: impl Fn(&str) -> bool) -> String {
    let started = Instant::now();
    loop {
        let told = std::fs::read_to_string(heard).unwrap_or_default();
        if wanted(&told) {
            return told;
        }
        assert!(started.elapsed() < WITHIN, "the program heard: {told:.300}");
        std::thread::sleep(RETRY);
    }
}

/// The `connected` line of the second connection the program heard of.
fn second_connected_line(heard: &Path) -> String {
    let told = until_heard(heard, |told| told.matches("\"connected\"").count() >= 2);
    told.lines().filter(|line| line.contains("\"connected\"")).nth(1).unwrap().to_owned()
}

/// The first `connected` line the program has written to `heard`.
fn connected_line(heard: &Path) -> String {
    let told = until_heard(heard, |told| told.contains("\"connected\""));
    told.lines().find(|line| line.contains("\"connected\"")).unwrap().to_owned()
}

/// REQ-1693 item 6: page parameters of the longest length reach the program
/// through a real socket, and the next selection leaves them behind.
/// REQ-1674: the query reaches the program, at the longest length.
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
    // The first selection goes in `connected` (TODO-1731 item 1). The program
    // may not have started yet, and selections made before it hears of the
    // connection are told as one `connected` with the latest (REQ-1471), so
    // the next ones wait for it.
    send_input(&mut viewer, &[hello, select(0x301, "")]);
    let connected = json::parse(connected_line(&heard).as_bytes()).unwrap();
    assert_eq!(connected.get("selection").and_then(|selection| selection.get("page")).and_then(Value::text), Some("301"));
    send_input(&mut viewer, &[select(0x302, &parameters)]);
    send_input(&mut viewer, &[select(0x303, "")]);
    let told = until_heard(&heard, |told| told.matches("\"selected\"").count() == 2);
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

/// A program that answers each `describe` by the shell's `case` on the page: 100
/// public, 200 not public, and anything else never, logging what it hears.
fn describing(heard: &Path, content: &str) -> String {
    let public = format!(r#"{{"type":"description","page":"100","public":true,"title":"<b>Title</b>","text":"The </script> words","content":{content}}}"#);
    let not_public = r#"{"type":"description","page":"200","public":false}"#;
    format!(
        r#"read -r _; while read -r line; do printf '%s\n' "$line" >> {heard}; case "$line" in *'"describe"'*'"100"'*) printf '%s\n' '{public}';; *'"describe"'*'"200"'*) printf '%s\n' '{not_public}';; esac; done"#,
        heard = heard.display()
    )
}

/// One sub-page of page 100, blank but for its row of `A`s.
fn blank_subpage() -> String {
    let row = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQQ==";
    let rows = format!("[null,\"{row}\",{}null]", "null,".repeat(22));
    format!(
        r#"{{"page":"100","sub":"0000","rows":{rows},"links":[null,null,null,null,null,null],"display_row_24":false,"link_extension":{{"keys":[],"numbers":[],"texts":[]}},"control":{{"erase":false,"newsflash":false,"subtitle":false,"suppress_header":false,"update":false,"interrupted_sequence":false,"inhibit_display":false,"magazine_serial":false}},"character_set":{{"region":null,"national_option":0}},"enhancements":[],"packets":[],"cursor":null}}"#
    )
}

/// REQ-1728 over HTTP: a server whose program answers, once it has started:
/// until then a page's address is not public and nothing is kept.
fn described_server(name: &str) -> (Server, PathBuf) {
    let folder = scratch(name);
    let heard = folder.join("heard");
    let file = channel_in(&folder, "home", &describing(&heard, &blank_subpage()));
    let server = serve(&["--channels", &file.display().to_string()], &[], Stdio::null());
    until_status(&server.address, &settings("home"), 200);
    let started = Instant::now();
    while !fetched(&server.address, "/home/100").1.windows(7).any(|window| window == b"paywall") {
        assert!(started.elapsed() < WITHIN, "the program did not start");
        std::thread::sleep(RETRY);
    }
    (server, heard)
}

/// REQ-1728 verification 1, with the answer's own head: not public, a page the
/// program never answers for, and a page that cannot exist are one answer.
#[test]
fn every_page_that_is_not_public_answers_alike() {
    let (server, _) = described_server("not-public");
    let (head, not_public) = fetched(&server.address, "/home/200");
    let page = String::from_utf8(not_public.clone()).unwrap();
    assert!(page.contains("noindex"), "{page}");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(header(&head, "content-security-policy"), Some(teletext_runtime::landing::CONTENT_SECURITY_POLICY));
    let same = |target: &str, number: &str| {
        let (other_head, other) = fetched(&server.address, target);
        let spelled = String::from_utf8(other).unwrap().replace(number, "200");
        assert_eq!(spelled, page, "{target}");
        assert_eq!(header(&other_head, "content-security-policy"), header(&head, "content-security-policy"));
        assert_eq!(header(&other_head, "cache-control"), header(&head, "cache-control"));
    };
    same("/home/300", "300");
    same("/home/1FF", "1FF");
}

/// REQ-1728 verification 2 and 3 over HTTP, and verification 6: the same bytes
/// whatever `User-Agent` a request carries.
#[test]
fn a_public_page_is_the_same_for_everyone() {
    let (server, _) = described_server("public");
    let request = |agent: &str| {
        let mut stream = TcpStream::connect(&server.address).unwrap();
        write!(stream, "GET /home/100 HTTP/1.1\r\nHost: {}\r\nUser-Agent: {agent}\r\nConnection: close\r\n\r\n", server.address).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    };
    let person = request("Mozilla/5.0 (Macintosh)");
    let crawler = request("Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)");
    let without_date = |response: &[u8]| String::from_utf8_lossy(response).lines().filter(|line| !line.starts_with("date:")).collect::<Vec<_>>().join("\n");
    assert_eq!(without_date(&person), without_date(&crawler));
    let page = String::from_utf8_lossy(&person).into_owned();
    assert!(page.contains("<h1>&lt;b&gt;Title&lt;/b&gt;</h1>"), "{page:.2000}");
    assert!(page.contains("<img src=\"data:image/png;base64,iVBOR"));
    assert!(page.contains("<div class=\"paywall\">The &lt;/script&gt; words</div>"));
    assert!(page.contains(r#""isAccessibleForFree":false"#) && page.contains(r#""cssSelector":".paywall""#));
    assert!(!page.contains("noindex"));
    let policy = page.lines().find_map(|line| line.strip_prefix("content-security-policy: ")).unwrap();
    assert_eq!(policy.trim_end(), teletext_runtime::landing::PUBLIC_CONTENT_SECURITY_POLICY);
}

/// REQ-1728 verification 5 over HTTP: the program hears one `describe`, and a
/// query is not sent and does not divide the cache.
#[test]
fn repeated_visits_reach_the_program_once() {
    let (server, heard) = described_server("once");
    // `described_server` has asked already, so none of these is the first.
    for target in ["/home/100", "/home/100?article=5", "/home/100/0002", "/home/100?article=6"] {
        let (_, body) = fetched(&server.address, target);
        assert!(String::from_utf8_lossy(&body).contains("paywall"), "{target}");
    }
    let told = std::fs::read_to_string(&heard).unwrap();
    assert_eq!(told.lines().filter(|line| line.contains("\"describe\"")).collect::<Vec<_>>(), [r#"{"type":"describe","page":"100"}"#]);
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
/// REQ-1040: the cookie, its attributes, and a forged one ignored.
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

/// REQ-1052, REQ-1054: the administration listener answers `/healthz`,
/// `/readyz` (200 once the channel's program has answered), `/metrics` and
/// `/stats.json`, and the last two are not on the viewers' listener
/// (requirements 8.1 and 8.4).
#[test]
fn probes_and_metrics_are_on_the_administration_listener() {
    let folder = scratch("admin");
    let answering = r#"read -r _; read -r _; printf '%s\n' '{"type":"description","page":"100","public":false}'; while read -r _; do :; done"#;
    let file = channels(&folder, answering);
    let mut server = serve(&["--channels", &file.display().to_string(), "--admin-listen", ANY_LOCAL_PORT], &[], Stdio::piped());
    let mut stderr = BufReader::new(server.child.stderr.take().unwrap());
    let serving = serving_line(&mut stderr);
    let admin = serving.get("fields").and_then(|fields| fields.get("admin")).and_then(Value::text).unwrap();
    assert_eq!(until_status(admin, "/healthz", 200), b"ok\n");
    assert_eq!(get(&server.address, &settings("ch")).map(|(status, _)| status), Some(200));
    assert_eq!(until_status(admin, "/readyz", 200), b"ready\n");
    let metrics = String::from_utf8(until_status(admin, "/metrics", 200)).unwrap();
    assert!(metrics.contains("# TYPE teletext_connections gauge\n"), "{metrics}");
    assert!(metrics.contains("teletext_channel_ready{channel=\"ch\"} 1\n"), "{metrics}");
    assert_eq!(get(&server.address, "/metrics").map(|(status, _)| status), Some(404), "not where viewers reach");
    let statistics = String::from_utf8(until_status(admin, "/stats.json", 200)).unwrap();
    assert!(statistics.contains("\"all_time\""), "{statistics}");
    assert_eq!(get(&server.address, "/stats.json").map(|(status, _)| status), Some(404), "not where viewers reach");
    let _ = std::fs::remove_dir_all(&folder);
}

/// `/readyz` says a channel can answer, not that its program started: a
/// program still loading reads nothing, as the demo site does for half a
/// minute, and a viewer in that time is told DENIED 503 (requirement 8.4).
#[test]
fn a_program_loading_is_not_ready() {
    let folder = scratch("loading");
    let loading = folder.join("loading");
    let loaded = folder.join("loaded");
    let script = format!(
        r#"read -r _; : > {loading:?}; until [ -e {loaded:?} ]; do sleep 0.02; done; read -r _; printf '%s\n' '{{"type":"description","page":"100","public":false}}'; while read -r _; do :; done"#
    );
    let file = channels(&folder, &script);
    let mut server = serve(&["--channels", &file.display().to_string(), "--admin-listen", ANY_LOCAL_PORT], &[], Stdio::piped());
    let mut stderr = BufReader::new(server.child.stderr.take().unwrap());
    let serving = serving_line(&mut stderr);
    let admin = serving.get("fields").and_then(|fields| fields.get("admin")).and_then(Value::text).unwrap();
    let started = Instant::now();
    while !loading.exists() {
        assert!(started.elapsed() < WITHIN, "the program did not start");
        std::thread::sleep(RETRY);
    }
    // The figures /readyz reads are recorded each second.
    std::thread::sleep(FIGURES_RECORDED);
    assert_eq!(get(admin, "/readyz"), Some((503, b"no program answering: ch\n".to_vec())));
    assert!(String::from_utf8(until_status(admin, "/metrics", 200)).unwrap().contains("teletext_channel_ready{channel=\"ch\"} 0\n"));
    std::fs::write(&loaded, b"").unwrap();
    assert_eq!(until_status(admin, "/readyz", 200), b"ready\n");
    let _ = std::fs::remove_dir_all(&folder);
}

/// With standard error a pipe, the server writes one JSON object a line,
/// and its start is an `INFO` event; `RUST_LOG=warn` leaves it out
/// (requirements 7.1 and 7.2). With no socket passed it binds `--listen`,
/// and names what it bound.
#[test]
fn the_log_is_json_lines_and_rust_log_chooses_the_level() {
    let child = Command::new(env!("CARGO_BIN_EXE_teletext-server"))
        .args(["--listen", ANY_LOCAL_PORT])
        .env_remove("RUST_LOG")
        .env_remove("STATE_DIRECTORY")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut server = Server { child, address: String::new() };
    let mut stderr = BufReader::new(server.child.stderr.take().unwrap());
    let mut line = String::new();
    stderr.read_line(&mut line).unwrap();
    let event = json::parse(line.trim_end().as_bytes()).unwrap();
    assert_eq!(event.get("level").and_then(Value::text), Some("INFO"), "{line}");
    let fields = event.get("fields").unwrap();
    assert_eq!(fields.get("message").and_then(Value::text), Some("serving"), "{line}");
    let address = fields.get("listen").and_then(Value::text).unwrap();
    assert!(!address.ends_with(":0"), "the port it was given, not the one it has: {line}");
    until_status(address, "/", 404);
    assert!(event.get("timestamp").and_then(Value::text).is_some(), "{line}");
    assert!(server.terminate().success());

    let mut quiet = serve(&[], &[("RUST_LOG", "warn")], Stdio::piped());
    until_status(&quiet.address, "/", 404);
    assert!(quiet.terminate().success());
    let mut written = String::new();
    quiet.child.stderr.take().unwrap().read_to_string(&mut written).unwrap();
    assert_eq!(written, "", "nothing below WARN");
}
