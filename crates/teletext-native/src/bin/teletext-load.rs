//! `teletext-load`: drives a channel through its push WebSocket, as
//! receivers do, and measures the server under load.
//!
//! ```text
//! teletext-load --url ws://HOST:PORT/FOLDER/push --mode hold    [--connections N] [--page 105] [--seconds 60]
//! teletext-load --url ws://HOST:PORT/FOLDER/push --mode churn   [--rate R] [--page 100] [--seconds 60]
//! teletext-load --url ws://HOST:PORT/FOLDER/push --mode answers [--connections N] [--pages 100,101,102] [--selections S] [--rounds R]
//! ```
//!
//! Every connection says HELLO and SELECTs a page (DOC-1921, DOC-1753),
//! as a receiver does, from a new session each time, since it keeps no
//! cookie.
//! - **hold** (decision 7): opens many connections and holds them; each
//!   frame of `--page` is grouped with the same frame at the other
//!   connections, and the spread between its first and last arrival is the
//!   fan-out time.
//! - **churn** (TODO-1301): opens `--rate` connections a second, each a new
//!   session that takes its first answer and closes, so the server keeps a
//!   session waiting for its viewer for each (REQ-1040).
//! - **answers** (TODO-1521, REQ-1481): each connection selects `--pages`
//!   in turn, `--selections` times, and times each selection from the
//!   SELECT sent to its answer arrived, a frame of the page or a DENIED
//!   record. The first selection of a connection is answered after the
//!   program's answer to `connected` (the server waits for it), the others
//!   after its answer to `selected`; the two are reported apart. With
//!   `--rounds`, the whole is done that many times one after another, so
//!   that many first selections are timed without opening them all at once.

#![forbid(unsafe_code)]
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use neoteletext_core::ecc::{Status, hamming84_decode};
use neoteletext_core::packet::Packet;
use neoteletext_input::MAX_MESSAGE;
use neoteletext_input::VERSION;
use neoteletext_input::caps::{CURSOR_CAP, SELECT_CAP};
use neoteletext_input::event::{Event, Item, Select, write};
use neoteletext_web::consts::PUSH_SUBPROTOCOL;
use neoteletext_web::records::{ChannelRecord, Read, RecordReader};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// A T42 packet's bytes.
const PACKET: usize = 42;
/// How often the client reports.
const REPORT: Duration = Duration::from_secs(5);
/// Connections opened at a time, so the listener's backlog is not
/// overrun.
const BATCH: usize = 500;
/// The sub-code that asks for a page's first sub-page (DOC-1753).
const ANY_SUB_PAGE: u16 = 0x3F7F;
/// The longest a selection is waited for before it counts as unanswered:
/// beyond the server's own deadline, after which it sends DENIED 503.
const ANSWER_WAIT: Duration = Duration::from_secs(30);
/// A WebSocket frame's first byte: the last fragment of a binary message
/// (RFC 6455 section 5.2, FIN and opcode 2).
const BINARY_FRAME: u8 = 0x82;
/// The first byte of a close frame (FIN and opcode 8).
const CLOSE_FRAME: u8 = 0x88;
/// The mask bit of a frame's second byte, which a client must set.
const MASKED: u8 = 0x80;
/// The 7-bit length's marker of a 16-bit length that follows.
const LENGTH_16: u8 = 126;
/// The 7-bit length's marker of a 64-bit length that follows.
const LENGTH_64: u8 = 127;
/// The longest payload the 7-bit length holds.
const SHORT_LENGTH: usize = 125;
/// The bits of a frame's 7-bit length.
const LENGTH_BITS: u8 = 0x7F;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Hold,
    Churn,
    Answers,
}

struct Arguments {
    host: String,
    path: String,
    mode: Mode,
    connections: usize,
    page: u16,
    pages: Vec<u16>,
    selections: usize,
    rounds: usize,
    rate: u64,
    seconds: u64,
}

fn arguments() -> Result<Arguments, String> {
    let mut url = None;
    let (mut mode, mut connections, mut page, mut seconds, mut rate, mut selections, mut rounds) =
        (Mode::Hold, 100_usize, 0x105_u16, 60_u64, 10_u64, 100_usize, 1_usize);
    let mut pages = vec![0x100_u16];
    let hexadecimal = |value: &str| u16::from_str_radix(value, 16).map_err(|_| format!("{value} is not a page in hexadecimal, such as 105"));
    let mut list = std::env::args().skip(1);
    while let Some(argument) = list.next() {
        let value = list.next().ok_or_else(|| format!("{argument} needs a value"))?;
        match argument.as_str() {
            "--url" => url = Some(value),
            "--mode" => {
                mode = match value.as_str() {
                    "hold" => Mode::Hold,
                    "churn" => Mode::Churn,
                    "answers" => Mode::Answers,
                    other => return Err(format!("--mode is hold, churn or answers, not {other}")),
                }
            }
            "--connections" => connections = value.parse().map_err(|_| "--connections is a number")?,
            "--page" => page = hexadecimal(&value)?,
            "--pages" => pages = value.split(',').map(hexadecimal).collect::<Result<_, _>>()?,
            "--selections" => selections = value.parse().map_err(|_| "--selections is a number")?,
            "--rounds" => rounds = value.parse().map_err(|_| "--rounds is a number")?,
            "--rate" => rate = value.parse().map_err(|_| "--rate is a number of connections a second")?,
            "--seconds" => seconds = value.parse().map_err(|_| "--seconds is a number")?,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let url = url.ok_or("--url ws://HOST:PORT/FOLDER/push is needed")?;
    let rest = url.strip_prefix("ws://").ok_or("only ws:// is supported")?;
    let (host, path) = rest.split_once('/').ok_or("the URL needs a path")?;
    if pages.is_empty() || rate == 0 {
        return Err("--pages needs a page and --rate more than 0".to_owned());
    }
    Ok(Arguments { host: host.to_owned(), path: format!("/{path}"), mode, connections, page, pages, selections, rounds, rate, seconds })
}

/// Arrivals of each frame of the page watched, by its content, as times
/// since the start.
type Arrivals = Arc<Mutex<BTreeMap<u64, (Duration, Duration, usize)>>>;

fn main() -> ExitCode {
    let arguments = match arguments() {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("teletext-load: {error}");
            return ExitCode::FAILURE;
        }
    };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread().enable_all().build() else { return ExitCode::FAILURE };
    match arguments.mode {
        Mode::Hold => runtime.block_on(hold(arguments)),
        Mode::Churn => runtime.block_on(churn(arguments)),
        Mode::Answers => runtime.block_on(answers(arguments)),
    }
    ExitCode::SUCCESS
}

async fn hold(arguments: Arguments) {
    let start = Instant::now();
    let open = Arc::new(AtomicU64::new(0));
    let messages = Arc::new(AtomicU64::new(0));
    let bytes = Arc::new(AtomicU64::new(0));
    let arrivals: Arrivals = Arc::new(Mutex::new(BTreeMap::new()));
    let arguments = Arc::new(arguments);
    for batch in (0..arguments.connections).collect::<Vec<_>>().chunks(BATCH) {
        let mut opening = Vec::new();
        for _ in batch {
            let (arguments, open, messages, bytes, arrivals) =
                (Arc::clone(&arguments), Arc::clone(&open), Arc::clone(&messages), Arc::clone(&bytes), Arc::clone(&arrivals));
            opening.push(tokio::spawn(async move { held(&arguments, start, &open, &messages, &bytes, &arrivals).await }));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(opening);
    }
    let deadline = start.checked_add(Duration::from_secs(arguments.seconds)).unwrap_or(start);
    let (mut last_messages, mut last_bytes) = (0, 0);
    while Instant::now() < deadline {
        tokio::time::sleep(REPORT).await;
        let (now_messages, now_bytes) = (messages.load(Ordering::Relaxed), bytes.load(Ordering::Relaxed));
        let spreads: Vec<u128> = {
            let mut table = arrivals.lock().unwrap_or_else(PoisonError::into_inner);
            let done: Vec<u128> = table.values().filter(|&&(_, _, count)| count > 0).map(|&(first, last, _)| last.saturating_sub(first).as_millis()).collect();
            // Keep only the last few frames' arrivals.
            while table.len() > 8 {
                let Some(&oldest) = table.keys().next() else { break };
                table.remove(&oldest);
            }
            done
        };
        println!(
            "{:>5}s open {:>7}  messages/s {:>9}  bytes/s {:>11}  fan-out of page {:03X}, last frames: {:?} ms",
            start.elapsed().as_secs(),
            open.load(Ordering::Relaxed),
            now_messages.saturating_sub(last_messages).checked_div(REPORT.as_secs()).unwrap_or(0),
            now_bytes.saturating_sub(last_bytes).checked_div(REPORT.as_secs()).unwrap_or(0),
            arguments.page,
            spreads.iter().rev().take(3).collect::<Vec<_>>()
        );
        (last_messages, last_bytes) = (now_messages, now_bytes);
    }
}

async fn held(arguments: &Arguments, start: Instant, open: &AtomicU64, messages: &AtomicU64, bytes: &AtomicU64, arrivals: &Arrivals) {
    let Some(mut connection) = Connection::open(arguments).await else { return };
    if !connection.select(arguments.page, true).await {
        return;
    }
    open.fetch_add(1, Ordering::Relaxed);
    while let Some(payload) = connection.next_message().await {
        messages.fetch_add(1, Ordering::Relaxed);
        bytes.fetch_add(u64::try_from(payload.len()).unwrap_or(0), Ordering::Relaxed);
        if header_page(&payload) == Some(arguments.page) {
            let key = hash(&payload);
            let at = start.elapsed();
            let mut table = arrivals.lock().unwrap_or_else(PoisonError::into_inner);
            let entry = table.entry(key).or_insert((at, at, 0));
            entry.1 = entry.1.max(at);
            entry.2 = entry.2.saturating_add(1);
        }
    }
    open.fetch_sub(1, Ordering::Relaxed);
}

async fn churn(arguments: Arguments) {
    let start = Instant::now();
    let opened = Arc::new(AtomicU64::new(0));
    let answered = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let arguments = Arc::new(arguments);
    let gap = Duration::from_secs(1).checked_div(u32::try_from(arguments.rate).unwrap_or(u32::MAX)).unwrap_or(Duration::from_millis(1));
    let deadline = start.checked_add(Duration::from_secs(arguments.seconds)).unwrap_or(start);
    let mut next_report = start.checked_add(REPORT).unwrap_or(start);
    let mut next_open = start;
    while Instant::now() < deadline {
        tokio::time::sleep_until(next_open.into()).await;
        next_open = next_open.checked_add(gap).unwrap_or(next_open);
        let (task_arguments, task_opened, task_answered, task_failed) =
            (Arc::clone(&arguments), Arc::clone(&opened), Arc::clone(&answered), Arc::clone(&failed));
        tokio::spawn(async move {
            task_opened.fetch_add(1, Ordering::Relaxed);
            let page = task_arguments.page;
            let done = match Connection::open(&task_arguments).await {
                Some(mut connection) => connection.select(page, true).await && connection.answer(page).await.is_some() && connection.close().await,
                None => false,
            };
            let counter = if done { &task_answered } else { &task_failed };
            counter.fetch_add(1, Ordering::Relaxed);
        });
        if Instant::now() >= next_report {
            next_report = next_report.checked_add(REPORT).unwrap_or(next_report);
            println!(
                "{:>5}s opened {:>8}  answered and closed {:>8}  failed {:>6}",
                start.elapsed().as_secs(),
                opened.load(Ordering::Relaxed),
                answered.load(Ordering::Relaxed),
                failed.load(Ordering::Relaxed)
            );
        }
    }
}

async fn answers(arguments: Arguments) {
    let arguments = Arc::new(arguments);
    let (mut first, mut later, mut unanswered) = (Vec::new(), Vec::new(), 0_usize);
    for _ in 0..arguments.rounds {
        let mut running = Vec::new();
        for _ in 0..arguments.connections {
            let arguments = Arc::clone(&arguments);
            running.push(tokio::spawn(async move { timed(&arguments).await }));
        }
        for task in running {
            let Ok((connection_first, connection_later, connection_unanswered)) = task.await else { continue };
            first.extend(connection_first);
            later.extend(connection_later);
            unanswered = unanswered.saturating_add(connection_unanswered);
        }
    }
    report("first selection (connected, then its reply)", &mut first);
    report("later selections (selected, then the page)", &mut later);
    println!("unanswered within {} s: {unanswered}", ANSWER_WAIT.as_secs());
}

/// One connection's selections: the first one's answer time, the later
/// ones', and how many went unanswered.
async fn timed(arguments: &Arguments) -> (Vec<Duration>, Vec<Duration>, usize) {
    let (mut first, mut later, mut unanswered) = (Vec::new(), Vec::new(), 0_usize);
    let Some(mut connection) = Connection::open(arguments).await else { return (first, later, 1) };
    for (index, &page) in arguments.pages.iter().cycle().take(arguments.selections).enumerate() {
        let sent = Instant::now();
        if !connection.select(page, index == 0).await {
            unanswered = unanswered.saturating_add(1);
            break;
        }
        match connection.answer(page).await {
            Some(()) if index == 0 => first.push(sent.elapsed()),
            Some(()) => later.push(sent.elapsed()),
            None => unanswered = unanswered.saturating_add(1),
        }
    }
    let _ = connection.close().await;
    (first, later, unanswered)
}

/// Prints the count, median, 99th percentile and largest of `times`.
fn report(what: &str, times: &mut [Duration]) {
    times.sort_unstable();
    let at = |fraction_in_hundredths: usize| {
        let index = times.len().saturating_sub(1).saturating_mul(fraction_in_hundredths).checked_div(100).unwrap_or(0);
        times.get(index).map_or(0.0, |time| time.as_secs_f64() * 1_000.0)
    };
    println!("{what}: {} timed, median {:.1} ms, 99th percentile {:.1} ms, largest {:.1} ms", times.len(), at(50), at(99), at(100));
}

/// A push connection, as a receiver opens it.
struct Connection {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl Connection {
    /// Connects and upgrades, or `None`.
    async fn open(arguments: &Arguments) -> Option<Self> {
        let mut stream = TcpStream::connect(&arguments.host).await.ok()?;
        let _ = stream.set_nodelay(true);
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: {PUSH_SUBPROTOCOL}\r\n\r\n",
            arguments.path, arguments.host
        );
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut buffer: Vec<u8> = Vec::new();
        let mut chunk = vec![0_u8; 16 * 1024];
        // The 101 response's end.
        loop {
            let count = stream.read(&mut chunk).await.ok()?;
            if count == 0 {
                return None;
            }
            buffer.extend_from_slice(chunk.get(..count).unwrap_or(&[]));
            if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                if !buffer.starts_with(b"HTTP/1.1 101") {
                    return None;
                }
                buffer.drain(..end.saturating_add(4));
                return Some(Self { stream, buffer });
            }
        }
    }

    /// Sends SELECT of `page`'s first sub-page, after HELLO when `hello`.
    async fn select(&mut self, page: u16, hello: bool) -> bool {
        let Some(selection) = Select::new(page, ANY_SUB_PAGE) else { return false };
        let mut items = Vec::new();
        if hello {
            items.push(Item::Event(Event::Hello { version: VERSION, caps: CURSOR_CAP | SELECT_CAP, agent: None }));
        }
        items.push(Item::Event(Event::Select(selection, "")));
        let mut payload = Vec::new();
        for item in &items {
            let mut one = vec![0_u8; MAX_MESSAGE];
            let Ok(length) = write(item, &mut one) else { return false };
            payload.extend_from_slice(one.get(..length).unwrap_or(&[]));
        }
        self.stream.write_all(&client_frame(BINARY_FRAME, &payload)).await.is_ok()
    }

    /// Waits for the answer to a selection of `page`: a frame whose header
    /// is the page's, or a DENIED record; `None` if the connection ends or
    /// nothing comes within [`ANSWER_WAIT`].
    async fn answer(&mut self, page: u16) -> Option<()> {
        let mut records = RecordReader::new();
        tokio::time::timeout(ANSWER_WAIT, async {
            while let Some(payload) = self.next_message().await {
                if header_page(&payload) == Some(page) {
                    return Some(());
                }
                for &bytes in payload.as_chunks::<PACKET>().0 {
                    let packet = Packet { bytes };
                    if matches!(records.push(&packet), Read::Record(ChannelRecord::Denied(_))) {
                        return Some(());
                    }
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    }

    /// The next whole message from the server, or `None` when it ends.
    async fn next_message(&mut self) -> Option<Vec<u8>> {
        let mut chunk = vec![0_u8; 16 * 1024];
        loop {
            if let Some((payload, used)) = server_frame(&self.buffer) {
                self.buffer.drain(..used);
                return Some(payload);
            }
            let count = self.stream.read(&mut chunk).await.ok()?;
            if count == 0 {
                return None;
            }
            self.buffer.extend_from_slice(chunk.get(..count).unwrap_or(&[]));
        }
    }

    /// Sends a close frame and shuts the connection.
    async fn close(mut self) -> bool {
        let sent = self.stream.write_all(&client_frame(CLOSE_FRAME, &[])).await.is_ok();
        let _ = self.stream.shutdown().await;
        sent
    }
}

/// A client's frame of `payload`, masked with a zero key, as a client must
/// mask (RFC 6455 section 5.3).
fn client_frame(first: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![first];
    match u16::try_from(payload.len()) {
        Ok(_) if payload.len() <= SHORT_LENGTH => frame.push(MASKED | u8::try_from(payload.len()).unwrap_or(0)),
        Ok(length) => {
            frame.push(MASKED | LENGTH_16);
            frame.extend_from_slice(&length.to_be_bytes());
        }
        Err(_) => {
            frame.push(MASKED | LENGTH_64);
            frame.extend_from_slice(&u64::try_from(payload.len()).unwrap_or(u64::MAX).to_be_bytes());
        }
    }
    frame.extend_from_slice(&[0, 0, 0, 0]);
    frame.extend_from_slice(payload);
    frame
}

/// One unmasked server frame's payload and the bytes it took.
fn server_frame(buffer: &[u8]) -> Option<(Vec<u8>, usize)> {
    let second = *buffer.get(1)?;
    let (length, at) = match second & LENGTH_BITS {
        LENGTH_16 => (usize::from(u16::from_be_bytes([*buffer.get(2)?, *buffer.get(3)?])), 4_usize),
        LENGTH_64 => {
            let mut word = [0_u8; 8];
            word.copy_from_slice(buffer.get(2..10)?);
            (usize::try_from(u64::from_be_bytes(word)).ok()?, 10_usize)
        }
        short => (usize::from(short), 2_usize),
    };
    let end = at.checked_add(length)?;
    Some((buffer.get(at..end)?.to_vec(), end))
}

/// The page number of a frame's first header packet, if it has one (EN
/// 300 706 clause 7.1.2: magazine and row Hamming 8/4 coded, then the
/// page's units and tens).
fn header_page(payload: &[u8]) -> Option<u16> {
    for packet in payload.chunks(PACKET) {
        let address = nibble(*packet.first()?)? | (nibble(*packet.get(1)?)? << 4);
        let (magazine, row) = (address & 0x7, address >> 3);
        if row == 0 {
            let units = nibble(*packet.get(2)?)?;
            let tens = nibble(*packet.get(3)?)?;
            let magazine = if magazine == 0 { 8 } else { u16::from(magazine) };
            return Some((magazine << 8) | (u16::from(tens) << 4) | u16::from(units));
        }
    }
    None
}

/// A Hamming 8/4 byte's value, if it decodes.
fn nibble(byte: u8) -> Option<u8> {
    let (value, status) = hamming84_decode(byte);
    (status != Status::Uncorrectable).then_some(value)
}

fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3))
}
