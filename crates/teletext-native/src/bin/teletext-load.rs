//! `teletext-load`: opens many push WebSocket connections to a channel
//! and measures what decision 7 asks: how many the server holds, and how
//! long a change of one shared page takes to reach the last viewer.
//!
//! ```text
//! teletext-load --url ws://HOST:PORT/FOLDER/push --connections N [--page 105] [--seconds 60]
//! ```
//!
//! Every connection reads every message; each frame of `--page` is
//! grouped with the same frame at the other connections, and the spread
//! between its first and last arrival is the fan-out time.

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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// A T42 packet's bytes.
const PACKET: usize = 42;
/// How often the client reports.
const REPORT: Duration = Duration::from_secs(5);
/// Connections opened at a time, so the listener's backlog is not
/// overrun.
const BATCH: usize = 500;

struct Arguments {
    host: String,
    path: String,
    connections: usize,
    page: u16,
    seconds: u64,
}

fn arguments() -> Result<Arguments, String> {
    let mut url = None;
    let (mut connections, mut page, mut seconds) = (100_usize, 0x105_u16, 60_u64);
    let mut list = std::env::args().skip(1);
    while let Some(argument) = list.next() {
        let value = list.next().ok_or_else(|| format!("{argument} needs a value"))?;
        match argument.as_str() {
            "--url" => url = Some(value),
            "--connections" => connections = value.parse().map_err(|_| "--connections is a number")?,
            "--page" => page = u16::from_str_radix(&value, 16).map_err(|_| "--page is hexadecimal, such as 105")?,
            "--seconds" => seconds = value.parse().map_err(|_| "--seconds is a number")?,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let url = url.ok_or("--url ws://HOST:PORT/FOLDER/push is needed")?;
    let rest = url.strip_prefix("ws://").ok_or("only ws:// is supported")?;
    let (host, path) = rest.split_once('/').ok_or("the URL needs a path")?;
    Ok(Arguments { host: host.to_owned(), path: format!("/{path}"), connections, page, seconds })
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
    runtime.block_on(run(arguments));
    ExitCode::SUCCESS
}

async fn run(arguments: Arguments) {
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
            opening.push(tokio::spawn(async move { connect(&arguments, start, &open, &messages, &bytes, &arrivals).await }));
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

async fn connect(arguments: &Arguments, start: Instant, open: &AtomicU64, messages: &AtomicU64, bytes: &AtomicU64, arrivals: &Arrivals) {
    let Ok(mut stream) = TcpStream::connect(&arguments.host).await else { return };
    let _ = stream.set_nodelay(true);
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        arguments.path, arguments.host
    );
    if stream.write_all(request.as_bytes()).await.is_err() {
        return;
    }
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = vec![0_u8; 16 * 1024];
    // The 101 response's end.
    loop {
        let Ok(count) = stream.read(&mut chunk).await else { return };
        if count == 0 {
            return;
        }
        buffer.extend_from_slice(chunk.get(..count).unwrap_or(&[]));
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            if !buffer.starts_with(b"HTTP/1.1 101") {
                return;
            }
            buffer.drain(..end.saturating_add(4));
            break;
        }
    }
    // `subscribe all`, masked with a zero key, as a client must mask.
    let subscribe = b"subscribe all";
    let mut frame = vec![0x81, 0x80 | u8::try_from(subscribe.len()).unwrap_or(0), 0, 0, 0, 0];
    frame.extend_from_slice(subscribe);
    if stream.write_all(&frame).await.is_err() {
        return;
    }
    open.fetch_add(1, Ordering::Relaxed);
    loop {
        while let Some((payload, used)) = server_frame(&buffer) {
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
            buffer.drain(..used);
        }
        let Ok(count) = stream.read(&mut chunk).await else { break };
        if count == 0 {
            break;
        }
        buffer.extend_from_slice(chunk.get(..count).unwrap_or(&[]));
    }
    open.fetch_sub(1, Ordering::Relaxed);
}

/// One unmasked server frame's payload and the bytes it took.
fn server_frame(buffer: &[u8]) -> Option<(Vec<u8>, usize)> {
    let second = *buffer.get(1)?;
    let (length, at) = match second & 0x7F {
        126 => (usize::from(u16::from_be_bytes([*buffer.get(2)?, *buffer.get(3)?])), 4_usize),
        127 => {
            let mut word = [0_u8; 8];
            word.copy_from_slice(buffer.get(2..10)?);
            (usize::try_from(u64::from_be_bytes(word)).ok()?, 10_usize)
        }
        short => (usize::from(short), 2_usize),
    };
    let end = at.checked_add(length)?;
    Some((buffer.get(at..end)?.to_vec(), end))
}

/// The page number of a frame's first packet, if it is a header (EN 300
/// 706 clause 7.1.2: magazine and row Hamming 8/4 coded, then the page's
/// units and tens).
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
