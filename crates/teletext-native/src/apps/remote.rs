//! A channel's program over a WebSocket (REQ-1044), for a program on
//! another machine: it connects to `<channel>program` with the channel's
//! secret, one program at a time, and each text message is a line of the
//! program protocol. Once in, it is served as a program on a Unix domain
//! socket is: its channel is told it started and sends it `start` and
//! `connected` for each open connection, its lines reach the channel, and
//! when it goes its channel is told it ended.
//!
//! The HTTP side checks the secret and claims the door before it answers
//! 101, or 200 to an extended CONNECT over HTTP/2 or HTTP/3 (REQ-6966), so
//! that a second program while one is connected is answered 409 and not
//! upgraded; the claim goes when the connection does. What arrives is the
//! connection's two halves, whatever carries them: a socket, a TLS
//! session, or a stream of an HTTP/2 or HTTP/3 connection.
//!
//! **Keepalive:** a program's connection crosses networks whose devices
//! forget quiet connections, and one whose machine is gone leaves no end to
//! read; were it kept, its claim would keep every later program out. So
//! this side pings it as it pings a viewer (`connection::PING_AFTER`) and
//! drops it when nothing at all comes back.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use neoteletext_web::consts::{CLOSE_PROTOCOL_ERROR, CLOSE_TOO_BIG};
use teletext_runtime::landing::{program_admitted, program_lines};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{FromApplication, LONGEST_LINE, Outgoing, Secret, started};
use crate::connection::{DEAD_AFTER_PING, PING_AFTER, write_frame};
use crate::websocket::{FrameError, Message, Opcode, Reader};

/// The longest message a program may send: a line of [`LONGEST_LINE`]
/// bytes and the `\r\n` a sender may end it with, as standard input allows
/// one (REQ-1345). A longer line within it is the runtime's to refuse.
pub const LONGEST_MESSAGE: usize = LONGEST_LINE + b"\r\n".len();
/// The bytes read from the connection at a time: a line is up to
/// [`LONGEST_LINE`], so reads are larger than a viewer's.
const READ_BYTES: usize = 64 * 1024;
/// Control frames waiting to be written: a pong and a ping at most, with
/// room to spare. One that does not fit is dropped, which RFC 6455 section
/// 5.5.3 allows of a pong when pings come faster than they are answered.
const CONTROL_FRAMES: usize = 4;

/// The half of a WebSocket's connection that is read, whatever carries it.
pub type Reading = Box<dyn AsyncRead + Send + Unpin>;
/// The half that is written.
pub type Writing = Box<dyn AsyncWrite + Send + Unpin>;

/// A connection of the program's, upgraded, with what was read past its
/// upgrade, and the claim that keeps a second program out while it lasts.
pub struct Arrival {
    reading: Reading,
    writing: Writing,
    early: Vec<u8>,
    claim: Claim,
}

impl std::fmt::Debug for Arrival {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Arrival").field("early", &self.early.len()).field("claim", &self.claim).finish_non_exhaustive()
    }
}

/// The HTTP side's way in for a channel's program over a WebSocket.
#[derive(Clone, Debug)]
pub struct Door {
    secret: Secret,
    connected: Arc<AtomicBool>,
    arrivals: mpsc::Sender<Arrival>,
}

/// While held, a program is connected or connecting: no other is let in.
#[derive(Debug)]
pub struct Claim(Arc<AtomicBool>);

impl Drop for Claim {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Door {
    /// Whether the `Authorization` header presented is the channel's secret
    /// as a bearer token, compared in time that does not depend on it.
    #[must_use]
    pub fn admits(&self, authorization: Option<&[u8]>) -> bool {
        program_admitted(authorization, self.secret.bytes())
    }

    /// The door to itself, unless a program is connected or connecting.
    #[must_use]
    pub fn claim(&self) -> Option<Claim> {
        self.connected.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).ok().map(|_| Claim(Arc::clone(&self.connected)))
    }

    /// Hands an upgraded connection to the channel's program task. It is
    /// dropped, and the claim with it, when the channel has stopped.
    pub async fn arrive(&self, reading: Reading, writing: Writing, early: Vec<u8>, claim: Claim) {
        let _ = self.arrivals.send(Arrival { reading, writing, early, claim }).await;
    }
}

/// A door for a program presenting `secret`, and where what comes in by it
/// arrives. The claim keeps arrivals to one at a time.
pub fn door(secret: Secret) -> (Door, mpsc::Receiver<Arrival>) {
    let (arrivals, arriving) = mpsc::channel(1);
    (Door { secret, connected: Arc::new(AtomicBool::new(false)), arrivals }, arriving)
}

/// Serves each program that comes in by the door, one at a time, until the
/// channel stops.
pub async fn await_programs(folder: String, mut arriving: mpsc::Receiver<Arrival>, arrived: mpsc::Sender<FromApplication>, mut outgoing: Outgoing) {
    while let Some(Arrival { reading, writing, early, claim }) = arriving.recv().await {
        tracing::info!(target: "program", channel = folder, "the program connected over a WebSocket");
        if !started(&arrived, &mut outgoing).await {
            return;
        }
        let reason = relay(reading, writing, &early, &arrived, &mut outgoing).await;
        tracing::info!(target: "program", channel = folder, reason, "the program went");
        if arrived.send(FromApplication::Ended).await.is_err() {
            return;
        }
        drop(claim);
    }
}

/// A frame the hearing side asks the telling side to write.
#[derive(Debug)]
enum Control {
    /// A ping or a pong, with its payload.
    Frame(Opcode, Vec<u8>),
    /// A Close with this payload, after which the relay ends for this
    /// reason.
    Close(Vec<u8>, String),
}

/// Carries lines both ways over the program's WebSocket until it goes;
/// answers why it did. As the relay of the other transports, each
/// direction runs on its own, so neither waits for the other (REQ-1345):
/// the hearing side asks the telling side, which alone writes, for the
/// control frames it needs.
pub(super) async fn relay<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reading: R,
    mut writing: W,
    early: &[u8],
    arrived: &mpsc::Sender<FromApplication>,
    outgoing: &mut Outgoing,
) -> String {
    let (control, mut controls) = mpsc::channel::<Control>(CONTROL_FRAMES);
    let reason = {
        let hearing = hear(reading, early, arrived, &control);
        let telling = async {
            loop {
                tokio::select! {
                    biased;
                    asked = controls.recv() => match asked {
                        Some(Control::Frame(opcode, payload)) => {
                            if !write_frame(&mut writing, opcode, &payload).await {
                                return "writing to it failed".to_owned();
                            }
                        }
                        Some(Control::Close(payload, reason)) => {
                            let _ = write_frame(&mut writing, Opcode::Close, &payload).await;
                            return reason;
                        }
                        None => return "the channel stopped".to_owned(),
                    },
                    line = outgoing.next() => {
                        let Some(line) = line else { return "the channel stopped".to_owned() };
                        // One line a message, without its newline (REQ-1044).
                        let text = line.strip_suffix(b"\n").unwrap_or(&line);
                        if !write_frame(&mut writing, Opcode::Text, text).await {
                            return "writing to it failed".to_owned();
                        }
                    }
                }
            }
        };
        tokio::select! {
            reason = hearing => reason,
            reason = telling => reason,
        }
    };
    let _ = writing.shutdown().await;
    reason
}

/// The hearing side: the program's lines to the channel, its pings
/// answered, and the connection pinged when quiet. Answers why the
/// connection is gone; a Close to write ends the relay from the telling
/// side, which this side waits for while it goes on reading.
async fn hear<R: AsyncRead + Unpin>(mut reading: R, early: &[u8], arrived: &mpsc::Sender<FromApplication>, control: &mpsc::Sender<Control>) -> String {
    let mut reader = Reader::with_limit(LONGEST_MESSAGE);
    reader.feed(early);
    let mut buffer = vec![0_u8; READ_BYTES];
    let mut heard = Instant::now();
    let mut pinged: Option<Instant> = None;
    let mut closing = false;
    loop {
        while !closing {
            let close = match reader.next_message() {
                Ok(None) => break,
                Ok(Some(Message::Text(text))) => {
                    for line in program_lines(&text) {
                        if arrived.send(FromApplication::Line(line.to_vec())).await.is_err() {
                            return "the channel stopped".to_owned();
                        }
                    }
                    continue;
                }
                // The protocol is text (REQ-1044); a pong only shows the
                // program is there, which any byte does.
                Ok(Some(Message::Binary(_) | Message::Pong)) => continue,
                Ok(Some(Message::Ping(payload))) => {
                    let _ = control.try_send(Control::Frame(Opcode::Pong, payload));
                    continue;
                }
                Ok(Some(Message::Close)) => (Vec::new(), "it closed the connection".to_owned()),
                Err(fault) => {
                    let code = if fault == FrameError::TooLong { CLOSE_TOO_BIG } else { CLOSE_PROTOCOL_ERROR };
                    (code.to_be_bytes().to_vec(), format!("it broke the WebSocket protocol: {fault:?}"))
                }
            };
            // Nothing more of it is read as messages; the telling side
            // writes the Close and ends the relay, unless it cannot.
            closing = true;
            let (payload, reason) = close;
            if control.try_send(Control::Close(payload, reason.clone())).is_err() {
                return reason;
            }
        }
        let due = pinged.map_or_else(|| heard.checked_add(PING_AFTER), |at| at.checked_add(DEAD_AFTER_PING)).unwrap_or(heard);
        tokio::select! {
            read = reading.read(&mut buffer) => match read {
                Ok(0) => return "the connection closed".to_owned(),
                Ok(count) => {
                    heard = Instant::now();
                    pinged = None;
                    if !closing {
                        reader.feed(buffer.get(..count).unwrap_or(&[]));
                    }
                }
                Err(error) => return format!("reading from it: {error}"),
            },
            () = tokio::time::sleep_until(due) => {
                if pinged.is_some() {
                    return "no answer to a ping".to_owned();
                }
                pinged = Some(Instant::now());
                let _ = control.try_send(Control::Frame(Opcode::Ping, Vec::new()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::DuplexStream;

    use super::*;
    use crate::apps::waiting;
    use crate::websocket::{FIN, MASKED};

    /// The program's end's buffer: far below a line, so writes wait on reads.
    const PIPE_BYTES: usize = 64 * 1024;

    /// A program's masked frame, with a key of zeros.
    fn masked(opcode: Opcode, payload: &[u8]) -> Vec<u8> {
        let mut frame = vec![FIN | opcode as u8, MASKED | u8::try_from(payload.len()).unwrap()];
        frame.extend_from_slice(&[0; 4]);
        frame.extend_from_slice(payload);
        frame
    }

    /// The next frame the server wrote: its opcode bits and payload.
    async fn frame(program: &mut DuplexStream) -> (u8, Vec<u8>) {
        let mut head = [0_u8; 2];
        program.read_exact(&mut head).await.unwrap();
        let mut payload = vec![0_u8; usize::from(head[1] & 0x7F)];
        program.read_exact(&mut payload).await.unwrap();
        (head[0] & 0x0F, payload)
    }

    struct Relayed {
        program: DuplexStream,
        to: waiting::ToProgram,
        from: mpsc::Receiver<FromApplication>,
        ended: tokio::task::JoinHandle<String>,
    }

    fn relayed() -> Relayed {
        let (server, program) = tokio::io::duplex(PIPE_BYTES);
        let (to, mut outgoing) = waiting::waiting();
        let (arrived, from) = mpsc::channel(16);
        let ended = tokio::spawn(async move {
            let (reading, writing) = tokio::io::split(server);
            relay(reading, writing, b"", &arrived, &mut outgoing).await
        });
        Relayed { program, to, from, ended }
    }

    /// REQ-1044: each text message's line reaches the channel, each line for
    /// the program is one text message without its newline, and a ping is
    /// answered with its payload.
    #[tokio::test]
    async fn lines_go_both_ways_and_pings_are_answered() {
        let mut relayed = relayed();
        relayed.program.write_all(&masked(Opcode::Text, b"{\"type\":\"log\"}")).await.unwrap();
        let Some(FromApplication::Line(line)) = relayed.from.recv().await else { panic!("a line") };
        assert_eq!(line, b"{\"type\":\"log\"}");
        assert!(relayed.to.line(None, b"{\"type\":\"start\"}\n".to_vec()));
        assert_eq!(frame(&mut relayed.program).await, (Opcode::Text as u8, b"{\"type\":\"start\"}".to_vec()));
        relayed.program.write_all(&masked(Opcode::Ping, b"are you there")).await.unwrap();
        assert_eq!(frame(&mut relayed.program).await, (Opcode::Pong as u8, b"are you there".to_vec()));
        relayed.program.write_all(&masked(Opcode::Close, b"")).await.unwrap();
        assert_eq!(frame(&mut relayed.program).await.0, Opcode::Close as u8);
        assert_eq!(relayed.ended.await.unwrap(), "it closed the connection");
    }

    /// REQ-1044: a message over the longest line is refused from its header,
    /// before its bytes are kept, with the Close that says too big.
    #[tokio::test]
    async fn a_message_over_the_longest_line_is_refused() {
        let mut relayed = relayed();
        let mut head = vec![FIN | Opcode::Text as u8, MASKED | 127];
        head.extend_from_slice(&u64::try_from(LONGEST_MESSAGE + 1).unwrap().to_be_bytes());
        head.extend_from_slice(&[0; 4]);
        relayed.program.write_all(&head).await.unwrap();
        assert_eq!(frame(&mut relayed.program).await, (Opcode::Close as u8, CLOSE_TOO_BIG.to_be_bytes().to_vec()));
        assert!(relayed.ended.await.unwrap().contains("TooLong"));
    }

    /// A quiet program is pinged, and one that answers nothing at all is
    /// dropped, so that its claim does not keep the next program out.
    #[tokio::test(start_paused = true)]
    async fn a_program_that_answers_nothing_is_dropped() {
        let mut relayed = relayed();
        let began = Instant::now();
        assert_eq!(frame(&mut relayed.program).await.0, Opcode::Ping as u8);
        assert!(began.elapsed() >= PING_AFTER);
        assert_eq!(relayed.ended.await.unwrap(), "no answer to a ping");
        assert!(began.elapsed() >= PING_AFTER + DEAD_AFTER_PING);
        drop(relayed.to);
    }

    /// The door lets one program in at a time, and the next once the claim
    /// is gone; it admits the secret alone.
    #[test]
    fn the_door_lets_one_in_at_a_time() {
        let (door, _arriving) = door(Secret::new(b"secret"));
        assert!(door.admits(Some(b"Bearer secret")));
        assert!(!door.admits(Some(b"Bearer secrets")));
        let claim = door.claim().unwrap();
        assert!(door.claim().is_none(), "a second is kept out");
        drop(claim);
        assert!(door.claim().is_some(), "the next is let in");
    }
}
