use super::*;
use crate::logging::tests::{Captured, field};

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("teletext-apps-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A program in `sh` that writes a line to its standard error, declares a
/// page, echoes the first line it is sent back as a log line, publishes
/// once, then runs `then`.
fn program(then: &str) -> Vec<String> {
    let script = format!(
        r#"printf 'warming up\n' >&2
printf '%s\n' '{{"type":"pages","pages":[{{"page":"100","title":"Front"}}]}}'
read line
printf '{{"type":"log","text":"got %s"}}\n' "$(printf '%s' "$line" | tr -d '"{{}}')"
printf '%s\n' '{{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nOL,1,Hello\n"}}'
{then}"#
    );
    vec!["sh".to_owned(), "-c".to_owned(), script]
}

fn spec(folder: &str, program: Program) -> ChannelSpec {
    ChannelSpec { folder: folder.to_owned(), name: "Test".to_owned(), program }
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

fn pages_of(arrived: FromApplication) -> Vec<u16> {
    let FromApplication::Declared(pages) = arrived else { panic!("a declaration, not {arrived:?}") };
    pages.iter().map(|entry| entry.number).collect()
}

/// The program declares its pages, which reach the channel; it hears what
/// the channel sends and is heard; its standard error is logged with its
/// channel; when it ends the channel is told, and when it is started again
/// it declares its pages again (requirements 5.3 and 7.5).
#[tokio::test]
async fn a_program_is_started_heard_and_started_again() {
    let captured = Captured::default();
    let _log = captured.install();
    let mut link = start(spec("test", Program::Command { command: program("exit 0"), directory: scratch("run") })).unwrap();
    assert_eq!(pages_of(next(&mut link).await), [0x100]);
    link.to.send(b"{\"type\":\"start\"}\n".to_vec()).await.unwrap();
    let FromApplication::Line(echo) = next(&mut link).await else { panic!("a line first") };
    assert_eq!(String::from_utf8(echo).unwrap(), r#"{"type":"log","text":"got type:start"}"#);
    let FromApplication::Line(publish) = next(&mut link).await else { panic!("then the page") };
    assert!(publish.starts_with(br#"{"type":"publish""#));
    assert!(matches!(next(&mut link).await, FromApplication::Ended));
    // After the first pause it runs again and declares its pages again.
    assert_eq!(pages_of(next(&mut link).await), [0x100]);
    let events = captured.events();
    assert!(
        events.iter().any(|event| field(event, &["target"]) == Some("program")
            && field(event, &["fields", "message"]) == Some("warming up")
            && field(event, &["fields", "channel"]) == Some("test")),
        "{events:?}"
    );
}

/// A program's first line must declare its pages; anything else, or
/// nothing, is refused.
#[tokio::test]
async fn a_program_must_declare_its_pages_first() {
    let declared = declare(&mut BufReader::new(&br#"{"type":"pages","pages":[{"page":"200","title":"A"}]}"#[..])).await.unwrap();
    assert_eq!(declared.iter().map(|entry| entry.number).collect::<Vec<_>>(), [0x200]);
    let error = declare(&mut BufReader::new(&b"{\"type\":\"log\",\"text\":\"hello\"}\n"[..])).await.unwrap_err();
    assert!(error.contains("the first line must be"), "{error}");
    let error = declare(&mut BufReader::new(&b""[..])).await.unwrap_err();
    assert!(error.contains("ended before declaring"), "{error}");
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
/// declares, so none reaches it before `start` (requirement 5.3).
#[tokio::test]
async fn lines_for_an_ended_program_are_dropped() {
    let (to, mut outgoing) = mpsc::channel(WAITING_LINES);
    let (arrived, mut from) = mpsc::channel(ARRIVING_LINES);
    to.send(b"{\"type\":\"join\",\"session\":1}\n".to_vec()).await.unwrap();
    assert!(started(Vec::new(), &arrived, &mut outgoing).await);
    assert!(matches!(from.recv().await, Some(FromApplication::Declared(_))));
    assert!(outgoing.try_recv().is_err(), "nothing left for the new program");
}

/// A program that ran steadily was ended on purpose, as when its author
/// saves a change: it starts again at once. One that ends soon after
/// starting keeps the back-off, so a failing program cannot spin.
#[test]
fn a_steady_program_starts_again_at_once_and_a_failing_one_waits() {
    assert_eq!(pause_after(STEADY, FIRST_PAUSE), Duration::ZERO);
    assert_eq!(pause_after(STEADY.saturating_sub(Duration::from_millis(1)), FIRST_PAUSE), FIRST_PAUSE);
    assert_eq!(pause_after(Duration::ZERO, LONGEST_PAUSE), LONGEST_PAUSE);
}

/// A program that runs as a service of its own connects to its channel's
/// socket, which only its owner may reach; declares; hears `start`; is
/// heard; and, when it goes and comes back, declares again. A second
/// program while one is connected is closed (requirement 6.2).
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
        writing.write_all(b"{\"type\":\"pages\",\"pages\":[{\"page\":\"100\",\"title\":\"Front\"}]}\n").await.unwrap();
        assert_eq!(pages_of(next(&mut link).await), [0x100], "round {round}");
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
