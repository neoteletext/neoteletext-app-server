use super::*;
use crate::hub::spawn;
use crate::websocket::{FIN, MASKED};
use teletext_runtime::{Channel, ChannelConfig, Remote};
use tokio::io::DuplexStream;

/// The server's Ping, which has no payload: two bytes.
const PING: [u8; 2] = [FIN | Opcode::Ping as u8, 0];
/// The server's Pong to an empty Ping.
const PONG: [u8; 2] = [FIN | Opcode::Pong as u8, 0];
/// The receiver's own heartbeat, in the receiver's own terms.
const RECEIVER_PING_AFTER: Duration = Duration::from_micros(PUSH_PING_AFTER_US);

/// A client's control frame of no payload, masked with the key 0 (RFC 6455
/// section 5.3: every client frame is masked; any key will do).
fn from_receiver(opcode: Opcode) -> [u8; 6] {
    [FIN | opcode as u8, MASKED, 0, 0, 0, 0]
}

/// Bytes either end of the in-memory connection holds unread: more than
/// any exchange here sends.
const IN_FLIGHT: usize = 64 * 1024;

/// A connection served on a channel with no pages, and the receiver's end.
fn connected() -> DuplexStream {
    let config = ChannelConfig {
        name: "test".to_owned(),
        push_address: "ws://127.0.0.1:1/test/push".to_owned(),
        initial_page: 0x100,
        epoch_at_start_ms: 0,
        session_grace_ms: 0,
    };
    let channel = Channel::new(config, vec![Box::new(Remote::new(Vec::new()))], 1);
    let hub = spawn("test", channel, Instant::now(), None, Arc::new(ServerStats::default()), None);
    let (receiver, server) = tokio::io::duplex(IN_FLIGHT);
    let (reading, writing) = tokio::io::split(server);
    tokio::spawn(async move { serve_halves(reading, writing, &[], hub, 1).await });
    receiver
}

/// The longest any wait here may take, on the stopped clock: past both
/// bounds, so a ping or a drop that never comes fails the test.
const WITHIN: Duration = PING_AFTER.saturating_add(DEAD_AFTER_PING).saturating_mul(2);

/// Two bytes from `receiver`: a control frame of no payload.
async fn frame(receiver: &mut DuplexStream) -> [u8; 2] {
    let mut bytes = [0_u8; 2];
    tokio::time::timeout(WITHIN, receiver.read_exact(&mut bytes)).await.expect("a frame in time").unwrap();
    bytes
}

/// Requirement 11.3: a connection heard nothing on is pinged after
/// `PING_AFTER`, and dropped, without a Close, when nothing answers within
/// `DEAD_AFTER_PING`. Before, it was never pinged, and stayed.
#[tokio::test(start_paused = true)]
async fn a_silent_connection_is_pinged_then_dropped() {
    let mut receiver = connected();
    let began = Instant::now();
    assert_eq!(frame(&mut receiver).await, PING);
    assert_eq!(began.elapsed(), PING_AFTER);
    let mut rest = Vec::new();
    tokio::time::timeout(WITHIN, receiver.read_to_end(&mut rest)).await.expect("dropped in time").unwrap();
    assert_eq!(rest, [] as [u8; 0], "dropped, with nothing more");
    assert_eq!(began.elapsed(), PING_AFTER.saturating_add(DEAD_AFTER_PING));
}

/// A receiver that answers is pinged again only after `PING_AFTER` more.
#[tokio::test(start_paused = true)]
async fn an_answered_ping_keeps_the_connection() {
    let mut receiver = connected();
    for _ in 0..3 {
        let began = Instant::now();
        assert_eq!(frame(&mut receiver).await, PING);
        assert_eq!(began.elapsed(), PING_AFTER);
        receiver.write_all(&from_receiver(Opcode::Pong)).await.unwrap();
    }
}

/// A receiver whose own heartbeat pings is answered, and never pinged:
/// on a quiet connection the keepalive costs the receiver's pings alone.
#[tokio::test(start_paused = true)]
async fn a_receiver_that_pings_is_answered_and_not_pinged() {
    let mut receiver = connected();
    for _ in 0..5 {
        tokio::time::sleep(RECEIVER_PING_AFTER).await;
        receiver.write_all(&from_receiver(Opcode::Ping)).await.unwrap();
        assert_eq!(frame(&mut receiver).await, PONG);
    }
}
