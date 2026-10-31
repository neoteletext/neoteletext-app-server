//! One WebSocket connection: its input to the channel, and what the
//! channel sends it, from its own queue.
//!
//! **Keepalive** (requirement 11.3): the receiver pings a connection it
//! has heard nothing on for a while and is answered here; this side pings
//! one it has heard nothing on for longer, which only a receiver busy
//! reading, or a dead one, leaves so quiet, and drops it when nothing at
//! all comes back. Both are WebSocket control frames (RFC 6455 sections
//! 5.5.2 and 5.5.3), which a receiver never displays.

use std::io::IoSlice;
use std::sync::Arc;
use std::time::Duration;

use teletext_runtime::frame;
use teletext_runtime::{ConnectionId, SessionId};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::hub::{Command, Hub, OUTBOX, Outbound};
use crate::stats::ServerStats;
use crate::websocket::{CLOSE_PROTOCOL_ERROR, Message, Opcode, Reader, header};
use neoteletext_web::consts::{PUSH_PING_AFTER_US, PUSH_PONG_WITHIN_US};

/// The bytes read from the socket at a time. Input is a few bytes a key;
/// a longer message arrives over several reads (decision 7 item 4).
const READ_BYTES: usize = 1_024;
/// A connection heard nothing on for this long is pinged: the receiver's
/// heartbeat (the engine's REQ-WEB-005) pings one it has heard nothing on
/// for `PUSH_PING_AFTER_US`, so a live receiver on a quiet connection has
/// always been heard by then, its own wait for the pong the margin.
pub const PING_AFTER: Duration = Duration::from_micros(PUSH_PING_AFTER_US.saturating_add(PUSH_PONG_WITHIN_US));
/// A ping answered by nothing within this long means the connection is
/// dead, the receiver's own bound for its pings.
pub const DEAD_AFTER_PING: Duration = Duration::from_micros(PUSH_PONG_WITHIN_US);

/// Serves one connection until it closes: `stream` is the socket hyper
/// upgraded, and `early` what hyper had read past the upgrade.
pub async fn serve(stream: TcpStream, early: &[u8], hub: Hub, connection: ConnectionId, session: SessionId, query: Option<String>) {
    let (reading, writing) = stream.into_split();
    serve_halves(reading, writing, early, hub, connection, session, query).await;
}

/// `serve` on the connection's two halves. The tests serve an in-memory
/// stream, whose delivery tokio's stopped clock governs: a socket's
/// is the kernel's, and the clock jumps to its next timer while a frame
/// is in the kernel's hands.
async fn serve_halves<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut reading: R,
    mut writing: W,
    early: &[u8],
    hub: Hub,
    connection: ConnectionId,
    session: SessionId,
    query: Option<String>,
) {
    let (outbox, mut own_queue) = mpsc::channel(OUTBOX);
    if hub.commands.send(Command::Opened { connection, session, query, outbox }).await.is_err() {
        return;
    }
    let stats = Arc::clone(&hub.stats);
    stats.connected();
    tracing::debug!(target: "connection", channel = hub.folder, connection, session, "opened");
    let mut reader = Reader::new();
    reader.feed(early);
    let mut buffer = vec![0_u8; READ_BYTES];
    // When anything was last heard, and when this side pinged since.
    let mut heard = Instant::now();
    let mut pinged: Option<Instant> = None;
    let mut open = true;
    while open {
        let due = pinged.map_or_else(|| heard.checked_add(PING_AFTER), |at| at.checked_add(DEAD_AFTER_PING)).unwrap_or(heard);
        tokio::select! {
            read = reading.read(&mut buffer) => {
                let Ok(count) = read else { break };
                if count == 0 {
                    break;
                }
                // Any byte at all shows the receiver is there.
                heard = Instant::now();
                pinged = None;
                reader.feed(buffer.get(..count).unwrap_or(&[]));
                open = messages(&mut reader, &mut writing, &hub, connection, &stats).await;
            }
            outbound = own_queue.recv() => match outbound {
                Some(Outbound::Bytes(bytes)) => open = write_binary(&mut writing, &bytes, &stats).await,
                Some(Outbound::Close(code)) => {
                    // RFC 6455 section 5.5.1: the status code first, two bytes.
                    let _ = write(&mut writing, Opcode::Close, &code.to_be_bytes(), &stats).await;
                    tracing::debug!(target: "connection", channel = hub.folder, connection, session, code, "closed by the server");
                    open = false;
                }
                None => open = false,
            },
            () = tokio::time::sleep_until(due) => {
                if pinged.is_some() {
                    // RFC 6455 section 7.1.7: a peer that answers nothing
                    // is not sent a Close; the connection is dropped.
                    tracing::debug!(target: "connection", channel = hub.folder, connection, session, "dropped: no answer to a ping");
                    open = false;
                } else {
                    pinged = Some(Instant::now());
                    open = write(&mut writing, Opcode::Ping, &[], &stats).await;
                }
            }
        }
    }
    let _ = writing.shutdown().await;
    stats.disconnected();
    let _ = hub.commands.send(Command::Closed { connection }).await;
    tracing::debug!(target: "connection", channel = hub.folder, connection, session, "closed");
}

/// Handles the client's whole messages; returns whether the connection
/// stays open.
async fn messages<W: AsyncWrite + Unpin>(reader: &mut Reader, writing: &mut W, hub: &Hub, connection: ConnectionId, stats: &ServerStats) -> bool {
    loop {
        match reader.next_message() {
            Ok(None) => return true,
            Ok(Some(Message::Binary(bytes))) => {
                if hub.commands.send(Command::Received { connection, bytes }).await.is_err() {
                    return false;
                }
            }
            // Upstream is binary input alone (INPUT.md).
            Ok(Some(Message::Text(_) | Message::Pong)) => {}
            Ok(Some(Message::Ping(payload))) => {
                if !write(writing, Opcode::Pong, &payload, stats).await {
                    return false;
                }
            }
            Ok(Some(Message::Close)) => {
                let _ = write(writing, Opcode::Close, &[], stats).await;
                return false;
            }
            Err(_) => {
                let _ = write(writing, Opcode::Close, &CLOSE_PROTOCOL_ERROR.to_be_bytes(), stats).await;
                return false;
            }
        }
    }
}

/// Writes binary bytes as one message, or as several when they are more
/// than a receiver takes in one (the runtime's `frame::messages`).
async fn write_binary<W: AsyncWrite + Unpin>(writing: &mut W, bytes: &[u8], stats: &ServerStats) -> bool {
    for part in frame::messages(bytes, frame::MOST_MESSAGE_BYTES) {
        if !write(writing, Opcode::Binary, part, stats).await {
            return false;
        }
    }
    true
}

/// Writes one message: the header and the payload, straight from its
/// shared buffer, in one write, so a frame is one segment when it fits.
async fn write<W: AsyncWrite + Unpin>(writing: &mut W, opcode: Opcode, payload: &[u8], stats: &ServerStats) -> bool {
    let (head, length) = header(opcode, payload.len());
    stats.sent(length.saturating_add(payload.len()));
    let mut head = head.get(..length).unwrap_or(&[]);
    let mut body = payload;
    while !head.is_empty() || !body.is_empty() {
        let Ok(written) = writing.write_vectored(&[IoSlice::new(head), IoSlice::new(body)]).await else { return false };
        if written == 0 {
            return false;
        }
        let from_head = written.min(head.len());
        head = head.get(from_head..).unwrap_or(&[]);
        body = body.get(written.saturating_sub(from_head)..).unwrap_or(&[]);
    }
    true
}

#[cfg(test)]
mod tests;
