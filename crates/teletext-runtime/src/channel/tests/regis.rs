//! A program's ReGIS as a viewer receives it (DEC-6335): read by the
//! receiver's own renderer before it is sent, carried in REGIS records,
//! and PICK passed to the program.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use serde_json::{Value, json};

use super::{CONNECTION, Channel, Effect, ON, answered, inputs, line, record_types, sent_to, told};
use crate::protocol::check::{TYPE_DRAWING_LINKS, TYPE_REGIS, TYPE_REGIS_CURSOR};
use crate::protocol::tests::base64;
use crate::records::REGIS_DATA;

/// A `regis` message for the test's connection and page 100, sub-code 0000.
fn regis(data: &[u8], restart: bool) -> Vec<u8> {
    line(&json!({"type": "regis", "connections": [CONNECTION], "page": "100", "sub": "0000", "restart": restart, "data": base64(data)}))
}

/// No bytes sent.
const NOTHING: [u8; 0] = [];

fn refusals(effects: &[Effect]) -> Vec<(Value, String)> {
    told(effects).iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["detail"].as_str().unwrap().into())).collect()
}

/// The REGIS records in `bytes`, each as (generation, offset, data),
/// reassembled from its fragments and its frame taken off.
fn records(bytes: &[u8]) -> Vec<(u16, u32, Vec<u8>)> {
    let mut found = Vec::new();
    let mut record: Vec<u8> = Vec::new();
    for bytes in bytes.as_chunks::<PACKET_BYTES>().0 {
        let fragment = neoteletext_audio::idl::fragment(&Packet { bytes: *bytes }).unwrap().unwrap();
        if fragment.index == 0 {
            record.clear();
        }
        record.extend_from_slice(&fragment.data);
        if fragment.last {
            let (kind, body) = neoteletext_audio::record::split(&record[..neoteletext_audio::record::framed_length(&record).unwrap()]).unwrap();
            assert_eq!(kind, TYPE_REGIS);
            assert_eq!(&body[..4], &[0x01, 0x00, 0x00, 0x00], "page 100, sub-code 0000");
            let generation = u16::from_be_bytes([body[4], body[5]]);
            let offset = u32::from_be_bytes([body[6], body[7], body[8], body[9]]);
            found.push((generation, offset, body[10..].to_vec()));
        }
    }
    found
}

/// DEC-6335: a sub-page's ReGIS reaches the viewer who shows it as REGIS
/// records, a stream started by `restart` and added to after, each record
/// beginning where the last ended.
#[test]
fn regis_reaches_the_viewer_as_regis_records_in_one_stream() {
    let mut channel = answered();
    let first = b"P[10,10]V[30,10]D(B1)V[5,5]D(E)";
    channel.from_program(&regis(first, true), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [(Value, String); 0]);
    let sent = sent_to(&effects, CONNECTION);
    assert_eq!(record_types(&sent), [TYPE_REGIS]);
    assert_eq!(records(&sent), [(1, 0, first.to_vec())]);

    channel.from_program(&regis(b"D(S1(X5),C)", false), 2);
    let more = records(&sent_to(&channel.take_effects(), CONNECTION));
    assert_eq!(more, [(1, u32::try_from(first.len()).unwrap(), b"D(S1(X5),C)".to_vec())]);

    // Starting again is a new generation from offset 0.
    channel.from_program(&regis(b"V[1,1]", true), 3);
    assert_eq!(records(&sent_to(&channel.take_effects(), CONNECTION)), [(2, 0, b"V[1,1]".to_vec())]);
}

/// More ReGIS than one record carries goes in as many as it needs.
#[test]
fn long_regis_goes_in_as_many_records_as_it_needs() {
    let mut channel = answered();
    let long = alloc::vec![b' '; REGIS_DATA + 100];
    channel.from_program(&regis(&long, true), 1);
    let sent = records(&sent_to(&channel.take_effects(), CONNECTION));
    assert_eq!(sent.iter().map(|(_, offset, data)| (*offset, data.len())).collect::<Vec<_>>(), [(0, REGIS_DATA), (u32::try_from(REGIS_DATA).unwrap(), 100)]);
}

/// The renderer's REQ-6332: a commit the receiver would refuse is refused
/// here, with its code, and nothing of the message is sent; the stream is
/// then started again before more is taken.
#[test]
fn a_commit_the_receiver_would_refuse_is_refused_and_nothing_is_sent() {
    let mut channel = answered();
    // Drawing 2 names a parent that does not exist: failure code 5.
    channel.from_program(&regis(b"V[1,1]D(S2(P9),C)", true), 1);
    let effects = channel.take_effects();
    let refused = refusals(&effects);
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0, json!("invalid_regis"));
    assert!(refused[0].1.contains("code 5"), "{}", refused[0].1);
    assert_eq!(sent_to(&effects, CONNECTION), NOTHING);
    // Its stream is gone: more without a restart goes nowhere.
    channel.from_program(&regis(b"V[2,2]", false), 2);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), NOTHING);
    channel.from_program(&regis(b"V[2,2]", true), 3);
    assert_eq!(records(&sent_to(&channel.take_effects(), CONNECTION)).len(), 1);
}

/// ReGIS goes only to a connection whose selection shows the sub-page, and
/// only once a stream of it has been started for that connection.
#[test]
fn regis_goes_only_where_the_sub_page_is_shown_and_started() {
    let mut channel = answered();
    channel.from_program(&regis(b"V[1,1]", false), 1);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), NOTHING, "no stream started");
    let other = line(&json!({"type": "regis", "connections": [CONNECTION], "page": "200", "sub": "0000", "restart": true, "data": base64(b"V[1,1]")}));
    channel.from_program(&other, 2);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), NOTHING, "another page than the one shown");
    let any = line(&json!({"type": "regis", "connections": [CONNECTION], "page": "100", "sub": "3F7F", "restart": true, "data": base64(b"V[1,1]")}));
    channel.from_program(&any, 3);
    assert_eq!(refusals(&channel.take_effects()).len(), 1, "3F7F names no sub-page");
}

/// A REGIS record cannot be sent as raw packets, past the check.
#[test]
fn a_regis_record_in_raw_packets_is_refused() {
    let mut channel = answered();
    channel.from_program(&regis(b"V[1,1]", true), 1);
    let record = sent_to(&channel.take_effects(), CONNECTION);
    channel.from_program(&line(&json!({"type": "packets", "connections": [CONNECTION], "packets": base64(&record)})), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects).iter().map(|(rule, _)| rule.clone()).collect::<Vec<_>>(), [json!("invalid_packet")]);
    assert_eq!(sent_to(&effects, CONNECTION), NOTHING);
}

/// PICK (the engine's REQ-6337): the drawing under the pointer reaches the
/// program as an input of kind `pick`, with the page and sub-code it was
/// stamped with; one stamped with another page is dropped as any input is.
#[test]
fn pick_reaches_the_program() {
    // Drawing 12 at (20, -6) within it, (412, 180) in the picture, after 7 commits.
    let payload = |page_low: u8| [0x80 | page_low, 0x02, 0, 12, 40, 11, 0x9C, 0x03, 0xB4, 0x01, 0, 0, 0, 7];
    let mut channel: Channel = answered();
    let on = payload(0);
    assert_eq!((ON.page, ON.sub), (0x100, 0));
    channel.received(CONNECTION, &extension(0x46, &on), 5);
    let effects = channel.take_effects();
    let picks = inputs(&effects);
    assert_eq!(picks.len(), 1);
    assert_eq!(picks[0]["page"], json!("100"));
    assert_eq!(picks[0]["event"], json!({"kind": "pick", "drawing": 12, "x": 20, "y": -6, "picture_x": 412, "picture_y": 180, "commits": 7}));
    // Page 101 is not on screen.
    let elsewhere = payload(1);
    channel.received(CONNECTION, &extension(0x46, &elsewhere), 6);
    assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0]);
}

/// An extension event of `kind` carrying `payload` as the receiver writes
/// it: `0xFF`, the type, the length and the payload (DEC-1444), so that a
/// payload the engine's reader refuses can be sent.
fn extension(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = alloc::vec![neoteletext_input::event::EVENT, kind, u8::try_from(payload.len()).unwrap()];
    bytes.extend_from_slice(payload);
    bytes
}

/// The records in `bytes`, each as its type and body, reassembled from
/// its fragments and its frame taken off.
fn bodies(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut found = Vec::new();
    let mut record: Vec<u8> = Vec::new();
    for bytes in bytes.as_chunks::<PACKET_BYTES>().0 {
        let fragment = neoteletext_audio::idl::fragment(&Packet { bytes: *bytes }).unwrap().unwrap();
        if fragment.index == 0 {
            record.clear();
        }
        record.extend_from_slice(&fragment.data);
        if fragment.last {
            let (kind, body) = neoteletext_audio::record::split(&record[..neoteletext_audio::record::framed_length(&record).unwrap()]).unwrap();
            found.push((kind, body.to_vec()));
        }
    }
    found
}

fn regis_links(page: &str, links: &Value) -> Vec<u8> {
    line(&json!({"type": "regis_links", "connections": [CONNECTION], "page": page, "sub": "0000", "links": links}))
}

/// REQ-6372: a sub-page's drawing links reach the viewer who shows it as a
/// `DRAWING_LINKS` record (the engine's REQ-6370), byte for byte; a viewer
/// showing another page gets none; an empty list is a record of no links.
#[test]
fn drawing_links_reach_the_viewer_who_shows_the_sub_page() {
    let mut channel = answered();
    channel.from_program(&regis_links("100", &json!([{"drawing": 12, "target": "200"}, {"drawing": 13, "target": "web:https://example.org/"}])), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [(Value, String); 0]);
    let mut expected = alloc::vec![0x01, 0x00, 0x00, 0x00, 0, 1, 2, 0, 12, 0, 3, b'2', b'0', b'0', 0, 13, 0, 24];
    expected.extend_from_slice(b"web:https://example.org/");
    assert_eq!(bodies(&sent_to(&effects, CONNECTION)), [(TYPE_DRAWING_LINKS, expected)]);

    channel.from_program(&regis_links("200", &json!([{"drawing": 12, "target": "300"}])), 2);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), NOTHING, "another page than the one shown");

    channel.from_program(&regis_links("100", &json!([])), 3);
    assert_eq!(bodies(&sent_to(&channel.take_effects(), CONNECTION)), [(TYPE_DRAWING_LINKS, alloc::vec![0x01, 0x00, 0x00, 0x00, 0, 1, 0])]);
}

/// Links too long for one record go in as many as they need, none split,
/// each naming how many there are.
#[test]
fn long_drawing_links_go_in_several_records() {
    let mut channel = answered();
    let target = alloc::format!("web:https://example.org/{}", "a".repeat(3000));
    let links: Vec<Value> = (1..=5).map(|drawing| json!({"drawing": drawing, "target": target})).collect();
    channel.from_program(&regis_links("100", &json!(links)), 1);
    let sent = bodies(&sent_to(&channel.take_effects(), CONNECTION));
    assert_eq!(
        sent.iter().map(|(kind, body)| (*kind, body[4], body[5], body[6])).collect::<Vec<_>>(),
        [(TYPE_DRAWING_LINKS, 0, 3, 2), (TYPE_DRAWING_LINKS, 1, 3, 2), (TYPE_DRAWING_LINKS, 2, 3, 1)]
    );
}

/// What a receiver would discard is refused here, with where the fault is.
#[test]
fn drawing_links_a_receiver_would_refuse_are_refused() {
    let many: Vec<Value> = (1..=65).map(|drawing| json!({"drawing": drawing, "target": "200"})).collect();
    for (links, path) in [
        (json!(many), "links"),
        (json!([{"drawing": 0, "target": "200"}]), "links[0].drawing"),
        (json!([{"drawing": 7, "target": "200"}, {"drawing": 7, "target": "300"}]), "links[1].drawing"),
        (json!([{"drawing": 7, "target": ""}]), "links[0].target"),
        (json!([{"drawing": 7, "target": "2 00"}]), "links[0].target"),
        (json!([{"drawing": 7, "target": "zzz"}]), "links[0].target"),
    ] {
        let mut channel = answered();
        channel.from_program(&regis_links("100", &links), 1);
        let effects = channel.take_effects();
        let refused: Vec<(Value, Value)> =
            told(&effects).iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["path"].clone())).collect();
        assert_eq!(refused, [(json!("invalid_drawing_links"), json!(path))], "{links}");
        assert_eq!(sent_to(&effects, CONNECTION), NOTHING);
    }
}

/// REQ-6372: a sub-page's cursor drawing reaches the viewer as a
/// `REGIS_CURSOR` record (the engine's REQ-6371): page, sub-code, drawing.
#[test]
fn the_cursor_drawing_reaches_the_viewer() {
    let mut channel = answered();
    let cursor = |drawing: u16| line(&json!({"type": "regis_cursor", "connections": [CONNECTION], "page": "100", "sub": "0000", "drawing": drawing}));
    channel.from_program(&cursor(40), 1);
    assert_eq!(bodies(&sent_to(&channel.take_effects(), CONNECTION)), [(TYPE_REGIS_CURSOR, alloc::vec![0x01, 0x00, 0x00, 0x00, 0, 40])]);
    channel.from_program(&cursor(0), 2);
    assert_eq!(bodies(&sent_to(&channel.take_effects(), CONNECTION)), [(TYPE_REGIS_CURSOR, alloc::vec![0x01, 0x00, 0x00, 0x00, 0, 0])]);
}

/// Neither record can be sent as raw packets, past its check.
#[test]
fn drawing_links_and_cursor_records_in_raw_packets_are_refused() {
    let mut channel = answered();
    channel.from_program(&regis_links("100", &json!([{"drawing": 12, "target": "200"}])), 1);
    channel.from_program(&line(&json!({"type": "regis_cursor", "connections": [CONNECTION], "page": "100", "sub": "0000", "drawing": 4})), 1);
    let sent = sent_to(&channel.take_effects(), CONNECTION);
    for record in sent.chunks(PACKET_BYTES) {
        channel.from_program(&line(&json!({"type": "packets", "connections": [CONNECTION], "packets": base64(record)})), 2);
        let effects = channel.take_effects();
        assert_eq!(refusals(&effects).iter().map(|(rule, _)| rule.clone()).collect::<Vec<_>>(), [json!("invalid_packet")]);
        assert_eq!(sent_to(&effects, CONNECTION), NOTHING);
    }
}

/// `DRAWING_HOVER` (the engine's REQ-6369): the pointer coming onto a
/// drawing and going off it reach the program as inputs of kind
/// `drawing_hover`; one stamped with another page, and one the engine's
/// reader would refuse, are dropped.
#[test]
fn drawing_hover_reaches_the_program() {
    // Page 100, sub-code 0000; the flags; drawing 12; 7 commits.
    let payload = |page_low: u8, flags: u8, drawing: u8| [0x80 | page_low, 0x02, 0, flags, drawing, 0, 0, 0, 7];
    let mut channel: Channel = answered();
    for (flags, leave) in [(0, false), (1, true)] {
        let on = payload(0, flags, 12);
        channel.received(CONNECTION, &extension(0x47, &on), 5);
        let hovers = inputs(&channel.take_effects());
        assert_eq!(hovers.len(), 1);
        assert_eq!(hovers[0]["event"], json!({"kind": "drawing_hover", "drawing": 12, "leave": leave, "commits": 7}));
    }
    for bad in [payload(1, 0, 12), payload(0, 2, 12), payload(0, 0, 0)] {
        channel.received(CONNECTION, &extension(0x47, &bad), 6);
        assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0], "{bad:?}");
    }
}

/// The renderer's REQ-6367: looks that lead back to their drawing are a
/// ring, refused by the validator as a receiver would refuse them.
#[test]
fn looks_that_form_a_ring_are_refused() {
    let mut channel = answered();
    channel.from_program(&regis(b"D(S1(H2),S2(A1),C)", true), 1);
    let refused = refusals(&channel.take_effects());
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0, json!("invalid_regis"));
    assert!(refused[0].1.contains("code 4"), "{}", refused[0].1);
}
