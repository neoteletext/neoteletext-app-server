//! One WebSocket connection: its input to the channel, what is its own
//! from the channel, and the shared pages, which it catches up on by
//! itself from the channel's log (decision 7).

use std::collections::BTreeSet;
use std::io::IoSlice;
use std::sync::Arc;

use teletext_runtime::SessionId;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::hub::{Command, Hub, OUTBOX, Outbound};
use crate::websocket::{Message, Opcode, Reader, header};

/// The bytes read from the socket at a time. Input is a few bytes a key;
/// a longer message arrives over several reads (decision 7 item 4).
const READ_BYTES: usize = 1_024;

/// Serves one connection until it closes: `stream` is the socket hyper
/// upgraded, and `early` what hyper had read past the upgrade.
pub async fn serve(stream: TcpStream, early: &[u8], hub: Hub, session: SessionId) {
    let (outbox, mut own_queue) = mpsc::channel(OUTBOX);
    if hub.commands.send(Command::Opened { session, outbox }).await.is_err() {
        return;
    }
    let (mut reading, mut writing) = stream.into_split();
    let mut generation = hub.generation.clone();
    let mut sent = *generation.borrow_and_update();
    let mut own: BTreeSet<u16> = BTreeSet::new();
    let mut reader = Reader::new();
    reader.feed(early);
    let mut buffer = vec![0_u8; READ_BYTES];
    let mut open = send_pages(&mut writing, hub.pages.all(), &own).await;
    while open {
        tokio::select! {
            read = reading.read(&mut buffer) => {
                let Ok(count) = read else { break };
                if count == 0 {
                    break;
                }
                reader.feed(buffer.get(..count).unwrap_or(&[]));
                open = messages(&mut reader, &mut writing, &hub, session).await;
            }
            outbound = own_queue.recv() => match outbound {
                Some(Outbound::Bytes(bytes)) => open = write(&mut writing, Opcode::Binary, &bytes).await,
                Some(Outbound::Own(page)) => { own.insert(page); }
                Some(Outbound::Close) | None => open = false,
            },
            changed = generation.changed() => {
                if changed.is_err() {
                    break;
                }
                let now = *generation.borrow_and_update();
                let pages = match hub.pages.changed_since(sent) {
                    Some(changed) => changed.into_iter().filter_map(|page| hub.pages.frame(page).map(|frame| (page, frame))).collect(),
                    None => hub.pages.all(),
                };
                sent = now;
                open = send_pages(&mut writing, pages, &own).await;
            }
        }
    }
    let _ = writing.shutdown().await;
    let _ = hub.commands.send(Command::Closed { session }).await;
}

/// Handles the client's whole messages; returns whether the connection
/// stays open.
async fn messages<W: AsyncWrite + Unpin>(reader: &mut Reader, writing: &mut W, hub: &Hub, session: SessionId) -> bool {
    loop {
        match reader.next_message() {
            Ok(None) => return true,
            Ok(Some(Message::Binary(bytes))) => {
                if hub.commands.send(Command::Received { session, bytes }).await.is_err() {
                    return false;
                }
            }
            // `subscribe all`: every connection is sent every shared page.
            Ok(Some(Message::Text(_) | Message::Pong)) => {}
            Ok(Some(Message::Ping(payload))) => {
                if !write(writing, Opcode::Pong, &payload).await {
                    return false;
                }
            }
            Ok(Some(Message::Close)) | Err(_) => {
                let _ = write(writing, Opcode::Close, &[]).await;
                return false;
            }
        }
    }
}

/// Sends shared pages, but not those the session owns.
async fn send_pages<W: AsyncWrite + Unpin>(writing: &mut W, pages: Vec<(u16, Arc<[u8]>)>, own: &BTreeSet<u16>) -> bool {
    for (page, frame) in pages {
        if !own.contains(&page) && !write(writing, Opcode::Binary, &frame).await {
            return false;
        }
    }
    true
}

/// Writes one message: the header and the payload, straight from its
/// shared buffer, in one write, so a frame is one segment when it fits.
async fn write<W: AsyncWrite + Unpin>(writing: &mut W, opcode: Opcode, payload: &[u8]) -> bool {
    let (head, length) = header(opcode, payload.len());
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
