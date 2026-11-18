//! The mosaic fallback as a receiver without a picture language's bit
//! receives it (neoteletext-common `docs/renderer-parity.md` PAR-24,
//! PAR-25; REQ-6769).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::{Decoded, Packet, decode};
use neoteletext_input::VERSION;
use neoteletext_input::caps::{CURSOR_CAP, NAPLPS_CAP, SELECT_CAP};
use neoteletext_input::event::{Event, Item};
use serde_json::{Value, json};

use super::{Channel, ChannelConfig, Effect, all_sent_to, line, message, select, subpage, with};
use crate::channel::InitialPage;
use crate::kept::{FALLBACK_INTERVAL_MS, MOST_FALLBACK_POOL_BYTES, MOST_KEPT_STREAM_BYTES};
use crate::protocol::tests::base64;

/// Teletext's red graphics attribute (EN 300 706 Table 26).
const RED_GRAPHICS: u8 = 0x11;
/// A receiver that draws neither picture language.
const NEITHER: u32 = CURSOR_CAP | SELECT_CAP;
/// The left half of the ReGIS screen in red (manual chapter 2, "HLS": red at hue 120; chapter 11).
const RED_LEFT: &[u8] = b"S(M1(H120L50S100))W(I1)P[0,0]F(V[399,0][399,479][0,479][0,0])";

fn config() -> ChannelConfig {
    ChannelConfig {
        name: "Test".into(),
        address: "http://127.0.0.1/test/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        default_rules: crate::channel::DefaultRules::None,
    }
}

fn empty() -> Channel {
    let mut channel = Channel::new(config(), &crate::viewer::testing::secret(), crate::viewer::testing::random());
    channel.program_started(0);
    let _ = channel.take_effects();
    channel
}

/// The program's sub-page 100/0001: row 0 a header, row 24 Fastext text.
fn program_page() -> Value {
    with(&subpage(), "/sub", json!("0001"))
}

/// Opens `connection` offering `caps` on 100/0001 and answers it at `now`.
fn arrive(channel: &mut Channel, connection: u64, caps: u32, now: u64) -> Vec<Effect> {
    channel.opened(connection, None, now);
    let hello = Item::Event(Event::Hello { version: VERSION, caps, agent: None });
    channel.received(connection, &message(channel, connection, &[hello, select(0x100, 1)]), now);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "page", "connections": [connection], "request": 1, "page": program_page()})), now);
    channel.take_effects()
}

fn picture(kind: &str, connections: &Value, restart: bool, data: &[u8]) -> Vec<u8> {
    line(&json!({"type": kind, "connections": connections, "page": "100", "sub": "0001", "restart": restart, "data": base64(data)}))
}

/// Each row's codes as last sent, by row number.
fn rows(bytes: &[u8]) -> BTreeMap<u8, Vec<u8>> {
    let mut rows = BTreeMap::new();
    for packet in bytes.as_chunks::<PACKET_BYTES>().0 {
        if let Ok(Decoded::Row(row)) = decode(&Packet { bytes: *packet }) {
            rows.insert(row.row, row.bytes.iter().map(|byte| byte & 0x7F).collect());
        }
    }
    rows
}

/// The program's own rows, as the encoder sends them.
fn program_rows() -> BTreeMap<u8, Vec<u8>> {
    let mut channel = empty();
    let effects = arrive(&mut channel, 9, NEITHER, 0);
    rows(&all_sent_to(&effects, 9))
}

/// PAR-25 verification 1: a ReGIS picture of a red rectangle on black
/// gives red mosaics where it lies on rows 1 to 23, and the program's rows
/// 0 and 24 unchanged; the connection was sent no REGIS record.
#[test]
fn a_regis_picture_is_shown_as_mosaics_on_rows_1_to_23() {
    let mut channel = empty();
    channel.from_program(&picture("regis", &json!("kept"), true, RED_LEFT), 0);
    let effects = arrive(&mut channel, 1, NEITHER, 1);
    let sent = all_sent_to(&effects, 1);
    let shown = rows(&sent);
    let middle = &shown[&12];
    assert!(middle[..20].contains(&RED_GRAPHICS), "{middle:?}");
    assert!(!middle[20..].contains(&RED_GRAPHICS));
    let program = program_rows();
    assert_eq!(shown.get(&24), program.get(&24), "row 24 is the program's");
    assert!(!super::record_types(&sent).contains(&crate::protocol::check::TYPE_REGIS));
}

/// PAR-25 verification 2: a NAPLPS picture sent to the connection itself,
/// which did not agree NAPLPS, is kept for its fallback and drawn.
#[test]
fn a_naplps_picture_sent_to_the_connection_is_drawn_for_it() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, NEITHER, 0);
    let before = channel.fallbacks_drawn();
    // A white rectangle on the left half (ANSI X3.110-1983 sections
    // 5.3.2.6, 5.3.3.1, 5.3.3.4), as the fallback's own test draws it.
    let stream = [0x0E, 0x3E, 0x5C, 0x24, 0x40, 0x40, 0x40, 0x31, 0x52, 0x47, 0x47];
    channel.from_program(&picture("naplps", &json!([1]), true, &stream), 1);
    let sent = all_sent_to(&channel.take_effects(), 1);
    assert_eq!(channel.fallbacks_drawn(), before + 1);
    assert!(rows(&sent).values().any(|row| row.contains(&0x17)), "white graphics drawn: {:?}", rows(&sent));
}

/// PAR-25 verification 3: twenty changes within 500 ms draw at most two
/// fallbacks, and the last state is shown.
#[test]
fn twenty_changes_within_the_interval_draw_at_most_two_fallbacks() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, NEITHER, 0);
    channel.from_program(&picture("regis", &json!("kept"), true, b"S(E)"), 10);
    let first = channel.fallbacks_drawn();
    for now in 11..30 {
        channel.from_program(&picture("regis", &json!("kept"), false, b" "), now);
    }
    channel.from_program(&picture("regis", &json!("kept"), false, RED_LEFT), 30);
    let _ = channel.take_effects();
    assert!(channel.next_wake().is_some_and(|wake| wake <= 10 + FALLBACK_INTERVAL_MS), "the last change is drawn within the interval");
    channel.deadline(10 + FALLBACK_INTERVAL_MS);
    let sent = all_sent_to(&channel.take_effects(), 1);
    assert!(channel.fallbacks_drawn() - first <= 1, "at most two drawings in all");
    assert!(rows(&sent)[&12][..20].contains(&RED_GRAPHICS), "the last state");
}

/// PAR-25 verification 4: a kept picture with a hundred connections
/// without the bit is drawn once.
#[test]
fn a_kept_picture_shown_to_a_hundred_connections_is_drawn_once() {
    let mut channel = empty();
    channel.from_program(&picture("regis", &json!("kept"), true, RED_LEFT), 0);
    for connection in 1..=100 {
        let _ = arrive(&mut channel, connection, NEITHER, 1);
    }
    assert_eq!(channel.fallbacks_drawn(), 1);
    // And a connection with the bit is never drawn for.
    let _ = arrive(&mut channel, 200, NEITHER | neoteletext_input::caps::REGIS_CAP, 2);
    assert_eq!(channel.fallbacks_drawn(), 1);
}

/// PAR-25 verification 5: the pool. Streams kept for fallbacks reach
/// `MOST_FALLBACK_POOL_BYTES` together; one byte more is dropped for its
/// connection, which keeps what it had, and the author is told.
#[test]
fn streams_kept_for_fallbacks_are_bounded_by_the_pool() {
    let mut channel = empty();
    let connections = u64::try_from(MOST_FALLBACK_POOL_BYTES / MOST_KEPT_STREAM_BYTES).unwrap();
    let full = alloc::vec![b' '; MOST_KEPT_STREAM_BYTES];
    for connection in 1..=connections + 1 {
        let _ = arrive(&mut channel, connection, NEITHER | NAPLPS_CAP, 0);
        channel.from_program(&picture("regis", &json!([connection]), true, &full), 1);
    }
    let effects = channel.take_effects();
    let dropped: Vec<u64> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Log { event: "fallback_dropped", text, .. } => Some(text.split_whitespace().nth(1)?.trim_end_matches(',').parse().ok()?),
            _ => None,
        })
        .collect();
    assert_eq!(dropped, [connections + 1], "the last connection passed the pool");
    let told = all_sent_to(&effects, connections + 1);
    assert!(super::record_types(&told).contains(&neoteletext_audio::record::TYPE_LOG), "its author is told");
    // One connection's own bound.
    channel.from_program(&picture("regis", &json!([1]), false, b" "), 2);
    let passed = channel
        .take_effects()
        .iter()
        .any(|effect| matches!(effect, Effect::Log { event: "fallback_dropped", text, .. } if text.contains("MOST_KEPT_STREAM_BYTES")));
    assert!(passed);
}
