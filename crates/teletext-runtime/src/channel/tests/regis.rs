//! A program's ReGIS as a viewer receives it (DEC-6335): read by the
//! receiver's own renderer before it is sent, carried in REGIS records,
//! and PICK passed to the program.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use neoteletext_input::event::{Event, Item};
use serde_json::{Value, json};

use super::{CONNECTION, Channel, Effect, ON, answered, inputs, line, message, record_types, sent_to, told};
use crate::protocol::check::TYPE_REGIS;
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
    channel.received(CONNECTION, &message(&[Item::Event(Event::Extension { kind: 0x46, payload: &on })]), 5);
    let effects = channel.take_effects();
    let picks = inputs(&effects);
    assert_eq!(picks.len(), 1);
    assert_eq!(picks[0]["page"], json!("100"));
    assert_eq!(picks[0]["event"], json!({"kind": "pick", "drawing": 12, "x": 20, "y": -6, "picture_x": 412, "picture_y": 180, "commits": 7}));
    // Page 101 is not on screen.
    let elsewhere = payload(1);
    channel.received(CONNECTION, &message(&[Item::Event(Event::Extension { kind: 0x46, payload: &elsewhere })]), 6);
    assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0]);
}
