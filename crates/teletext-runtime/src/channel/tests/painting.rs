//! Painting as a show on a channel (neoteletext-common's DEC-7031 item 6;
//! REQ-7024, REQ-7025, TODO-7027): the PAINT record a `regis` or `naplps`
//! message with `line_speed` or `default_rate` begins its generation with,
//! read back by the engine's reader, for each connection at its own
//! generation, inside an open frame, for a kept picture at each giving,
//! and never where the message said neither or the connection is not sent
//! the stream itself.

use alloc::vec::Vec;
use core::net::IpAddr;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use neoteletext_input::VERSION;
use neoteletext_input::caps::{CURSOR_CAP, NAPLPS_CAP, REGIS_CAP, SELECT_CAP};
use neoteletext_input::event::{Event, Item};
use neoteletext_web::consts::{MAX_DEFAULT_PAINT_RATE, MAX_LINE_SPEED, MIN_LINE_SPEED};
use neoteletext_web::records::paint::{self, PaintRecord};
use neoteletext_web::records::sheet::SheetKind;
use neoteletext_web::records::{TYPE_FRAME, TYPE_PAINT};
use serde_json::{Value, json};

use super::{Channel, ChannelConfig, Effect, all_sent_to, line, message, select, sent_to, subpage, told, with};
use crate::channel::InitialPage;
use crate::kept::{Paint, PictureLanguage};
use crate::protocol::check::{TYPE_NAPLPS, TYPE_REGIS};
use crate::protocol::tests::base64;

/// Every capability the server uses.
const BOTH: u32 = CURSOR_CAP | SELECT_CAP | REGIS_CAP | NAPLPS_CAP;
/// A receiver that draws neither picture language: shown the mosaic
/// fallback (PAR-24).
const NEITHER: u32 = CURSOR_CAP | SELECT_CAP;
/// The page and sub-code the tests' pictures are of.
const PAGE: u16 = 0x100;
const SUB: u16 = 1;
/// A line speed and a default rate the tests state: within their ranges,
/// and neither at a bound, so that a bound mistaken for another is seen.
const SPEED: u32 = 1_200;
const RATE: u16 = 20;
/// A ReGIS picture and a NAPLPS one (SO, into the graphics set).
const REGIS_BYTES: &[u8] = b"P[1,1]V[9,9]";
const NAPLPS_BYTES: &[u8] = b"\x0e";

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

/// A channel with its program running and no connection.
fn empty() -> Channel {
    let mut channel = Channel::new(config(), &crate::viewer::testing::secret(), crate::viewer::testing::random());
    channel.program_started(0);
    let _ = channel.take_effects();
    channel
}

/// Has `connection` select 100/0001 and the program answer it: what it
/// was sent with the answer.
fn show(channel: &mut Channel, connection: u64, now: u64) -> Vec<u8> {
    channel.received(connection, &message(channel, connection, &[select(PAGE, SUB)]), now);
    let request = selected_request(&channel.take_effects());
    let page = with(&subpage(), "/sub", json!("0001"));
    channel.from_program(&line(&json!({"type": "page", "connections": [connection], "request": request, "page": page})), now);
    sent_to(&channel.take_effects(), connection)
}

/// The request number of the selection the program was told last, by
/// `selected`, or by `connected` for a connection's first.
fn selected_request(effects: &[Effect]) -> Value {
    let lines = told(effects);
    lines
        .iter()
        .rev()
        .find(|line| line["type"] == "selected" || line["type"] == "connected")
        .map_or_else(|| panic!("{lines:?}"), |line| line["request"].clone())
}

/// Opens `connection` offering `caps` and shows it 100/0001: what it was
/// sent with the answer.
fn arrive(channel: &mut Channel, connection: u64, caps: u32, now: u64) -> Vec<u8> {
    channel.opened(connection, None, now);
    let hello = Item::Event(Event::Hello { version: VERSION, caps, agent: None });
    channel.received(connection, &message(channel, connection, &[hello, select(PAGE, SUB)]), now);
    let request = selected_request(&channel.take_effects());
    let page = with(&subpage(), "/sub", json!("0001"));
    channel.from_program(&line(&json!({"type": "page", "connections": [connection], "request": request, "page": page})), now);
    sent_to(&channel.take_effects(), connection)
}

/// Has `connection` select page 200 and the program answer it.
fn leave(channel: &mut Channel, connection: u64, now: u64) {
    channel.received(connection, &message(channel, connection, &[select(0x200, 0)]), now);
    let request = selected_request(&channel.take_effects());
    let page = with(&subpage(), "/page", json!("200"));
    channel.from_program(&line(&json!({"type": "page", "connections": [connection], "request": request, "page": page})), now);
    let _ = channel.take_effects();
}

/// A picture message of `kind` for 100/0001 with `extra` fields added.
fn picture(kind: &str, connections: &Value, restart: bool, data: &[u8], extra: &Value) -> Vec<u8> {
    let mut message = json!({"type": kind, "connections": connections, "page": "100", "sub": "0001", "restart": restart, "data": base64(data)});
    for (key, value) in extra.as_object().unwrap() {
        message[key] = value.clone();
    }
    line(&message)
}

/// Every record in `bytes`, as its type and body.
fn records(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut found = Vec::new();
    let mut record: Vec<u8> = Vec::new();
    let mut open = false;
    for bytes in bytes.as_chunks::<PACKET_BYTES>().0 {
        let Some(Ok(fragment)) = neoteletext_audio::idl::fragment(&Packet { bytes: *bytes }) else { continue };
        if fragment.index == 0 {
            record.clear();
            open = true;
        }
        if !open {
            continue;
        }
        record.extend_from_slice(&fragment.data);
        if fragment.last {
            open = false;
            let length = neoteletext_audio::record::framed_length(&record).unwrap();
            let (kind, body) = neoteletext_audio::record::split(&record[..length]).unwrap();
            found.push((kind, body.to_vec()));
        }
    }
    found
}

/// The picture stream's records of `bytes`, PAINT among them, in order:
/// each PAINT as the engine's reader reads it, each stream record as
/// its type, generation and offset.
#[derive(Debug, PartialEq, Eq)]
enum Picture {
    Paint(PaintRecord),
    Stream { kind: u8, generation: u16, offset: u32, data: Vec<u8> },
}

fn pictures(bytes: &[u8]) -> Vec<Picture> {
    records(bytes)
        .into_iter()
        .filter_map(|(kind, body)| match kind {
            TYPE_PAINT => Some(Picture::Paint(paint::read(&body).unwrap_or_else(|fault| panic!("the engine refuses the PAINT record: {fault:?}")))),
            TYPE_REGIS | TYPE_NAPLPS => Some(Picture::Stream {
                kind,
                generation: u16::from_be_bytes([body[4], body[5]]),
                offset: u32::from_be_bytes([body[6], body[7], body[8], body[9]]),
                data: body[10..].to_vec(),
            }),
            _ => None,
        })
        .collect()
}

/// The PAINT records of `bytes`.
fn paints(bytes: &[u8]) -> Vec<PaintRecord> {
    pictures(bytes).into_iter().filter_map(|found| if let Picture::Paint(record) = found { Some(record) } else { None }).collect()
}

/// The PAINT record the engine reads for 100/0001.
fn expected(language: SheetKind, generation: u16, line_speed: Option<u32>, default_rate: Option<u16>) -> Picture {
    Picture::Paint(PaintRecord {
        page: crate::page::address(PAGE),
        subcode: neoteletext_core::types::SubCode::new(SUB),
        language,
        generation,
        line_speed,
        default_rate,
    })
}

fn stream(kind: u8, generation: u16, offset: u32, data: &[u8]) -> Picture {
    Picture::Stream { kind, generation, offset, data: data.to_vec() }
}

/// The refusals the program was told: rule and detail.
fn refusals(effects: &[Effect]) -> Vec<(Value, Value)> {
    told(effects).into_iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["detail"].clone())).collect()
}

/// DEC-7031 item 6: each connection a restart begins a generation on is
/// sent PAINT immediately before that generation's first stream record,
/// naming the page, the sub-code, the language, the speed, the rate and
/// that connection's own generation; two connections at different
/// generation counters are each sent their own.
#[test]
fn paint_goes_immediately_before_each_connections_first_stream_record_at_its_own_generation() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    let _ = arrive(&mut channel, 2, BOTH, 0);
    // Connection 1's ReGIS counter moves on to 1, connection 2's stays at 0.
    channel.from_program(&picture("regis", &json!([1]), true, b"P[0,0]", &json!({})), 1);
    let _ = channel.take_effects();
    let paint = json!({"line_speed": SPEED, "default_rate": RATE});
    channel.from_program(&picture("regis", &json!([1, 2]), true, REGIS_BYTES, &paint), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    for (connection, generation) in [(1, 2), (2, 1)] {
        assert_eq!(
            pictures(&sent_to(&effects, connection)),
            [expected(SheetKind::Regis, generation, Some(SPEED), Some(RATE)), stream(TYPE_REGIS, generation, 0, REGIS_BYTES)],
            "connection {connection}"
        );
    }
    // An append goes on with the generation, with no PAINT.
    channel.from_program(&picture("regis", &json!([1, 2]), false, b"V[2,2]", &json!({})), 3);
    let effects = channel.take_effects();
    for (connection, generation) in [(1, 2), (2, 1)] {
        assert_eq!(pictures(&sent_to(&effects, connection)), [stream(TYPE_REGIS, generation, 12, b"V[2,2]")], "connection {connection}");
    }
}

/// DEC-7031 item 6: a NAPLPS restart with a line speed is sent PAINT of
/// language NAPLPS, with no rate, before its first NAPLPS record; ReGIS
/// with each value alone states only that one.
#[test]
fn paint_says_each_language_and_only_the_values_given() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    channel.from_program(&picture("naplps", &json!([1]), true, NAPLPS_BYTES, &json!({"line_speed": SPEED})), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    assert_eq!(pictures(&sent_to(&effects, 1)), [expected(SheetKind::Naplps, 1, Some(SPEED), None), stream(TYPE_NAPLPS, 1, 0, NAPLPS_BYTES)]);

    channel.from_program(&picture("regis", &json!([1]), true, REGIS_BYTES, &json!({"line_speed": MIN_LINE_SPEED})), 2);
    assert_eq!(
        pictures(&sent_to(&channel.take_effects(), 1)),
        [expected(SheetKind::Regis, 1, Some(MIN_LINE_SPEED), None), stream(TYPE_REGIS, 1, 0, REGIS_BYTES)]
    );
    channel.from_program(&picture("regis", &json!([1]), true, REGIS_BYTES, &json!({"default_rate": MAX_DEFAULT_PAINT_RATE})), 3);
    assert_eq!(
        pictures(&sent_to(&channel.take_effects(), 1)),
        [expected(SheetKind::Regis, 2, None, Some(MAX_DEFAULT_PAINT_RATE)), stream(TYPE_REGIS, 2, 0, REGIS_BYTES)]
    );
}

/// DEC-7031 item 6: a restart with no bytes still begins a generation, so
/// its PAINT goes before the empty stream record; a second restart is
/// sent PAINT with the new generation.
#[test]
fn an_empty_restart_is_painted_and_a_second_restart_names_the_new_generation() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    channel.from_program(&picture("regis", &json!([1]), true, b"", &json!({"line_speed": MAX_LINE_SPEED})), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    assert_eq!(pictures(&sent_to(&effects, 1)), [expected(SheetKind::Regis, 1, Some(MAX_LINE_SPEED), None), stream(TYPE_REGIS, 1, 0, b"")]);
    channel.from_program(&picture("regis", &json!([1]), true, REGIS_BYTES, &json!({"line_speed": SPEED, "default_rate": 1})), 2);
    assert_eq!(pictures(&sent_to(&channel.take_effects(), 1)), [expected(SheetKind::Regis, 2, Some(SPEED), Some(1)), stream(TYPE_REGIS, 2, 0, REGIS_BYTES)]);
}

/// DEC-7031 item 6, ANIM-21: inside a frame, PAINT and the stream records
/// are held with the rest and released together between the two FRAME
/// records, PAINT first, in one message.
#[test]
fn inside_a_frame_paint_and_the_stream_are_held_and_released_together() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    let mark = |mark: &str| line(&json!({"type": "frame", "connections": [1], "page": "100", "sub": "0001", "mark": mark}));
    channel.from_program(&mark("begin"), 1);
    channel.from_program(&picture("regis", &json!([1]), true, REGIS_BYTES, &json!({"line_speed": SPEED})), 2);
    let held = channel.take_effects();
    assert_eq!(refusals(&held), []);
    assert_eq!(all_sent_to(&held, 1), [] as [u8; 0], "held to the frame's end");
    channel.from_program(&mark("end"), 3);
    let effects = channel.take_effects();
    assert_eq!(effects.iter().filter(|effect| matches!(effect, Effect::Send { connection: 1, .. })).count(), 1, "one message");
    let sent = all_sent_to(&effects, 1);
    let kinds: Vec<u8> = records(&sent).into_iter().map(|(kind, _)| kind).collect();
    assert_eq!(kinds, [TYPE_FRAME, TYPE_PAINT, TYPE_REGIS, TYPE_FRAME]);
    assert_eq!(pictures(&sent), [expected(SheetKind::Regis, 1, Some(SPEED), None), stream(TYPE_REGIS, 1, 0, REGIS_BYTES)]);
}

/// DEC-7031 item 6 (a kept picture keeps the values): the viewer who
/// shows the page at the restart is sent PAINT with it, one who selects
/// the page later is sent PAINT before the whole kept stream, at its own
/// generation, and so is one who leaves and selects it again; an append
/// is sent with none.
#[test]
fn a_kept_picture_is_painted_at_the_restart_at_a_later_selection_and_at_reselection() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    let paint = json!({"line_speed": SPEED, "default_rate": RATE});
    channel.from_program(&picture("regis", &json!("kept"), true, REGIS_BYTES, &paint), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    assert_eq!(pictures(&sent_to(&effects, 1)), [expected(SheetKind::Regis, 1, Some(SPEED), Some(RATE)), stream(TYPE_REGIS, 1, 0, REGIS_BYTES)]);

    // Connection 2's counter is moved on first, so its generation differs.
    channel.opened(2, None, 2);
    let hello = Item::Event(Event::Hello { version: VERSION, caps: BOTH, agent: None });
    channel.received(2, &message(&channel, 2, &[hello]), 2);
    let _ = channel.take_effects();
    leave(&mut channel, 2, 2);
    channel.from_program(&line(&json!({"type": "regis", "connections": [2], "page": "200", "sub": "0000", "restart": true, "data": base64(b"P[0,0]")})), 2);
    let _ = channel.take_effects();
    let late = show(&mut channel, 2, 3);
    assert_eq!(pictures(&late), [expected(SheetKind::Regis, 2, Some(SPEED), Some(RATE)), stream(TYPE_REGIS, 2, 0, REGIS_BYTES)], "a later selection");

    leave(&mut channel, 1, 4);
    let again = show(&mut channel, 1, 5);
    assert_eq!(pictures(&again), [expected(SheetKind::Regis, 2, Some(SPEED), Some(RATE)), stream(TYPE_REGIS, 2, 0, REGIS_BYTES)], "a re-selection");

    channel.from_program(&picture("regis", &json!("kept"), false, b"V[2,2]", &json!({})), 6);
    let effects = channel.take_effects();
    for connection in [1, 2] {
        assert_eq!(pictures(&sent_to(&effects, connection)), [stream(TYPE_REGIS, 2, 12, b"V[2,2]")], "an append, connection {connection}");
    }
}

/// DEC-7031 item 6: a kept restart without the values clears what the
/// picture held: neither the viewer showing it nor a later one is sent
/// PAINT. `forget_picture`'s empty generation is sent none either.
#[test]
fn a_kept_restart_without_the_values_clears_them_and_forget_picture_sends_no_paint() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    channel.from_program(&picture("naplps", &json!("kept"), true, NAPLPS_BYTES, &json!({"line_speed": SPEED})), 1);
    assert_eq!(paints(&sent_to(&channel.take_effects(), 1)).len(), 1);
    channel.from_program(&picture("naplps", &json!("kept"), true, NAPLPS_BYTES, &json!({})), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    assert_eq!(pictures(&sent_to(&effects, 1)), [stream(TYPE_NAPLPS, 2, 0, NAPLPS_BYTES)], "the restart is sent no PAINT");
    let late = arrive(&mut channel, 2, BOTH, 3);
    assert_eq!(pictures(&late), [stream(TYPE_NAPLPS, 1, 0, NAPLPS_BYTES)], "nor a later viewer");

    channel.from_program(&picture("naplps", &json!("kept"), true, NAPLPS_BYTES, &json!({"line_speed": SPEED})), 4);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "forget_picture", "page": "100", "sub": "0001"})), 5);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    for connection in [1, 2] {
        let sent = pictures(&sent_to(&effects, connection));
        assert!(
            matches!(sent.as_slice(), [Picture::Stream { kind: TYPE_NAPLPS, offset: 0, data, .. }] if data.is_empty()),
            "connection {connection}: {sent:?}"
        );
    }
}

/// DEC-7031 item 6: a restart that states neither value sends no PAINT,
/// to a connection's own stream or a kept one; nor does an append.
#[test]
fn no_paint_is_sent_without_the_values() {
    for (kind, record, bytes) in [("regis", TYPE_REGIS, REGIS_BYTES), ("naplps", TYPE_NAPLPS, NAPLPS_BYTES)] {
        for audience in [json!([1]), json!("kept")] {
            // A fresh channel each: a connection's own stream takes no
            // kept picture (PAR-18).
            let mut channel = empty();
            let _ = arrive(&mut channel, 1, BOTH, 0);
            channel.from_program(&picture(kind, &audience, true, bytes, &json!({})), 1);
            channel.from_program(&picture(kind, &audience, false, bytes, &json!({})), 2);
            let effects = channel.take_effects();
            assert_eq!(refusals(&effects), []);
            let sent = pictures(&sent_to(&effects, 1));
            assert_eq!(sent.len(), 2, "{kind} to {audience}: {sent:?}");
            assert!(sent.iter().all(|found| matches!(found, Picture::Stream { kind: found, .. } if *found == record)), "{kind} to {audience}: {sent:?}");
        }
    }
}

/// DEC-7031 item 6 (architecture section 5): a snapshot connection is
/// sent a kept picture's stream with no PAINT, though the picture was
/// begun with the values.
#[test]
fn a_snapshot_is_sent_a_painted_kept_picture_without_paint() {
    let mut channel = empty();
    channel.from_program(
        &line(&json!({"type": "regis", "connections": "kept", "page": "100", "sub": "0000", "restart": true, "data": base64(REGIS_BYTES),
                      "line_speed": SPEED, "default_rate": RATE})),
        0,
    );
    assert_eq!(refusals(&channel.take_effects()), []);
    let snapshot = 50;
    channel.opened(snapshot, Some(IpAddr::from([192, 0, 2, 7])), 1);
    let mut request = [0_u8; 64];
    let length = neoteletext_input::snapshot::write_request(PAGE, 0, &mut request).unwrap();
    channel.received(snapshot, &request[..length], 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "snapshot_page", "snapshot": 1, "page": subpage(), "picture": {"kept": "0000"}})), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = all_sent_to(&effects, snapshot);
    let kinds: Vec<u8> = records(&sent).into_iter().map(|(kind, _)| kind).collect();
    assert_eq!(kinds, [TYPE_REGIS], "the kept stream, and no PAINT");
}

/// DEC-7031 item 6, PAR-24: a connection that did not agree the picture's
/// language is sent no PAINT, kept or by its own message: the one shown
/// the mosaic fallback, and one that agreed only the other language.
#[test]
fn a_connection_without_the_languages_bit_is_sent_no_paint() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, NEITHER, 0);
    let _ = arrive(&mut channel, 2, CURSOR_CAP | SELECT_CAP | NAPLPS_CAP, 0);
    let _ = arrive(&mut channel, 3, CURSOR_CAP | SELECT_CAP | REGIS_CAP, 0);
    let paint = json!({"line_speed": SPEED});
    for audience in [json!("kept"), json!([1, 2, 3])] {
        channel.from_program(&picture("regis", &audience, true, REGIS_BYTES, &paint), 1);
        let effects = channel.take_effects();
        assert_eq!(refusals(&effects), []);
        assert_eq!(paints(&all_sent_to(&effects, 1)), [], "the fallback, {audience}");
        assert_eq!(paints(&all_sent_to(&effects, 2)), [], "NAPLPS only, {audience}");
        assert_eq!(paints(&all_sent_to(&effects, 3)).len(), 1, "the one that agreed ReGIS, {audience}");
    }
    let late = arrive(&mut channel, 4, NEITHER, 2);
    assert_eq!(paints(&late), [], "a late fallback viewer of the kept picture");
}

/// DEC-7031 item 6: a PAINT record cannot be sent as raw packets, past
/// the message's check: `packets` refuses it `invalid_packet` and sends
/// nothing.
#[test]
fn a_paint_record_in_raw_packets_is_refused() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    let record = crate::records::paint(PAGE, SUB, PictureLanguage::Regis, 1, Paint { line_speed: Some(SPEED), default_rate: None }, &mut 0).unwrap();
    assert_eq!(records(&record).into_iter().map(|(kind, _)| kind).collect::<Vec<_>>(), [TYPE_PAINT]);
    channel.from_program(&line(&json!({"type": "packets", "connections": [1], "packets": base64(&record)})), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects).into_iter().map(|(rule, _)| rule).collect::<Vec<_>>(), [json!("invalid_packet")]);
    assert_eq!(sent_to(&effects, 1), [] as [u8; 0]);
}
