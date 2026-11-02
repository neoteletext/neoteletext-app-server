//! A flood of pointer moves at a program that reads slowly, through the whole
//! of the server but its sockets to viewers (REQ-1360, DOC-1945, DOC-1926).
//! Unix only: the program is a socket's.
#![cfg(unix)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use neoteletext_input::VERSION;
use neoteletext_input::caps::{CURSOR_CAP, SELECT_CAP};
use neoteletext_input::event::{Action, Device, Event, Item, Key, KeyCode, MAX_MOVE_BYTES, Mouse, MouseAction, Point, Select, Stamp, write};
use neoteletext_input::{MAX_MESSAGE, MAX_PASTE, MOVES_PER_SECOND};
use teletext_native::apps::{self, ChannelSpec, Program};
use teletext_native::hub::{self, Command, Hub};
use teletext_native::stats::ServerStats;
use teletext_runtime::json::{self, Value};
use teletext_runtime::{Channel, ChannelConfig, ConnectionId, InitialPage};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::Instant;

const CONNECTION: ConnectionId = 1;
/// A page's columns, which the moves go round.
const COLUMNS: u32 = 40;
/// The row only the flood's last move is on.
const LATEST_ROW: u32 = 24;
/// The sub-page selected, which every event is stamped with.
const ON: Stamp = Stamp { page: 0x100, sub: 1 };
/// Moves in the flood: beyond the 4,096 lines that may wait for a program.
const FLOOD: u32 = 10_000;
/// What a slow program is given of a flood at the rate: the first, one a
/// period, and the one a key closes.
const WITHIN_THE_PACE: u128 = 3;
/// Pastes of the longest kind, which fill what the kernel holds for the
/// program on any system, so that everything after them waits in the server.
const FILLING: usize = 128;
/// How long the test waits for a line it expects.
const WITHIN: Duration = Duration::from_secs(10);

fn bytes_of(item: &Item<'_>) -> Vec<u8> {
    let mut bytes = vec![0; MAX_MESSAGE];
    let length = write(item, &mut bytes).unwrap();
    bytes.truncate(length);
    bytes
}

/// Each move a place of its own among its neighbours, on the rows above
/// [`LATEST_ROW`], where only the last goes.
fn moved(position: u32) -> Vec<u8> {
    let row = if position == FLOOD - 1 { LATEST_ROW } else { position % LATEST_ROW };
    let at = Point { column: position % COLUMNS, row, pixels: None, mods: 0 };
    let bytes = bytes_of(&Item::Event(Event::Mouse(Mouse { on: ON, action: MouseAction::Move, button: 0, at, device: Device::Mouse })));
    assert!(bytes.len() <= MAX_MOVE_BYTES);
    bytes
}

fn key(action: Action) -> Vec<u8> {
    bytes_of(&Item::Event(Event::Key(Key { on: ON, action, key: KeyCode::Character('a'), mods: 0, text: None, base: None, lost: false })))
}

fn paste(text: &str) -> Vec<u8> {
    bytes_of(&Item::Event(Event::Paste { on: ON, text }))
}

/// A channel whose program is this test, at the end of a socket, with one
/// connection on [`ON`].
async fn opened(name: &str) -> (Hub, BufReader<UnixStream>, mpsc::Receiver<hub::Outbound>) {
    // A socket's path is limited to about 100 bytes.
    let folder: PathBuf = std::env::temp_dir().join(format!("teletext-moves-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let path = folder.join("channel.sock");
    let link = apps::start(ChannelSpec { folder: name.to_owned(), name: "Moves".to_owned(), program: Program::Socket(path.clone()) }).unwrap();
    let config = ChannelConfig {
        name: "Moves".into(),
        address: "http://127.0.0.1/moves/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        session_grace_ms: 0,
    };
    let hub = hub::spawn(name, Channel::new(config), Instant::now(), Some(link), Arc::new(ServerStats::default()));
    let mut program = BufReader::new(UnixStream::connect(&path).await.unwrap());
    // Lines for a program not yet started are dropped.
    let start = line_of(&mut program, Instant::now() + WITHIN).await.unwrap();
    assert_eq!(start.get("type").and_then(Value::text), Some("start"));
    let (outbox, viewer) = mpsc::channel(hub::OUTBOX);
    hub.commands.send(Command::Opened { connection: CONNECTION, session: 1, cookies: Vec::new(), outbox }).await.unwrap();
    let hello = bytes_of(&Item::Event(Event::Hello { version: VERSION, caps: CURSOR_CAP | SELECT_CAP, agent: None }));
    let select = bytes_of(&Item::Event(Event::Select(Select::new(ON.page, ON.sub).unwrap(), "")));
    for bytes in [hello, select] {
        hub.commands.send(Command::Received { connection: CONNECTION, bytes }).await.unwrap();
    }
    (hub, program, viewer)
}

/// The next line the program reads, or none by `until`.
async fn line_of(program: &mut BufReader<UnixStream>, until: Instant) -> Option<Value> {
    let mut line = String::new();
    let read = tokio::time::timeout_at(until, program.read_line(&mut line)).await;
    read.is_ok_and(|read| read.unwrap() > 0).then(|| json::parse(line.as_bytes()).unwrap())
}

fn is_kind(line: &Value, kind: &str) -> bool {
    line.get("event").and_then(|event| event.get("kind")).and_then(Value::text) == Some(kind)
}

/// REQ-1360: a program that does not read while a viewer's pointer floods the
/// server is given, when it reads, moves at the pace and no backlog, and the
/// key pressed in the middle of the flood, and the latest position. It must
/// fail on the code of 2 November 2026, whose 4,096 lines fill with moves
/// and drop the key.
#[tokio::test]
async fn a_program_that_reads_slowly_loses_no_key_to_a_flood_of_moves() {
    let (hub, mut program, _viewer) = opened("flood").await;
    let began = Instant::now();
    let filler = "x".repeat(MAX_PASTE);
    for _ in 0..FILLING {
        hub.commands.send(Command::Received { connection: CONNECTION, bytes: paste(&filler) }).await.unwrap();
    }
    for position in 0..FLOOD {
        let bytes = if position == FLOOD / 2 {
            let mut both = key(Action::Press);
            both.extend(key(Action::Release));
            both
        } else {
            moved(position)
        };
        hub.commands.send(Command::Received { connection: CONNECTION, bytes }).await.unwrap();
    }
    let mut moves = 0_u128;
    let mut keys = 0;
    let mut latest = false;
    let until = Instant::now() + WITHIN;
    while keys < 2 || !latest {
        let Some(line) = line_of(&mut program, until).await else {
            panic!("the program stopped hearing, after {moves} moves and {keys} key events of 2, the latest position heard: {latest}");
        };
        if is_kind(&line, "mouse") {
            moves += 1;
            latest = line.get("event").and_then(|event| event.get("row")).and_then(Value::integer) == Some(i64::from(LATEST_ROW));
        } else if is_kind(&line, "key") {
            keys += 1;
        }
    }
    let elapsed = began.elapsed().as_millis();
    let most = elapsed * u128::from(MOVES_PER_SECOND) / 1_000 + WITHIN_THE_PACE;
    assert!(moves <= most, "{moves} moves in {elapsed} ms, at most {most} at the pace");
}
