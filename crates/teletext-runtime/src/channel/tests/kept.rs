//! Kept pictures as viewers receive them (neoteletext-common
//! `docs/renderer-parity.md` PAR-16 to PAR-20, PAR-23, PAR-24; REQ-6766,
//! REQ-6767, REQ-6769): late and reconnected viewers, appends, streams of a
//! connection's own, `forget_picture`, NAPLPS links and cursor, the
//! server's NAPLPS check, and the capability bits.

extern crate std;

use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use neoteletext_input::VERSION;
use neoteletext_input::caps::{CURSOR_CAP, NAPLPS_CAP, REGIS_CAP, SELECT_CAP};
use neoteletext_input::event::{Event, Item};
use serde_json::{Value, json};

use super::{Channel, ChannelConfig, line, message, select, sent_to, subpage, told, with};
use crate::channel::InitialPage;
use crate::protocol::check::{TYPE_DRAWING_LINKS, TYPE_NAPLPS, TYPE_REGIS, TYPE_REGIS_CURSOR};
use crate::protocol::tests::base64;

/// The record type of `NAPLPS_OBJECTS` (ANIM-11).
const TYPE_NAPLPS_OBJECTS: u8 = neoteletext_audio::record::TYPE_NAPLPS_OBJECTS;
/// The record type of the author's log (the engine's REQ-INP-026).
const TYPE_LOG: u8 = neoteletext_audio::record::TYPE_LOG;
/// Every capability the server uses.
const BOTH: u32 = CURSOR_CAP | SELECT_CAP | REGIS_CAP | NAPLPS_CAP;

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

/// Opens `connection` offering `caps`, selects page 100 sub-code 0001 and
/// answers it: what it was sent after the answer's frame is returned.
fn arrive(channel: &mut Channel, connection: u64, caps: u32, now: u64) -> Vec<u8> {
    channel.opened(connection, None, now);
    let hello = Item::Event(Event::Hello { version: VERSION, caps, agent: None });
    channel.received(connection, &message(channel, connection, &[hello, select(0x100, 1)]), now);
    let _ = channel.take_effects();
    let page = with(&subpage(), "/sub", json!("0001"));
    channel.from_program(&line(&json!({"type": "page", "connections": [connection], "request": 1, "page": page})), now);
    sent_to(&channel.take_effects(), connection)
}

/// A picture message for page 100, sub-code 0001.
fn picture(kind: &str, connections: &Value, restart: bool, data: &[u8]) -> Vec<u8> {
    line(&json!({"type": kind, "connections": connections, "page": "100", "sub": "0001", "restart": restart, "data": base64(data)}))
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
            let Ok(length) = neoteletext_audio::record::framed_length(&record) else { continue };
            let (kind, body) = neoteletext_audio::record::split(&record[..length]).unwrap();
            found.push((kind, body.to_vec()));
        }
    }
    found
}

/// The stream records of `bytes` of type `kind`: (generation, offset, data).
fn streams(bytes: &[u8], kind: u8) -> Vec<(u16, u32, Vec<u8>)> {
    records(bytes)
        .into_iter()
        .filter(|(found, _)| *found == kind)
        .map(|(_, body)| (u16::from_be_bytes([body[4], body[5]]), u32::from_be_bytes([body[6], body[7], body[8], body[9]]), body[10..].to_vec()))
        .collect()
}

/// The types of the picture records of `bytes`, in order.
fn picture_types(bytes: &[u8]) -> Vec<u8> {
    let picture = [TYPE_REGIS, TYPE_NAPLPS, TYPE_NAPLPS_OBJECTS, TYPE_DRAWING_LINKS, TYPE_REGIS_CURSOR];
    records(bytes).into_iter().map(|(kind, _)| kind).filter(|kind| picture.contains(kind)).collect()
}

/// The refusals the program was told: rule and detail.
fn refusals(channel: &mut Channel) -> Vec<(Value, Value)> {
    told(&channel.take_effects()).into_iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["detail"].clone())).collect()
}

/// PAR-18 verification 1 and 2: a connection that shows the page when the
/// picture is kept receives it as it is sent; one that selects it later
/// receives the whole stream, the links and the cursor in that order right
/// after the page's frame, then the appends, both as their own generation.
#[test]
fn a_late_viewer_receives_the_kept_picture_whole_then_its_appends() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    channel.from_program(&picture("regis", &json!("kept"), true, b"P[1,1]V[9,9]"), 1);
    let first = sent_to(&channel.take_effects(), 1);
    assert_eq!(streams(&first, TYPE_REGIS), [(1, 0, b"P[1,1]V[9,9]".to_vec())]);
    channel.from_program(
        &line(&json!({"type": "regis_links", "connections": "kept", "page": "100", "sub": "0001", "links": [{"drawing": 1, "target": "200"}]})),
        2,
    );
    channel.from_program(&line(&json!({"type": "regis_cursor", "connections": "kept", "page": "100", "sub": "0001", "drawing": 1})), 3);
    assert_eq!(picture_types(&sent_to(&channel.take_effects(), 1)), [TYPE_DRAWING_LINKS, TYPE_REGIS_CURSOR]);

    let late = arrive(&mut channel, 2, BOTH, 4);
    assert_eq!(picture_types(&late), [TYPE_REGIS, TYPE_DRAWING_LINKS, TYPE_REGIS_CURSOR], "stream, links, cursor");
    assert_eq!(streams(&late, TYPE_REGIS), [(1, 0, b"P[1,1]V[9,9]".to_vec())]);
    let header = late.as_chunks::<PACKET_BYTES>().0.iter().position(|packet| neoteletext_audio::idl::fragment(&Packet { bytes: *packet }).is_none());
    assert_eq!(header, Some(0), "the page's frame comes first");

    channel.from_program(&picture("regis", &json!("kept"), false, b"V[2,2]"), 5);
    let effects = channel.take_effects();
    for connection in [1, 2] {
        assert_eq!(streams(&sent_to(&effects, connection), TYPE_REGIS), [(1, 12, b"V[2,2]".to_vec())], "connection {connection}");
    }
}

/// PAR-18 verification 3 and 4: a connection that closes and opens again
/// receives the stream from offset 0 of a new generation; one given a
/// stream of its own takes no kept appends.
#[test]
fn a_reconnection_starts_again_and_a_stream_of_its_own_takes_no_kept_appends() {
    let mut channel = empty();
    channel.from_program(&picture("regis", &json!("kept"), true, b"P[1,1]"), 0);
    let _ = arrive(&mut channel, 1, BOTH, 1);
    channel.closed(1, 2);
    let again = arrive(&mut channel, 3, BOTH, 3);
    assert_eq!(streams(&again, TYPE_REGIS), [(1, 0, b"P[1,1]".to_vec())]);

    channel.from_program(&picture("regis", &json!([3]), true, b"V[5,5]"), 4);
    assert_eq!(streams(&sent_to(&channel.take_effects(), 3), TYPE_REGIS), [(2, 0, b"V[5,5]".to_vec())]);
    channel.from_program(&picture("regis", &json!("kept"), false, b"V[6,6]"), 5);
    assert_eq!(sent_to(&channel.take_effects(), 3), [] as [u8; 0], "its own stream takes no kept append");
}

/// PAR-18 verification 5: nothing of animation reaches a late viewer.
#[test]
fn nothing_of_animation_reaches_a_late_viewer() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    channel.from_program(&picture("naplps", &json!("kept"), true, b"\x0e"), 1);
    let animate = json!({"type": "animate", "connections": "all", "page": "100", "sub": "0001", "ops": [{"action": "hide", "target": {"object": 1}}]});
    channel.from_program(&line(&animate), 2);
    let _ = channel.take_effects();
    let late = arrive(&mut channel, 2, BOTH, 3);
    let animate_type = neoteletext_rules::command::TYPE_ANIMATE;
    assert!(!records(&late).iter().any(|(kind, _)| *kind == animate_type), "no ANIMATE record");
    assert_eq!(picture_types(&late), [TYPE_NAPLPS]);
}

/// PAR-16: a kept NAPLPS picture's objects go with it, the last whole set
/// with each later `one` applied.
#[test]
fn kept_objects_reach_a_late_viewer_with_their_redefinitions() {
    let mut channel = empty();
    channel.from_program(&picture("naplps", &json!("kept"), true, b"\x0e"), 0);
    let object = |number: u16, shown: bool| {
        json!({"number": number, "kind": "region", "shown": shown, "detectable": true, "layer": 1, "order": 0, "x": 0, "y": 0, "width": 10, "height": 10,
               "origin_x": 0, "origin_y": 0, "frames": [], "cover": 0})
    };
    let objects =
        |list: Value, one: bool| line(&json!({"type": "naplps_objects", "connections": "kept", "page": "100", "sub": "0001", "objects": list, "one": one}));
    channel.from_program(&objects(json!([object(1, true), object(2, true)]), false), 1);
    channel.from_program(&objects(json!([object(2, false)]), true), 2);
    assert_eq!(refusals(&mut channel), [] as [(Value, Value); 0]);
    let late = arrive(&mut channel, 1, BOTH, 3);
    let read: Vec<_> = records(&late).into_iter().filter(|(kind, _)| *kind == TYPE_NAPLPS_OBJECTS).collect();
    assert_eq!(read.len(), 1);
    let record = neoteletext_web::records::naplps_objects::read(&read[0].1).unwrap();
    let shown: Vec<(u16, bool)> = record.objects().map(|object| (object.number, object.shown)).collect();
    assert_eq!(shown, [(1, true), (2, false)]);
}

/// PAR-19: `forget_picture` empties the picture of every connection that
/// took it, and is refused when nothing is kept.
#[test]
fn forget_picture_sends_an_empty_picture_and_is_refused_when_nothing_is_kept() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    let forget = line(&json!({"type": "forget_picture", "page": "100", "sub": "0001"}));
    channel.from_program(&forget, 1);
    let refused = refusals(&mut channel);
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0, "not_kept");
    channel.from_program(&picture("regis", &json!("kept"), true, b"P[1,1]"), 2);
    channel.from_program(&line(&json!({"type": "regis_cursor", "connections": "kept", "page": "100", "sub": "0001", "drawing": 3})), 3);
    let _ = channel.take_effects();
    channel.from_program(&forget, 4);
    let sent = sent_to(&channel.take_effects(), 1);
    assert_eq!(streams(&sent, TYPE_REGIS), [(2, 0, Vec::new())], "a new generation of no bytes");
    assert_eq!(picture_types(&sent), [TYPE_REGIS, TYPE_REGIS_CURSOR]);
    let late = arrive(&mut channel, 2, BOTH, 5);
    assert_eq!(picture_types(&late), [] as [u8; 0], "nothing is kept now");
}

/// PAR-16: a kept picture of one language refuses the other's bytes
/// without `restart`, and takes them with it.
#[test]
fn a_kept_picture_refuses_the_other_language_without_restart() {
    let mut channel = empty();
    channel.from_program(&picture("regis", &json!("kept"), true, b"P[1,1]"), 0);
    channel.from_program(&picture("naplps", &json!("kept"), false, b"\x0e"), 1);
    let refused = refusals(&mut channel);
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0, "wrong_picture_language");
    channel.from_program(&picture("naplps", &json!("kept"), true, b"\x0e"), 2);
    assert_eq!(refusals(&mut channel), [] as [(Value, Value); 0]);
    let late = arrive(&mut channel, 1, BOTH, 3);
    assert_eq!(picture_types(&late), [TYPE_NAPLPS]);
}

/// PAR-20: `naplps_links` and `naplps_cursor` are written as `DRAWING_LINKS`
/// and `REGIS_CURSOR`, read back by the engine's readers with the object's
/// number.
#[test]
fn naplps_links_and_cursor_are_the_drawing_links_and_cursor_records() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    channel
        .from_program(&line(&json!({"type": "naplps_links", "connections": [1], "page": "100", "sub": "0001", "links": [{"object": 7, "target": "200"}]})), 1);
    channel.from_program(&line(&json!({"type": "naplps_cursor", "connections": [1], "page": "100", "sub": "0001", "object": 7})), 2);
    let sent = sent_to(&channel.take_effects(), 1);
    let read = records(&sent);
    assert_eq!(read.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(), [TYPE_DRAWING_LINKS, TYPE_REGIS_CURSOR]);
    let links = neoteletext_web::records::drawing_links::read(&read[0].1).unwrap();
    let named: Vec<(u16, Vec<u8>)> = links.links().map(|link| (link.drawing, link.target.to_vec())).collect();
    assert_eq!(named, [(7, b"200".to_vec())]);
    let cursor = neoteletext_web::records::regis_cursor::read(&read[1].1).unwrap();
    assert_eq!(cursor.drawing, 7);
}

/// Renderer parity PAR-2, PAR-44: `naplps_links` and `naplps_cursor` for
/// the shared wire vectors' NAPLPS sub-page (101/0001, object 301 leading
/// to 302) are sent as the records of their `drawing_links` and
/// `regis_cursor` rows, body for body (the record id is the connection's
/// own, so the packets are compared by the records they carry).
#[test]
fn naplps_links_and_cursor_are_the_shared_wire_vectors() {
    let vector = |name: &str| {
        let row = neoteletext_web::wire_vectors::vectors().find(|row| row.kind == "records" && row.name == name).unwrap();
        let mut bytes = Vec::new();
        for packet in row.value.split(' ') {
            let (packet, length) = neoteletext_web::wire_vectors::bytes_of(packet).unwrap();
            bytes.extend_from_slice(&packet[..length]);
        }
        records(&bytes)
    };
    // The viewer shows 101/0001, the vectors' sub-page, as `arrive` has one
    // show 100/0001.
    let mut channel = empty();
    channel.opened(1, None, 0);
    let hello = Item::Event(Event::Hello { version: VERSION, caps: BOTH, agent: None });
    channel.received(1, &message(&channel, 1, &[hello, select(0x101, 1)]), 0);
    let page = with(&with(&subpage(), "/page", json!("101")), "/sub", json!("0001"));
    channel.from_program(&line(&json!({"type": "page", "connections": [1], "request": 1, "page": page})), 0);
    let _ = channel.take_effects();
    channel.from_program(
        &line(&json!({"type": "naplps_links", "connections": [1], "page": "101", "sub": "0001", "links": [{"object": 301, "target": "302"}]})),
        1,
    );
    channel.from_program(&line(&json!({"type": "naplps_cursor", "connections": [1], "page": "101", "sub": "0001", "object": 301})), 2);
    let sent = records(&sent_to(&channel.take_effects(), 1));
    assert_eq!(sent, [vector("drawing_links"), vector("regis_cursor")].concat());
}

/// The stream of `naplps`'s processing-limit test (the NAPLPS renderer's
/// `tests/validator.rs`): sixteen macros, each calling the next sixty
/// times, far past `MAX_PROCESSED_BYTES` (ANSI X3.110-1983 section 6.2:
/// DEF MACRO is C1 8/0, END 8/5; the macros into G2, then SS2 calls one).
fn past_the_processing_limit() -> Vec<u8> {
    const DEF_MACRO: u8 = 0x80;
    const END: u8 = 0x85;
    const SS2: u8 = 0x19;
    let mut bytes = alloc::vec![0x1B, 0x2E, 0x20, 0x7A];
    for number in 0x21..0x31_u8 {
        bytes.extend_from_slice(&[DEF_MACRO, number]);
        for _ in 0..60 {
            bytes.extend_from_slice(&[SS2, number.saturating_add(1)]);
        }
    }
    bytes.extend_from_slice(&[DEF_MACRO, 0x31, b'x', END, SS2, 0x21, b'a']);
    bytes
}

/// PAR-23 verification: a stream past `MAX_PROCESSED_BYTES` is refused
/// naming the limit, per connection and kept; a byte the reader passes over
/// is sent, and logged for the author; an object naming an undefined macro
/// is logged once.
#[test]
fn the_server_checks_naplps_as_the_receiver_reads_it() {
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    for audience in [json!([1]), json!("kept")] {
        channel.from_program(&picture("naplps", &audience, true, &past_the_processing_limit()), 1);
        let refused = refusals(&mut channel);
        assert_eq!(refused.len(), 1, "{audience}");
        assert_eq!(refused[0].0, "invalid_naplps");
        assert!(refused[0].1.as_str().unwrap().contains("bytes left unprocessed at the processing limit"), "{:?}", refused[0].1);
    }
    // 0/1 belongs to another layer (ANSI X3.110-1983 section 6.1).
    channel.from_program(&picture("naplps", &json!([1]), true, b"\x0e\x01"), 2);
    let sent = sent_to(&channel.take_effects(), 1);
    // Generation 2: the refused restart above took the first.
    assert_eq!(streams(&sent, TYPE_NAPLPS), [(2, 0, b"\x0e\x01".to_vec())], "passed over, not refused");
    assert_eq!(records(&sent).iter().filter(|(kind, _)| *kind == TYPE_LOG).count(), 1, "the author is told");
    let object = json!({"number": 1, "kind": "macro", "shown": true, "detectable": true, "layer": 1, "order": 0, "x": 0, "y": 0, "width": 10, "height": 10,
                        "origin_x": 0, "origin_y": 0, "frames": [0x40]});
    let objects = line(&json!({"type": "naplps_objects", "connections": [1], "page": "100", "sub": "0001", "objects": [object]}));
    channel.from_program(&objects, 3);
    channel.from_program(&objects, 4);
    let sent = sent_to(&channel.take_effects(), 1);
    assert_eq!(records(&sent).iter().filter(|(kind, _)| *kind == TYPE_LOG).count(), 1, "once a generation");
    assert_eq!(records(&sent).iter().filter(|(kind, _)| *kind == TYPE_NAPLPS_OBJECTS).count(), 2, "not refused");
}

/// PAR-23 with PAR-5: a text part's mark written while colour mode 2 is in
/// force is logged for the author, and the bytes are sent, not refused.
#[test]
fn a_mark_written_in_colour_mode_two_is_logged_and_sent() {
    // SO, then SELECT COLOR (3/14) with operands giving colour mode 2
    // (ANSI X3.110-1983 section 5.3.2.6.3), then SI back to the text set.
    const MODE_TWO: [u8; 5] = [0x0E, 0x3E, 0x70, 0x4C, 0x0F];
    let mut channel = empty();
    let _ = arrive(&mut channel, 1, BOTH, 0);
    let logged = |channel: &mut Channel, parts: Value| {
        let message = json!({"type": "naplps", "connections": [1], "page": "100", "sub": "0001", "restart": true, "parts": parts});
        channel.from_program(&line(&message), 1);
        let sent = sent_to(&channel.take_effects(), 1);
        assert_eq!(streams(&sent, TYPE_NAPLPS).len(), 1, "sent, not refused");
        records(&sent)
            .into_iter()
            .filter(|(kind, _)| *kind == TYPE_LOG)
            .map(|(_, body)| alloc::string::String::from_utf8_lossy(&body).into_owned())
            .collect::<Vec<_>>()
    };
    let in_mode_two = logged(&mut channel, json!([{"data": base64(&MODE_TWO)}, {"text": "\u{e9}", "language": null}]));
    assert_eq!(in_mode_two.len(), 1, "{in_mode_two:?}");
    assert!(in_mode_two[0].contains("colour mode 2"), "{in_mode_two:?}");
    let in_mode_zero = logged(&mut channel, json!([{"text": "\u{e9}", "language": null}]));
    assert!(in_mode_zero.is_empty(), "colour mode 0 keeps the letter: {in_mode_zero:?}");
}

/// PAR-24 verification: a connection is sent a language's records only
/// when it agreed that language's bit, kept or sent to it.
#[test]
fn picture_records_go_only_to_connections_that_agreed_their_language() {
    let mut channel = empty();
    let offers = [(1, CURSOR_CAP | SELECT_CAP), (2, CURSOR_CAP | SELECT_CAP | REGIS_CAP), (3, CURSOR_CAP | SELECT_CAP | NAPLPS_CAP)];
    for (connection, caps) in offers {
        let _ = arrive(&mut channel, connection, caps, 0);
    }
    for (kind, record) in [("regis", TYPE_REGIS), ("naplps", TYPE_NAPLPS)] {
        for audience in [json!("kept"), json!([1, 2, 3])] {
            channel.from_program(&picture(kind, &audience, true, if kind == "regis" { b"P[1,1]" } else { b"\x0e" }), 1);
            let effects = channel.take_effects();
            let got: Vec<bool> = [1, 2, 3].iter().map(|&connection| streams(&sent_to(&effects, connection), record).len() == 1).collect();
            let expected = if kind == "regis" { [false, true, false] } else { [false, false, true] };
            assert_eq!(got, expected, "{kind} to {audience}");
        }
    }
    channel.from_program(&line(&json!({"type": "naplps_cursor", "connections": "all", "page": "100", "sub": "0001", "object": 1})), 2);
    let effects = channel.take_effects();
    let got: Vec<bool> = [1, 2, 3].iter().map(|&connection| !picture_types(&sent_to(&effects, connection)).is_empty()).collect();
    assert_eq!(got, [false, false, true]);
}

/// TODO-6836, TODO-6895: every connection's own stream holds a reader,
/// counted at what its renderer says it holds; the readers of a channel's
/// connections, with the font and sets they share counted once, stop at
/// `MOST_CONNECTION_READER_BYTES`, and a restart past it is refused naming
/// it, while a restart that replaces readers already held is not. Streams
/// of thousands of numbered drawings reach it within some hundreds of
/// connections; ordinary pages' do not before the channel's every
/// connection (the next test).
#[test]
fn the_readers_of_connections_own_streams_stop_at_their_bound() {
    use crate::kept::{Checker, MOST_CONNECTION_READER_BYTES, PictureLanguage};
    let fonts = crate::kept::tests::fonts();
    let heavy = crate::kept::tests::regis_drawings(8_000);
    let mut checker = Checker::new(PictureLanguage::Regis, fonts);
    checker.feed(&heavy).unwrap();
    let held = checker.held_bytes();
    let room = u64::try_from((MOST_CONNECTION_READER_BYTES - fonts.held_bytes()) / held).unwrap();
    assert!(room > 15 && room < 1_000, "{room} connections' readers of {held} bytes fit");
    let mut channel = empty();
    for connection in 1..=room + 1 {
        let _ = arrive(&mut channel, connection, BOTH, 0);
    }
    for connection in 1..=room {
        channel.from_program(&picture("regis", &json!([connection]), true, &heavy), 1);
        assert_eq!(refusals(&mut channel), [], "connection {connection}'s reader fits");
    }
    assert_eq!(channel.connection_readers_held(), channel.connection_readers_counted(), "the running total is every reader's");
    channel.from_program(&picture("regis", &json!([room + 1]), true, &heavy), 2);
    let refused = refusals(&mut channel);
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0, "kept_over_limit");
    assert!(refused[0].1.as_str().unwrap().starts_with("MOST_CONNECTION_READER_BYTES (1073741824) would be passed"), "{refused:?}");
    assert!(sent_to(&channel.take_effects(), room + 1).is_empty(), "nothing was sent to it");
    // Begun again, the readers held are replaced, not added to.
    let all: Vec<u64> = (1..=room).collect();
    channel.from_program(&picture("regis", &json!(all), true, b"P[1,1]"), 3);
    assert_eq!(refusals(&mut channel), []);
    assert_eq!(channel.connection_readers_held(), channel.connection_readers_counted(), "the running total is every reader's");
    // Gone, a connection's readers leave the total.
    for connection in 1..=room {
        channel.closed(connection, 4);
    }
    assert_eq!(channel.connection_readers_held(), fonts.held_bytes(), "only the shared font and sets are left");
    assert_eq!(channel.connection_readers_counted(), channel.connection_readers_held());
}

/// The NAPLPS page of text and the ReGIS page the scale test sends: a
/// page of the demonstration channel (`gallery`, 497 bytes: numbered
/// drawings, placed copies and a clip), whose reader is the median of the
/// demonstration pages'.
const ORDINARY_REGIS: &[u8] = b"S(E)W(I7)P[20,30]T(S2)'ReGIS v2: numbered drawings'D(B3)W(I3)F(V[0,0][59,0][59,59][0,59])W(I0)P[22,20]T'3'D(E)D(S3(V0))D(B1)W(I4)F(V[0,0][799,0][799,399][0,399])D(E)D(S1(L-10,Y80,C(X20,Y20,W760,H360)))D(S10(R3,X100,Y150),S11(R3,X200,Y150),S12(R3,X300,Y150))D(B20)W(I7)T(S1)'Three placed copies of one hidden master'D(E)D(S20(X100,Y240))D(S30(X500,Y150,C(X0,Y0,W120,H60)))D(B31)W(I2)C[+50]D(E)D(S31(P30,X60,Y60))D(B40)W(I7)T(S1)\"A circle cut by its parent's clip rectangle\"D(E)D(S40(X380,Y270))D(C)";
/// A NAPLPS page of text, as a program writes one.
const ORDINARY_NAPLPS: &[u8] = b"Hello, a page of text in the default font";

/// TODO-6895: the readers' bound holds `CONNECTIONS_WITH_OWN_PICTURES`
/// connections' readers of ordinary pages, of either language, with room.
#[test]
fn the_bound_holds_fifty_thousand_connections_readers_of_ordinary_pages() {
    use crate::kept::{CONNECTIONS_WITH_OWN_PICTURES, Checker, MOST_CONNECTION_READER_BYTES, ORDINARY_READER_BYTES, PictureLanguage};
    let fonts = crate::kept::tests::fonts();
    let read = |language: PictureLanguage, bytes: &[u8]| {
        let mut checker = Checker::new(language, fonts);
        checker.feed(bytes).unwrap();
        checker.held_bytes()
    };
    let (regis, naplps) = (read(PictureLanguage::Regis, ORDINARY_REGIS), read(PictureLanguage::Naplps, ORDINARY_NAPLPS));
    std::println!("an ordinary ReGIS page's reader holds {regis} bytes, a NAPLPS page of text's {naplps}");
    assert!(regis <= ORDINARY_READER_BYTES, "{regis}");
    assert!(naplps <= ORDINARY_READER_BYTES / 8, "{naplps}");
    assert!(fonts.held_bytes() + CONNECTIONS_WITH_OWN_PICTURES * regis <= MOST_CONNECTION_READER_BYTES);
}

/// TODO-6895, run by hand (`cargo test --release -p teletext-runtime --
/// --ignored fifty_thousand`): 50,000 connections of one channel each take
/// their own stream of an ordinary page, of NAPLPS and then of ReGIS, and
/// none is refused; the running total of their readers is every reader's.
/// It prints the time, which the per-message count over every connection
/// made grow with the square of the connections.
#[test]
#[ignore = "opens 50,000 connections; run in a release build by hand, as REQ-1023's verification names"]
fn fifty_thousand_connections_each_take_their_own_ordinary_pictures() {
    use crate::kept::CONNECTIONS_WITH_OWN_PICTURES;
    let connections = u64::try_from(CONNECTIONS_WITH_OWN_PICTURES).unwrap();
    let mut channel = empty();
    let started = std::time::Instant::now();
    for connection in 1..=connections {
        let _ = arrive(&mut channel, connection, BOTH, 0);
    }
    let opened = started.elapsed();
    for connection in 1..=connections {
        channel.from_program(&picture("naplps", &json!([connection]), true, ORDINARY_NAPLPS), 1);
        channel.from_program(&picture("regis", &json!([connection]), true, ORDINARY_REGIS), 1);
        if connection % 1_000 == 0 {
            assert_eq!(refusals(&mut channel), [], "up to connection {connection}");
        }
    }
    assert_eq!(refusals(&mut channel), []);
    let held = channel.connection_readers_held();
    assert_eq!(held, channel.connection_readers_counted());
    std::println!(
        "{connections} connections opened in {opened:?}; their own NAPLPS and ReGIS streams read in {:?}; the readers hold {held} bytes",
        started.elapsed().checked_sub(opened).unwrap()
    );
}

/// TODO-6860, DEC-6858: a connection's own NAPLPS stream's reader is
/// counted at what it holds after every message it reads: streams that
/// define every DRCS character at the largest mask reach the bound within
/// a few connections, and the append that would pass it is refused naming
/// it and sent to none.
#[test]
fn connections_own_naplps_readers_are_counted_at_what_their_streams_define() {
    use crate::kept::{Checker, MOST_CONNECTION_READER_BYTES, PictureLanguage};
    let fonts = crate::kept::tests::fonts();
    let small = {
        let mut checker = Checker::new(PictureLanguage::Naplps, fonts);
        checker.feed(b"Hello").unwrap();
        checker.held_bytes()
    };
    let heavy = crate::kept::tests::heaviest_naplps();
    let mut checker = Checker::new(PictureLanguage::Naplps, fonts);
    checker.feed(b"Hello").unwrap();
    checker.feed(&heavy).unwrap();
    let held = checker.held_bytes();
    // Two connections keep their small streams beside the heavy ones.
    let room = u64::try_from((MOST_CONNECTION_READER_BYTES - fonts.held_bytes() - 2 * small) / held).unwrap();
    assert!(room > 1 && room < 1_000, "{held} bytes a reader, {room} fit");
    let mut channel = empty();
    for connection in 1..=room + 2 {
        let _ = arrive(&mut channel, connection, BOTH, 0);
        channel.from_program(&picture("naplps", &json!([connection]), true, b"Hello"), 1);
        assert_eq!(refusals(&mut channel), [], "connection {connection}'s small stream fits");
    }
    let _ = channel.take_effects();
    for connection in 1..=room {
        channel.from_program(&picture("naplps", &json!([connection]), false, &heavy), 2);
        assert_eq!(refusals(&mut channel), [], "connection {connection}'s definitions fit");
    }
    channel.from_program(&picture("naplps", &json!([room + 1]), false, &heavy), 3);
    let effects = channel.take_effects();
    let refused: Vec<(Value, Value)> =
        told(&effects).into_iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["detail"].clone())).collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].0, "kept_over_limit");
    assert!(refused[0].1.as_str().unwrap().starts_with("MOST_CONNECTION_READER_BYTES (1073741824) would be passed"), "{refused:?}");
    assert!(sent_to(&effects, room + 1).is_empty(), "nothing was sent to it");
}

/// TODO-6845, DEC-6850: a connection's own ReGIS stream's reader is
/// counted at what its renderer says it holds, after every message it
/// reads, not at a fixed 131,072 bytes when the stream begins: an ordinary
/// page's reader is some 20 KB, so over a thousand connections' streams
/// fit, while streams that define thousands of numbered drawings reach the
/// bound within a few connections, and the append that would pass it is
/// refused naming it, sent to none, and its stream begun again.
#[test]
fn connections_own_regis_readers_are_counted_at_what_their_streams_define() {
    use crate::kept::{Checker, MOST_CONNECTION_READER_BYTES, PictureLanguage};
    let fonts = crate::kept::tests::fonts();
    let fresh = Checker::new(PictureLanguage::Regis, fonts).held_bytes();
    assert!(MOST_CONNECTION_READER_BYTES / fresh > 50_000, "a new reader holds {fresh} bytes");
    let heavy = crate::kept::tests::regis_drawings(8_000);
    let mut checker = Checker::new(PictureLanguage::Regis, fonts);
    checker.feed(b"P[1,1]").unwrap();
    checker.feed(&heavy).unwrap();
    let held = checker.held_bytes();
    // The small streams of the two connections beyond them stay too.
    let room = u64::try_from((MOST_CONNECTION_READER_BYTES - fonts.held_bytes() - 2 * fresh) / held).unwrap();
    assert!(room > 1 && room < 1_000, "{held} bytes a reader, {room} fit");
    let mut channel = empty();
    for connection in 1..=room + 2 {
        let _ = arrive(&mut channel, connection, BOTH, 0);
        channel.from_program(&picture("regis", &json!([connection]), true, b"P[1,1]"), 1);
        assert_eq!(refusals(&mut channel), [], "connection {connection}'s small stream fits");
    }
    let _ = channel.take_effects();
    for connection in 1..=room {
        channel.from_program(&picture("regis", &json!([connection]), false, &heavy), 2);
        assert_eq!(refusals(&mut channel), [], "connection {connection}'s drawings fit");
    }
    channel.from_program(&picture("regis", &json!([room + 1]), false, &heavy), 3);
    let effects = channel.take_effects();
    let refused: Vec<(Value, Value)> =
        told(&effects).into_iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["detail"].clone())).collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].0, "kept_over_limit");
    assert!(refused[0].1.as_str().unwrap().starts_with("MOST_CONNECTION_READER_BYTES (1073741824) would be passed"), "{refused:?}");
    assert!(sent_to(&effects, room + 1).is_empty(), "nothing was sent to it");
    // Its stream was begun again, so an append without restart is passed
    // over, and one with restart starts afresh.
    channel.from_program(&picture("regis", &json!([room + 1]), false, b"V[2,2]"), 4);
    assert_eq!(refusals(&mut channel), []);
    channel.from_program(&picture("regis", &json!([room + 1]), true, b"P[1,1]"), 5);
    assert_eq!(refusals(&mut channel), []);
}
