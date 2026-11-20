//! `teletext-load`: drives a channel through its push WebSocket, as
//! receivers do, and measures the server under load.
//!
//! ```text
//! teletext-load --url ws://HOST:PORT/FOLDER/push --mode hold    [--connections N] [--page 105] [--seconds 60]
//!               [--caps none|regis|naplps] [--metrics HOST:PORT] [--sources N] [--open-rate R]
//! teletext-load --url ws://HOST:PORT/FOLDER/push --mode churn   [--rate R] [--page 100] [--seconds 60]
//! teletext-load --url ws://HOST:PORT/FOLDER/push --mode answers [--connections N] [--pages 100,101,102] [--selections S] [--rounds R]
//!
//! and with any mode: [--http 1|2|3 --ca FILE] [--tabs T] [--source-ports FIRST-LAST]
//! ```
//!
//! `--http` is the version of HTTP the tabs reach the server over (1
//! unless given): 1, each tab its own TCP connection and upgrade, without
//! TLS for `ws://` and with it for `wss://`; 2 or 3, each viewer one TLS connection over TCP (RFC 8441) or
//! one QUIC connection (RFC 9220), each of its tabs a stream of it, as a
//! browser sharing one connection among a channel's tabs does (DEC-6979).
//! The URL is then `https://HOST:PORT/FOLDER/push`. Over TLS `--ca` names
//! the certificate of the authority that signed the server's, in PEM. `--tabs` is how many of
//! `--connections` are one viewer's (1 unless given), each saying HELLO and
//! SELECT on its own WebSocket. `--source-ports` binds the TCP connections
//! to local ports from FIRST to LAST in turn, so that more connections
//! than the system's ephemeral ports can be held to one server from one
//! address, without changing the system's range.
//!
//! Every connection says HELLO and SELECTs a page (DOC-1921, DOC-1753),
//! as a receiver does, answering the server's viewer-state challenge as
//! a client that keeps no state (VS-501).
//! - **hold** (decision 7): opens many connections and holds them, and
//!   opens again whatever closes, a viewer's connection and then its tabs,
//!   as a browser does after a restart, reporting how long after the
//!   first loss every tab was open again (the reconnect storm); each
//!   frame of `--page` is grouped with the same frame at the other
//!   connections, and the spread between its first and last arrival is the
//!   fan-out time. With `--metrics`, the server's administration address,
//!   each report also gives the server's resident memory and what it grew
//!   by for each connection open since the run began (REQ-1023,
//!   TODO-6518). With `--caps`, each connection also agrees ReGIS or NAPLPS,
//!   so that a program may send it its own picture stream (PAR-23).
//!   With `--sources`, the connections come from that many loopback
//!   addresses, 127.0.0.1 upwards, so that more connections than one
//!   address's ephemeral ports can be held to a server on the loopback.
//!   `--open-rate` is how many connections are opened a second (10,000
//!   unless given), so that a program answering each one's `connected` is
//!   not given them faster than it answers.
//! - **churn** (TODO-1301): opens `--rate` connections a second, each of
//!   which takes its first answer and closes.
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
use std::future::Future;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use neoteletext_core::ecc::{Status, hamming84_decode};
use neoteletext_core::packet::Packet;
use neoteletext_input::MAX_MESSAGE;
use neoteletext_input::VERSION;
use neoteletext_input::caps::{CURSOR_CAP, NAPLPS_CAP, REGIS_CAP, SELECT_CAP};
use neoteletext_input::event::{Event, Item, Select, write};
use neoteletext_input::state::NoneReason;
use neoteletext_web::records::{ChannelRecord, Read, RecordReader};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use transport::{Duplex, Target, Version};

// The binary's root is in `src/bin`, so its module's file is named.
#[path = "teletext-load/transport.rs"]
mod transport;

/// A T42 packet's bytes.
const PACKET: usize = 42;
/// How often the client reports.
const REPORT: Duration = Duration::from_secs(5);
/// Connections opened at a time, so the listener's backlog is not
/// overrun.
const BATCH: usize = 500;
/// Connections `hold` opens a second unless `--open-rate` says otherwise:
/// a [`BATCH`] every 50 milliseconds.
const DEFAULT_OPEN_RATE: u64 = 10_000;
/// The most of the metrics answer read: the server's whole `/metrics`
/// text is a few kilobytes, and what is read from outside is bounded.
const MOST_METRICS_BYTES: usize = 1 << 20;
/// The family whose value is the server's resident memory, in bytes.
const RESIDENT_MEMORY: &str = "process_resident_memory_bytes";
/// The loopback addresses `--sources` may name: 127.0.0.1 to 127.0.0.254.
const MOST_SOURCES: usize = 254;
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
/// A ping frame's first byte (FIN and opcode 9): the server pings an idle
/// connection and closes it when no pong comes (RFC 6455 section 5.5.2).
const PING_FRAME: u8 = 0x89;
/// A pong frame's first byte (FIN and opcode 10), the answer to a ping.
const PONG_FRAME: u8 = 0x8A;
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
    /// Where the tabs go, over which version of HTTP.
    target: Target,
    mode: Mode,
    connections: usize,
    page: u16,
    pages: Vec<u16>,
    selections: usize,
    rounds: usize,
    rate: u64,
    seconds: u64,
    /// Capabilities HELLO adds to the cursor and SELECT.
    caps: u32,
    /// The server's administration address, for its resident memory.
    metrics: Option<String>,
    /// Connections `hold` opens a second.
    open_rate: u64,
    /// How many tabs are one viewer's.
    tabs: usize,
}

/// What [`arguments`] read, before the target is resolved.
/// Between a viewer's tries to reach a server that is not answering, so
/// that a viewer whose server is down does not spin; spread by viewer
/// over a further [`RETRY_SPREAD_MS`], so that the viewers of a restarted
/// server do not all try at once, as browsers' own waits differ.
const RETRY_AFTER: Duration = Duration::from_millis(250);
const RETRY_SPREAD_MS: u64 = 250;
/// How often `hold` looks whether every tab is open again after a loss.
const STORM_LOOK_EVERY: Duration = Duration::from_millis(20);

/// What the command line says, before the target is resolved.
struct Given {
    host: String,
    path: String,
    version: Version,
    /// `wss://`: HTTP/1.1 over TLS.
    secure: bool,
    ca: Option<String>,
    sources: usize,
    ports: Option<(u16, u16)>,
    rest: Box<dyn FnOnce(Target) -> Arguments>,
}

fn arguments() -> Result<Given, String> {
    let mut url = None;
    let (mut mode, mut connections, mut page, mut seconds, mut rate, mut selections, mut rounds) =
        (Mode::Hold, 100_usize, 0x105_u16, 60_u64, 10_u64, 100_usize, 1_usize);
    let mut pages = vec![0x100_u16];
    let (mut caps, mut metrics, mut sources, mut open_rate) = (0_u32, None, 1_usize, DEFAULT_OPEN_RATE);
    let (mut version, mut ca, mut tabs, mut ports) = (Version::Http1, None, 1_usize, None);
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
            "--caps" => {
                caps = match value.as_str() {
                    "none" => 0,
                    "regis" => REGIS_CAP,
                    "naplps" => NAPLPS_CAP,
                    other => return Err(format!("--caps is none, regis or naplps, not {other}")),
                }
            }
            "--metrics" => metrics = Some(value),
            "--open-rate" => open_rate = value.parse().ok().filter(|&rate| rate > 0).ok_or("--open-rate is a number of connections a second")?,
            "--sources" => {
                sources = value.parse().ok().filter(|count| (1..=MOST_SOURCES).contains(count)).ok_or("--sources is a number from 1 to 254")?;
            }
            "--http" => {
                version = match value.as_str() {
                    "1" => Version::Http1,
                    "2" => Version::Http2,
                    "3" => Version::Http3,
                    other => return Err(format!("--http is 1, 2 or 3, not {other}")),
                }
            }
            "--ca" => ca = Some(value),
            "--tabs" => tabs = value.parse().ok().filter(|&tabs| tabs > 0).ok_or("--tabs is a number of tabs a viewer, 1 or more")?,
            "--source-ports" => {
                let (lowest, highest) = value.split_once('-').ok_or("--source-ports is FIRST-LAST")?;
                let lowest: u16 = lowest.parse().map_err(|_| "--source-ports' first is a port")?;
                let highest: u16 = highest.parse().map_err(|_| "--source-ports' last is a port")?;
                let count = highest
                    .checked_sub(lowest)
                    .and_then(|span| span.checked_add(1))
                    .filter(|_| lowest > 0)
                    .ok_or("--source-ports is FIRST-LAST, FIRST at least 1 and not past LAST")?;
                ports = Some((lowest, count));
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let url = url.ok_or("--url ws://HOST:PORT/FOLDER/push is needed")?;
    let (rest, secure) = match version {
        Version::Http1 => match url.strip_prefix("wss://") {
            Some(rest) => (rest, true),
            None => (url.strip_prefix("ws://").ok_or("over HTTP/1.1 the URL is ws:// or wss://")?, false),
        },
        Version::Http2 | Version::Http3 => (url.strip_prefix("https://").ok_or("over HTTP/2 and HTTP/3 the URL is https://")?, true),
    };
    let (host, path) = rest.split_once('/').ok_or("the URL needs a path")?;
    if pages.is_empty() || rate == 0 {
        return Err("--pages needs a page and --rate more than 0".to_owned());
    }
    let rest = Box::new(move |target| Arguments { target, mode, connections, page, pages, selections, rounds, rate, seconds, caps, metrics, open_rate, tabs });
    Ok(Given { host: host.to_owned(), path: format!("/{path}"), version, secure, ca, sources, ports, rest })
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
    let Given { host, path, version, secure, ca, sources, ports, rest } = arguments;
    let target = match runtime.block_on(Target::new(host, path, version, secure, ca.as_deref(), sources, ports)) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("teletext-load: {error}");
            return ExitCode::FAILURE;
        }
    };
    let arguments = rest(target);
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
    let resident_before = match &arguments.metrics {
        Some(address) => match resident_memory(address).await {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                eprintln!("teletext-load: {error}");
                return;
            }
        },
        None => None,
    };
    let viewers = arguments.connections.div_ceil(arguments.tabs);
    let lost: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    for batch in (0..viewers).collect::<Vec<_>>().chunks(BATCH) {
        let mut opening = Vec::new();
        for &viewer in batch {
            let tabs = arguments.tabs.min(arguments.connections.saturating_sub(viewer.saturating_mul(arguments.tabs)));
            let counters = Counters {
                open: Arc::clone(&open),
                messages: Arc::clone(&messages),
                bytes: Arc::clone(&bytes),
                arrivals: Arc::clone(&arrivals),
                lost: Arc::clone(&lost),
            };
            let arguments = Arc::clone(&arguments);
            opening.push(tokio::spawn(async move { viewing(&arguments, viewer, tabs, start, &counters).await }));
        }
        let batch_size = u64::try_from(batch.len().saturating_mul(arguments.tabs)).unwrap_or(u64::MAX);
        let gap = Duration::from_millis(batch_size.saturating_mul(1_000).checked_div(arguments.open_rate).unwrap_or(0));
        tokio::time::sleep(gap).await;
        drop(opening);
    }
    let deadline = start.checked_add(Duration::from_secs(arguments.seconds)).unwrap_or(start);
    tokio::spawn(storms(Arc::clone(&open), Arc::clone(&lost), u64::try_from(arguments.connections).unwrap_or(u64::MAX), Arc::clone(&arguments)));
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
        let open_now = open.load(Ordering::Relaxed);
        let memory = match (&arguments.metrics, resident_before) {
            (Some(address), Some(before)) => match resident_memory(address).await {
                Ok(now) => format!(
                    "  server resident {now} bytes, {} a connection",
                    now.saturating_sub(before).checked_div(open_now).map_or_else(|| "none".to_owned(), |each| each.to_string())
                ),
                Err(error) => format!("  server resident unknown: {error}"),
            },
            _ => String::new(),
        };
        println!(
            "{:>5}s open {:>7}  messages/s {:>9}  bytes/s {:>11}  fan-out of page {:03X}, last frames: {:?} ms{memory}",
            start.elapsed().as_secs(),
            open_now,
            now_messages.saturating_sub(last_messages).checked_div(REPORT.as_secs()).unwrap_or(0),
            now_bytes.saturating_sub(last_bytes).checked_div(REPORT.as_secs()).unwrap_or(0),
            arguments.page,
            spreads.iter().rev().take(3).collect::<Vec<_>>()
        );
        (last_messages, last_bytes) = (now_messages, now_bytes);
    }
}

/// What every tab of `hold` adds to.
struct Counters {
    open: Arc<AtomicU64>,
    messages: Arc<AtomicU64>,
    bytes: Arc<AtomicU64>,
    arrivals: Arrivals,
    /// When a tab was first lost since every tab was last open.
    lost: Arc<Mutex<Option<Instant>>>,
}

/// One viewer of `hold`, `number` among them, with `tabs` tabs: its
/// connection, then its tabs, each held until it closes; and once every
/// tab has closed, all again.
async fn viewing(arguments: &Arguments, number: usize, tabs: usize, start: Instant, counters: &Counters) {
    let pause = RETRY_AFTER.saturating_add(Duration::from_millis(u64::try_from(number).unwrap_or(0).checked_rem(RETRY_SPREAD_MS).unwrap_or(0)));
    loop {
        let Some(viewer) = arguments.target.viewer(number).await else {
            tokio::time::sleep(pause).await;
            continue;
        };
        let viewer = Arc::new(tokio::sync::Mutex::new(viewer));
        let mut held = Vec::new();
        for tab in 0..tabs {
            let viewer = Arc::clone(&viewer);
            let tab_number = number.saturating_mul(arguments.tabs).saturating_add(tab);
            held.push(async move {
                let opened = {
                    let mut viewer = viewer.lock().await;
                    Connection::open_on(arguments, &mut viewer, tab_number).await
                };
                if let Some(connection) = opened {
                    hold_tab(arguments, connection, start, counters).await;
                }
            });
        }
        all(held).await;
        tokio::time::sleep(pause).await;
    }
}

/// Runs every one of `tasks` to its end, polled together on this task:
/// they borrow the viewer's arguments and counters.
async fn all<F: Future<Output = ()>>(tasks: Vec<F>) {
    let mut pinned: Vec<std::pin::Pin<Box<F>>> = tasks.into_iter().map(Box::pin).collect();
    std::future::poll_fn(|context| {
        pinned.retain_mut(|task| task.as_mut().poll(context).is_pending());
        if pinned.is_empty() { std::task::Poll::Ready(()) } else { std::task::Poll::Pending }
    })
    .await;
}

/// One tab of `hold`: SELECT, then every message counted until it closes.
async fn hold_tab(arguments: &Arguments, mut connection: Connection, start: Instant, counters: &Counters) {
    if !connection.select(arguments.page, true).await {
        return;
    }
    counters.open.fetch_add(1, Ordering::Relaxed);
    while let Some(payload) = connection.next_message().await {
        counters.messages.fetch_add(1, Ordering::Relaxed);
        counters.bytes.fetch_add(u64::try_from(payload.len()).unwrap_or(0), Ordering::Relaxed);
        if header_page(&payload) == Some(arguments.page) {
            let key = hash(&payload);
            let at = start.elapsed();
            let mut table = counters.arrivals.lock().unwrap_or_else(PoisonError::into_inner);
            let entry = table.entry(key).or_insert((at, at, 0));
            entry.1 = entry.1.max(at);
            entry.2 = entry.2.saturating_add(1);
        }
    }
    counters.open.fetch_sub(1, Ordering::Relaxed);
    counters.lost.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert_with(Instant::now);
}

/// Reports each reconnect storm: once tabs have been lost, how long after
/// the first loss all `total` were open again.
async fn storms(open: Arc<AtomicU64>, lost: Arc<Mutex<Option<Instant>>>, total: u64, arguments: Arc<Arguments>) {
    loop {
        tokio::time::sleep(STORM_LOOK_EVERY).await;
        if open.load(Ordering::Relaxed) < total {
            continue;
        }
        let Some(first_loss) = lost.lock().unwrap_or_else(PoisonError::into_inner).take() else { continue };
        let memory = match &arguments.metrics {
            Some(address) => resident_memory(address)
                .await
                .map_or_else(|error| format!(", server resident unknown: {error}"), |bytes| format!(", server resident {bytes} bytes")),
            None => String::new(),
        };
        println!(
            "storm: all {total} tabs open again {} ms after the first loss (at {} ms since the Unix epoch){memory}",
            first_loss.elapsed().as_millis(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_millis())
        );
    }
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
            let done = match Connection::open(&task_arguments, 0).await {
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
    let Some(mut connection) = Connection::open(arguments, 0).await else { return (first, later, 1) };
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
    stream: Box<dyn Duplex>,
    buffer: Vec<u8>,
    /// The capabilities HELLO gives besides the cursor and SELECT.
    caps: u32,
}

impl Connection {
    /// A viewer of its own, `number` among them, and its one tab, or
    /// `None`.
    async fn open(arguments: &Arguments, number: usize) -> Option<Self> {
        let mut viewer = arguments.target.viewer(number).await?;
        Self::open_on(arguments, &mut viewer, number).await
    }

    /// A tab, `number` among them, on `viewer`, or `None`.
    async fn open_on(arguments: &Arguments, viewer: &mut transport::Viewer, number: usize) -> Option<Self> {
        let (stream, buffer) = arguments.target.tab(viewer, number).await?;
        Some(Self { stream, buffer, caps: arguments.caps })
    }

    /// Sends SELECT of `page`'s first sub-page, after HELLO when `hello`.
    async fn select(&mut self, page: u16, hello: bool) -> bool {
        let Some(selection) = Select::new(page, ANY_SUB_PAGE) else { return false };
        let mut items = Vec::new();
        if hello {
            items.push(Item::Event(Event::Hello { version: VERSION, caps: CURSOR_CAP | SELECT_CAP | self.caps, agent: None }));
            // The answer to the challenge that follows WELCOME, given before
            // it comes, as a client that keeps no state may (VS-202, VS-501).
            items.push(Item::Event(Event::StateNone(NoneReason::Stateless)));
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

    /// The next whole message from the server, or `None` when it ends. A
    /// ping is answered with its pong, as a receiver does, so that a
    /// connection held for minutes stays open.
    async fn next_message(&mut self) -> Option<Vec<u8>> {
        let mut chunk = vec![0_u8; 16 * 1024];
        loop {
            if let Some((payload, used)) = server_frame(&self.buffer) {
                let first = self.buffer.first().copied();
                self.buffer.drain(..used);
                if first == Some(PING_FRAME) {
                    self.stream.write_all(&client_frame(PONG_FRAME, &payload)).await.ok()?;
                    continue;
                }
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

/// The server's resident memory in bytes, from `process_resident_memory_bytes`
/// of the `/metrics` its administration address `address` serves.
///
/// # Errors
/// What failed: the connection, an answer past [`MOST_METRICS_BYTES`], or
/// one without the figure.
async fn resident_memory(address: &str) -> Result<u64, String> {
    let mut stream = TcpStream::connect(address).await.map_err(|error| format!("metrics at {address}: {error}"))?;
    let request = format!("GET /metrics HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.map_err(|error| format!("metrics at {address}: {error}"))?;
    let mut answer = Vec::new();
    let mut chunk = vec![0_u8; 16 * 1024];
    loop {
        let count = stream.read(&mut chunk).await.map_err(|error| format!("metrics at {address}: {error}"))?;
        if count == 0 {
            break;
        }
        answer.extend_from_slice(chunk.get(..count).unwrap_or(&[]));
        if answer.len() > MOST_METRICS_BYTES {
            return Err(format!("metrics at {address}: the answer passes {MOST_METRICS_BYTES} bytes"));
        }
    }
    resident_in(&String::from_utf8_lossy(&answer)).ok_or_else(|| format!("metrics at {address}: no {RESIDENT_MEMORY} in the answer"))
}

/// The value of [`RESIDENT_MEMORY`] in a metrics text, if it has one.
fn resident_in(text: &str) -> Option<u64> {
    text.lines().find_map(|line| line.strip_prefix(RESIDENT_MEMORY)?.strip_prefix(' ')?.trim().parse::<f64>().ok()).and_then(whole_bytes)
}

/// A figure of the metrics text as whole bytes: none when it is negative,
/// not finite, or not a whole number within `u64`.
fn whole_bytes(value: f64) -> Option<u64> {
    let text = format!("{value:.0}");
    (value.is_finite() && value >= 0.0).then(|| text.parse().ok()).flatten()
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

#[cfg(test)]
mod tests {
    use super::resident_in;

    #[test]
    fn the_resident_memory_is_read_from_the_metrics_text() {
        let text = "# HELP process_resident_memory_bytes Resident memory.\n# TYPE process_resident_memory_bytes gauge\nprocess_resident_memory_bytes 12345678\nteletext_connections 3\n";
        assert_eq!(resident_in(text), Some(12_345_678));
        assert_eq!(resident_in("process_resident_memory_bytes 1.5e9\n"), Some(1_500_000_000));
    }

    #[test]
    fn a_text_without_the_figure_or_with_a_malformed_one_gives_none() {
        assert_eq!(resident_in("teletext_connections 3\n"), None);
        assert_eq!(resident_in("process_resident_memory_bytes many\n"), None);
        assert_eq!(resident_in("process_resident_memory_bytes -4\n"), None);
        assert_eq!(resident_in("process_resident_memory_bytes_total 4\n"), None);
    }
}
