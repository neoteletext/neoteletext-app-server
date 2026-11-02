use super::*;
use crate::logging::tests::{Captured, field};

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("teletext-apps-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A program in `sh` that writes a line to its standard error, echoes the
/// first line it is sent back as a log line, then runs `then`.
fn program(then: &str) -> Vec<String> {
    let script = format!(
        r#"printf 'warming up\n' >&2
read line
printf '{{"type":"log","text":"got %s"}}\n' "$(printf '%s' "$line" | tr -d '"{{}}')"
{then}"#
    );
    vec!["sh".to_owned(), "-c".to_owned(), script]
}

fn spec(folder: &str, program: Program) -> ChannelSpec {
    ChannelSpec { folder: folder.to_owned(), name: "Test".to_owned(), program }
}

/// DEC-1694 rule 2: a folder named like a place in a channel belongs to the
/// channel above it, so it cannot be one. A page number has its first digit
/// from 1 to 8, in either case, and any other name is a folder.
#[test]
fn a_folder_named_like_a_place_is_not_a_channel() {
    let folder = scratch("reserved");
    let file = folder.join("channels.json");
    let read = |name: &str| {
        std::fs::write(&file, format!(r#"{{"channels":[{{"folder":"{name}","command":["true"]}}]}}"#)).unwrap();
        read_channels(&file)
    };
    for name in ["100", "2fa", "2FA", "8FF", "1BE", "contents", "wall"] {
        assert!(read(name).is_err_and(|error| error.contains("belongs to the channel above it")), "{name}");
    }
    for name in ["000", "900", "1000", "10", "walls", "Wall", "news"] {
        assert_eq!(read(name).unwrap().len(), 1, "{name}");
    }
}

#[test]
fn the_channels_file_names_each_program() {
    let folder = scratch("file");
    let file = folder.join("channels.json");
    std::fs::write(
        &file,
        br#"{"channels":[{"folder":"hackaday","name":"Hackaday","command":["uv","run","main.py"],"directory":"channels/hackaday"},{"folder":"showcase","command":["node","main.ts"]},{"folder":"own","socket":"sockets/own.sock"}]}"#,
    )
    .unwrap();
    let specs = read_channels(&file).unwrap();
    assert_eq!(
        specs,
        [
            ChannelSpec {
                folder: "hackaday".to_owned(),
                name: "Hackaday".to_owned(),
                program: Program::Command {
                    command: vec!["uv".to_owned(), "run".to_owned(), "main.py".to_owned()],
                    directory: folder.join("channels/hackaday")
                },
            },
            ChannelSpec {
                folder: "showcase".to_owned(),
                name: "showcase".to_owned(),
                program: Program::Command { command: vec!["node".to_owned(), "main.ts".to_owned()], directory: folder.join("") },
            },
            ChannelSpec { folder: "own".to_owned(), name: "own".to_owned(), program: Program::Socket(folder.join("sockets/own.sock")) },
        ]
    );
    for bad in [
        &br#"{"channels":[{"folder":"../up","command":["x"]}]}"#[..],
        br#"{"channels":[{"folder":"a","command":[]}]}"#,
        br#"{"channels":[{"folder":"a","command":["x"],"socket":"a.sock"}]}"#,
        br#"{"channels":[{"folder":"a","command":["x"]},{"folder":"a","command":["y"]}]}"#,
        br#"{"channel":[]}"#,
    ] {
        std::fs::write(&file, bad).unwrap();
        assert!(read_channels(&file).is_err(), "{}", String::from_utf8_lossy(bad));
    }
}

async fn next(link: &mut Link) -> FromApplication {
    tokio::time::timeout(Duration::from_secs(10), link.from.recv()).await.unwrap().unwrap()
}

/// The program's start reaches the channel; it hears what the channel
/// sends and is heard; its standard error is logged with its channel; when
/// it ends the channel is told, and it is started again (requirement 7.5).
/// REQ-1042 and REQ-1034.
#[tokio::test]
async fn a_program_is_started_heard_and_started_again() {
    let captured = Captured::default();
    let _log = captured.install();
    let mut link = start(spec("test", Program::Command { command: program("exit 0"), directory: scratch("run") })).unwrap();
    assert!(matches!(next(&mut link).await, FromApplication::Started));
    link.to.send(b"{\"type\":\"start\"}\n".to_vec()).await.unwrap();
    let FromApplication::Line(echo) = next(&mut link).await else { panic!("a line first") };
    assert_eq!(String::from_utf8(echo).unwrap(), r#"{"type":"log","text":"got type:start"}"#);
    assert!(matches!(next(&mut link).await, FromApplication::Ended));
    // After the first pause it runs again.
    assert!(matches!(next(&mut link).await, FromApplication::Started));
    let events = captured.events();
    assert!(
        events.iter().any(|event| field(event, &["target"]) == Some("program")
            && field(event, &["fields", "message"]) == Some("warming up")
            && field(event, &["fields", "channel"]) == Some("test")),
        "{events:?}"
    );
}

/// A program that cannot be started says why.
#[tokio::test]
async fn a_program_that_cannot_start_says_why() {
    let Err(error) = launch("test", &["/no/such/program".to_owned()], &scratch("refuse")) else { panic!("it cannot start") };
    assert!(error.contains("cannot start"), "{error}");
}

/// A line of standard error longer than the limit is cut there, and the
/// line after it is read whole.
#[tokio::test]
async fn a_long_line_of_standard_error_is_cut() {
    let mut text = vec![b'x'; LONGEST_ERROR_LINE * 2 + 10];
    text.extend_from_slice(b"\nnext\r\n");
    let mut reader = BufReader::new(text.as_slice());
    let (line, cut) = read_line_cut(&mut reader, LONGEST_ERROR_LINE).await.unwrap().unwrap();
    assert_eq!((line.len(), cut), (LONGEST_ERROR_LINE, true));
    assert_eq!(read_line_cut(&mut reader, LONGEST_ERROR_LINE).await.unwrap(), Some((b"next".to_vec(), false)));
    assert_eq!(read_line_cut(&mut reader, LONGEST_ERROR_LINE).await.unwrap(), None);
}

/// Lines queued for a program that ended are dropped when the next one
/// starts, so none reaches it before `start`.
#[tokio::test]
async fn lines_for_an_ended_program_are_dropped() {
    let (to, mut outgoing) = mpsc::channel(WAITING_LINES);
    let (arrived, mut from) = mpsc::channel(ARRIVING_LINES);
    to.send(b"{\"type\":\"join\",\"session\":1}\n".to_vec()).await.unwrap();
    assert!(started(&arrived, &mut outgoing).await);
    assert!(matches!(from.recv().await, Some(FromApplication::Started)));
    assert!(outgoing.try_recv().is_err(), "nothing left for the new program");
}

/// A program that ran steadily was ended on purpose, as when its author
/// saves a change: it starts again at once. One that ends soon after
/// starting keeps the back-off, so a failing program cannot spin.
/// REQ-1034: a steady program at once, a failing one after its pause.
#[test]
fn a_steady_program_starts_again_at_once_and_a_failing_one_waits() {
    assert_eq!(pause_after(STEADY, FIRST_PAUSE), Duration::ZERO);
    assert_eq!(pause_after(STEADY.saturating_sub(Duration::from_millis(1)), FIRST_PAUSE), FIRST_PAUSE);
    assert_eq!(pause_after(Duration::ZERO, LONGEST_PAUSE), LONGEST_PAUSE);
}

/// A program that runs as a service of its own connects to its channel's
/// socket, which only its owner may reach; is told to the channel; hears
/// `start`; is heard; and, when it goes and comes back, is told again. A second
/// program while one is connected is closed (requirement 6.2).
/// REQ-1043.
#[cfg(unix)]
#[tokio::test]
async fn a_program_connects_to_its_socket() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::UnixStream;

    // A socket's path is limited to about 100 bytes: the system's own
    // temporary folder keeps it short.
    let folder = std::env::temp_dir().join(format!("teletext-socket-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let path = folder.join("channel.sock");
    let mut link = start(spec("own", Program::Socket(path.clone()))).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, OWNER_ONLY_SOCKET);
    assert_eq!(std::fs::metadata(&folder).unwrap().permissions().mode() & 0o777, OWNER_ONLY_FOLDER);
    for round in 0..2 {
        let stream = UnixStream::connect(&path).await.unwrap();
        let (reading, mut writing) = stream.into_split();
        let mut reading = BufReader::new(reading);
        assert!(matches!(next(&mut link).await, FromApplication::Started), "round {round}");
        link.to.send(b"{\"type\":\"start\"}\n".to_vec()).await.unwrap();
        let mut heard = String::new();
        reading.read_line(&mut heard).await.unwrap();
        assert_eq!(heard, "{\"type\":\"start\"}\n");
        writing.write_all(b"{\"type\":\"log\",\"text\":\"hello\"}\n").await.unwrap();
        let FromApplication::Line(received) = next(&mut link).await else { panic!("a line") };
        assert_eq!(received, br#"{"type":"log","text":"hello"}"#);
        let mut second = UnixStream::connect(&path).await.unwrap();
        let mut nothing = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(10), second.read_to_end(&mut nothing)).await.unwrap();
        assert_eq!((read.unwrap(), nothing.len()), (0, 0), "the second is closed");
        drop((reading, writing));
        assert!(matches!(next(&mut link).await, FromApplication::Ended));
    }
    let _ = std::fs::remove_dir_all(&folder);
}
