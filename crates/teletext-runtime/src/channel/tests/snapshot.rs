//! Snapshot connections (REQ-6881, REQ-6882, REQ-6900; neoteletext-common
//! `docs/tab-thumbnails.md` sections 5 and 6): held apart from viewers,
//! answered with the page the program draws for a stranger or its public
//! description, limited, and closed. Each test fails on the code before
//! them, which took a snapshot connection for a viewer's.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::net::IpAddr;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use neoteletext_input::VERSION;
use neoteletext_input::caps::{AGENT, SELECT_CAP, SNAPSHOT_CAP, SNAPSHOT_OFFER};
use neoteletext_input::control::{Control, FRAGMENT_DATA, from_packet};
use neoteletext_input::event::{Event, Item};
use neoteletext_web::consts::{CLOSE_BROKE_PROTOCOL, CLOSE_NORMAL};
use neoteletext_web::records::TYPE_NAPLPS_OBJECTS;
use serde_json::{Value, json};

use super::{ACKNOWLEDGEMENT_WAIT_MS, ANY, Channel, ChannelConfig, DefaultRules, Effect, FIRST_SELECTION_WAIT_MS, InitialPage, all_sent_to, bare, denied_in};
use super::{hello, key, line, message, record_types, select_with, subpage, told, vector_bytes, with};
use crate::descriptions::DESCRIPTION_WAIT_MS;
use crate::protocol::check::{TYPE_DRAWING_LINKS, TYPE_NAPLPS, TYPE_REGIS, TYPE_REGIS_CURSOR};
use crate::protocol::tests::base64;
use crate::snapshot::{
    MOST_OPEN_SNAPSHOTS, MOST_OPEN_SNAPSHOTS_PER_ADDRESS, MOST_SNAPSHOTS_AWAITED, MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE, SNAPSHOT_ANSWER_MS, SNAPSHOT_CACHE_MS,
    SNAPSHOT_CONNECTION_MS, SNAPSHOT_WINDOW_MS, SNAPSHOTS_BURST,
};
use crate::viewer::MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR;

/// The first snapshot connection of a test.
const SNAPSHOT: u64 = 50;
/// An address snapshots come from.
const FROM: [u8; 4] = [192, 0, 2, 7];

/// A channel whose program runs, with no connection.
fn running() -> Channel {
    let config = ChannelConfig {
        name: "Test".into(),
        address: "http://127.0.0.1/test/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        default_rules: DefaultRules::SendAll,
    };
    let mut channel = Channel::new(config, &crate::viewer::testing::secret(), crate::viewer::testing::random());
    channel.program_started(0);
    let _ = channel.take_effects();
    channel
}

/// The engine's snapshot request (REQ-6879): HELLO offering
/// `SNAPSHOT_OFFER` and SELECT of `page`, `sub`, as one input message.
fn request(page: u16, sub: u16) -> Vec<u8> {
    let mut bytes = [0_u8; 64];
    let length = neoteletext_input::snapshot::write_request(page, sub, &mut bytes).unwrap();
    bytes[..length].to_vec()
}

/// Opens `connection` from `address` at `now` and sends it the snapshot
/// request for `page`, `sub`.
fn ask(channel: &mut Channel, connection: u64, address: [u8; 4], (page, sub): (u16, u16), now: u64) {
    channel.opened(connection, Some(IpAddr::from(address)), now);
    channel.received(connection, &request(page, sub), now);
}

/// The `snapshot` lines among `effects`.
fn snapshots_told(effects: &[Effect]) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == "snapshot").collect()
}

/// The closes among `effects` of `connection`.
fn closes(effects: &[Effect], connection: u64) -> Vec<u16> {
    effects.iter().filter_map(|effect| if let Effect::Close { connection: to, code } = effect { (*to == connection).then_some(*code) } else { None }).collect()
}

/// What `effects` sent `connection` after its WELCOME, if they sent it
/// one, and the WELCOME's capabilities (0 when they sent none).
fn after_welcome(effects: &[Effect], connection: u64) -> (u32, Vec<u8>) {
    let bytes = all_sent_to(effects, connection);
    let Some(first) = bytes.first_chunk::<PACKET_BYTES>() else { return (0, bytes) };
    let mut buffer = [0; FRAGMENT_DATA];
    match from_packet(&Packet { bytes: *first }, &mut buffer) {
        Some(Ok(Control::Welcome { caps, .. })) => (caps, bytes[PACKET_BYTES..].to_vec()),
        _ => (0, bytes),
    }
}

/// The program's `snapshot_page` for snapshot `number` of page 100's
/// sub-page 0000, with `picture` if not null.
fn answer(number: u64, picture: &Value) -> Vec<u8> {
    let mut message = json!({"type": "snapshot_page", "snapshot": number, "page": subpage()});
    if !picture.is_null() {
        message["picture"] = picture.clone();
    }
    line(&message)
}

fn declined(number: u64) -> Vec<u8> {
    line(&json!({"type": "snapshot_declined", "snapshot": number}))
}

/// The refusals among `effects`: rule and detail.
fn refusals(effects: &[Effect]) -> Vec<(Value, String)> {
    told(effects).iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["detail"].as_str().unwrap().into())).collect()
}

/// The packets page 100's sub-page 0000 is, as a first selection's
/// sub-page is sent.
fn page_packets(page: &Value) -> Vec<u8> {
    let read: crate::protocol::Page = serde_json::from_value(page.clone()).unwrap();
    crate::page::encode(&read).packets.concat()
}

/// A public description of page 100 whose sub-page is `content`.
fn public(content: &Value) -> Vec<u8> {
    line(&json!({"type": "description", "page": "100", "public": true, "title": "Front", "text": "The front page", "content": content}))
}

/// REQ-6881 items 1 and 2, REQ-6900 item 1: WELCOME agrees exactly what a
/// snapshot uses, no challenge follows, the program hears `snapshot` and
/// nothing else, and is never told `connected`, however long the
/// connection waits.
#[test]
fn a_snapshot_is_welcomed_without_a_challenge_and_the_program_hears_only_snapshot() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let effects = channel.take_effects();
    let (caps, rest) = after_welcome(&effects, SNAPSHOT);
    assert_eq!(caps, SNAPSHOT_OFFER, "SNAPSHOT, SELECT, REGIS and NAPLPS as offered");
    assert!(rest.is_empty(), "no STATE_CHALLENGE or anything else: {:?}", record_types(&rest));
    assert_eq!(told(&effects), [json!({"type": "snapshot", "snapshot": 1, "page": "100", "sub": "3F7F", "path": "100"})]);
    channel.deadline(FIRST_SELECTION_WAIT_MS);
    channel.deadline(ACKNOWLEDGEMENT_WAIT_MS + FIRST_SELECTION_WAIT_MS);
    assert!(told(&channel.take_effects()).iter().all(|line| line["type"] != "connected"), "never connected");
    assert_eq!((channel.viewers(), channel.stats().viewers, channel.stats().viewers_total, channel.stats().snapshots), (0, 0, 0, 1));
}

/// The wire vectors `hello_snapshot` and `welcome_snapshot` (REQ-6879,
/// TODO-6892): the request is read as a snapshot's, and the WELCOME the
/// server writes is byte for byte the shared table's.
#[test]
fn the_snapshot_wire_vectors_hold() {
    use neoteletext_web::wire_vectors::vectors;
    let row = |name: &str| vectors().find(|vector| vector.name == name).unwrap_or_else(|| panic!("{name}"));
    assert_eq!(vector_bytes(row("hello_snapshot").value), request(0x100, ANY));
    let mut channel = running();
    channel.opened(SNAPSHOT, None, 0);
    channel.received(SNAPSHOT, &vector_bytes(row("hello_snapshot").value), 0);
    let effects = channel.take_effects();
    assert_eq!(all_sent_to(&effects, SNAPSHOT), vector_bytes(row("welcome_snapshot").value));
    assert_eq!(snapshots_told(&effects).len(), 1);
}

/// REQ-6881 item 5: the program's page goes whole as page packets, with no
/// LINKS, index, cursor or rule table, then a normal close.
#[test]
fn the_programs_snapshot_page_is_sent_whole_then_the_connection_closes() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let _ = channel.take_effects();
    channel.from_program(&answer(1, &Value::Null), 10);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    assert_eq!(all_sent_to(&effects, SNAPSHOT), page_packets(&subpage()), "the sub-page's packets and nothing else, its links among them");
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_NORMAL]);
    assert_eq!(told(&effects), [] as [Value; 0], "nothing is told of the answer's sending");
}

/// REQ-6881 item 5: a kept ReGIS picture follows its page as one REGIS
/// record of its kept stream, and its links and cursor are not sent.
#[test]
fn a_kept_regis_picture_follows_its_page_as_the_regis_record() {
    let mut channel = running();
    let drawing = b"P[10,10]V[30,10]";
    channel.from_program(&line(&json!({"type": "regis", "connections": "kept", "page": "100", "sub": "0000", "restart": true, "data": base64(drawing)})), 0);
    channel.from_program(
        &line(&json!({"type": "regis_links", "connections": "kept", "page": "100", "sub": "0000", "links": [{"drawing": 1, "target": "200"}]})),
        0,
    );
    channel.from_program(&line(&json!({"type": "regis_cursor", "connections": "kept", "page": "100", "sub": "0000", "drawing": 1})), 0);
    let _ = channel.take_effects();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, 0), 1);
    let _ = channel.take_effects();
    channel.from_program(&answer(1, &json!({"kept": "0000"})), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = all_sent_to(&effects, SNAPSHOT);
    let packets = page_packets(&subpage());
    assert_eq!(sent[..packets.len()], packets[..]);
    assert_eq!(record_types(&sent[packets.len()..]), [TYPE_REGIS], "never DRAWING_LINKS ({TYPE_DRAWING_LINKS}) or REGIS_CURSOR ({TYPE_REGIS_CURSOR})");
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_NORMAL]);
}

/// REQ-6881 item 5: a NAPLPS picture's objects follow it as one
/// `NAPLPS_OBJECTS` record, with no drawing links.
#[test]
fn a_naplps_picture_brings_its_objects_and_no_drawing_links() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let _ = channel.take_effects();
    let object = json!({"number": 1, "kind": "region", "shown": true, "detectable": true, "layer": 1, "order": 0, "x": 0, "y": 0, "width": 10, "height": 10,
                        "origin_x": 0, "origin_y": 0, "frames": [], "cover": 0});
    channel.from_program(&answer(1, &json!({"language": "naplps", "data": base64(b"\x0e"), "objects": [object]})), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = all_sent_to(&effects, SNAPSHOT);
    let packets = page_packets(&subpage());
    assert_eq!(record_types(&sent[packets.len()..]), [TYPE_NAPLPS, TYPE_NAPLPS_OBJECTS]);
}

/// REQ-6769, REQ-6881 item 5: a receiver that did not agree the picture's
/// language is sent the sub-page with the picture as mosaics, and no
/// picture record.
#[test]
fn a_picture_in_a_language_not_agreed_is_sent_as_its_mosaic_fallback() {
    let mut channel = running();
    channel.opened(SNAPSHOT, None, 0);
    let hello = Item::Event(Event::Hello { version: VERSION, caps: SNAPSHOT_CAP | SELECT_CAP, agent: None });
    channel.received(SNAPSHOT, &bare(&[hello, select_with(0x100, ANY, "")]), 0);
    let (caps, _) = after_welcome(&channel.take_effects(), SNAPSHOT);
    assert_eq!(caps, SNAPSHOT_CAP | SELECT_CAP);
    channel.from_program(&answer(1, &json!({"language": "regis", "data": base64(b"P[0,0]V[799,479]")})), 1);
    let effects = channel.take_effects();
    let (_, sent) = after_welcome(&effects, SNAPSHOT);
    assert!(record_types(&sent).is_empty(), "no picture record: {:?}", record_types(&sent));
    assert!(sent.len() > page_packets(&subpage()).len(), "rows 1 to 23 are the picture's mosaics, where the sub-page has two rows");
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_NORMAL]);
}

/// A visitor's `describe` answered public, kept for the description cache's
/// minute.
fn described_publicly(channel: &mut Channel, content: &Value, now: u64) {
    channel.describe(9, crate::protocol::PageNumber(0x100), now);
    assert_eq!(told(&channel.take_effects()), [json!({"type": "describe", "page": "100"})]);
    channel.from_program(&public(content), now);
    let _ = channel.take_effects();
}

/// REQ-6881 item 4 step 2: a public description already kept answers at
/// once, and the program is asked nothing; one of another sub-page does not
/// answer a sub-page's snapshot.
#[test]
fn a_cached_public_description_answers_without_asking_the_program() {
    let mut channel = running();
    described_publicly(&mut channel, &subpage(), 0);
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 1);
    let effects = channel.take_effects();
    assert_eq!(told(&effects), [] as [Value; 0]);
    assert_eq!(after_welcome(&effects, SNAPSHOT).1, page_packets(&subpage()));
    ask(&mut channel, SNAPSHOT + 1, FROM, (0x100, 0), 2);
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0], "its sub-page is the one selected");
    ask(&mut channel, SNAPSHOT + 2, FROM, (0x100, 1), 3);
    assert_eq!(snapshots_told(&channel.take_effects()).len(), 1, "another sub-page asks the program");
}

/// REQ-6881 item 4 step 5: a declined snapshot asks for the description as
/// a visitor would, and a public one answers.
#[test]
fn a_declined_snapshot_is_answered_by_a_public_description() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let _ = channel.take_effects();
    channel.from_program(&declined(1), 1);
    assert_eq!(told(&channel.take_effects()), [json!({"type": "describe", "page": "100"})], "the page number and nothing else");
    channel.from_program(&public(&subpage()), 2);
    let effects = channel.take_effects();
    assert_eq!(after_welcome(&effects, SNAPSHOT).1, page_packets(&subpage()));
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_NORMAL]);
}

/// REQ-6881 item 4 step 6: declined, and not public: DENIED 404 naming the
/// place, then a normal close.
#[test]
fn a_declined_snapshot_of_a_page_not_public_is_denied_404() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, 2), 0);
    let _ = channel.take_effects();
    channel.from_program(&declined(1), 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "description", "page": "100", "public": false})), 2);
    let effects = channel.take_effects();
    assert_eq!(denied_in(&after_welcome(&effects, SNAPSHOT).1), Some((404, "100/0002".into())));
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_NORMAL]);
}

/// REQ-6900 item 5, REQ-6881 item 4 step 5: past `SNAPSHOT_ANSWER_MS` the
/// description is asked for, and the late answer is refused `not_awaited`,
/// naming the snapshot.
#[test]
fn a_late_answer_is_refused_and_the_description_answers() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let _ = channel.take_effects();
    assert_eq!(channel.next_wake(), Some(SNAPSHOT_ANSWER_MS));
    channel.deadline(SNAPSHOT_ANSWER_MS);
    assert_eq!(told(&channel.take_effects()), [json!({"type": "describe", "page": "100"})]);
    channel.from_program(&answer(1, &Value::Null), SNAPSHOT_ANSWER_MS + 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [(json!("not_awaited"), "snapshot 1 is not awaited".into())]);
    assert_eq!(all_sent_to(&effects, SNAPSHOT), Vec::<u8>::new());
    channel.from_program(&public(&subpage()), SNAPSHOT_ANSWER_MS + 2);
    assert_eq!(after_welcome(&channel.take_effects(), SNAPSHOT).1, page_packets(&subpage()));
}

/// REQ-6900 item 5: an answer to a snapshot never sent, or answered
/// already, is refused `not_awaited`.
#[test]
fn a_snapshot_answer_not_awaited_is_refused() {
    let mut channel = running();
    channel.from_program(&declined(7), 0);
    assert_eq!(refusals(&channel.take_effects()), [(json!("not_awaited"), "snapshot 7 is not awaited".into())]);
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    channel.from_program(&answer(1, &Value::Null), 1);
    let _ = channel.take_effects();
    channel.from_program(&answer(1, &Value::Null), 2);
    assert_eq!(refusals(&channel.take_effects())[0].0, json!("not_awaited"));
}

/// REQ-6900 item 5: an answer of another page or sub-code is refused
/// `wrong_page`, a kept picture not kept `not_kept`, a picture the
/// receiver would refuse by its rule; each is then a decline, and the
/// description is asked for.
#[test]
fn a_snapshot_page_of_another_page_or_sub_code_is_refused_wrong_page() {
    let cases: [(u16, Value, Value, &str); 4] = [
        (0x100, with(&subpage(), "/page", json!("200")), Value::Null, "wrong_page"),
        (0x1, subpage(), Value::Null, "wrong_page"),
        (0x0, subpage(), json!({"kept": "0000"}), "not_kept"),
        (0x0, subpage(), json!({"language": "regis", "data": base64(b"V[1,1]D(S2(P9),C)")}), "invalid_regis"),
    ];
    for (sub, page, picture, rule) in cases {
        let mut channel = running();
        ask(&mut channel, SNAPSHOT, FROM, (0x100, if sub == 0x100 { ANY } else { sub }), 0);
        let _ = channel.take_effects();
        let mut message = json!({"type": "snapshot_page", "snapshot": 1, "page": page});
        if !picture.is_null() {
            message["picture"] = picture;
        }
        channel.from_program(&line(&message), 1);
        let effects = channel.take_effects();
        let refused = refusals(&effects);
        assert_eq!(refused.len(), 1, "{rule}: {refused:?}");
        assert_eq!(refused[0].0, json!(rule));
        assert!(refused[0].1.starts_with("snapshot 1: "), "{}", refused[0].1);
        assert!(told(&effects).iter().any(|line| line["type"] == "describe"), "{rule}: taken as a decline");
    }
}

/// REQ-6881 item 4 step 4: connections on one place wait on one question,
/// and an answer kept for the place answers the next without asking.
#[test]
fn two_snapshots_of_one_place_ask_the_program_once() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    ask(&mut channel, SNAPSHOT + 1, [198, 51, 100, 1], (0x100, ANY), 1);
    assert_eq!(snapshots_told(&channel.take_effects()).len(), 1);
    channel.from_program(&answer(1, &Value::Null), 2);
    let effects = channel.take_effects();
    for connection in [SNAPSHOT, SNAPSHOT + 1] {
        assert_eq!(after_welcome(&effects, connection).1, page_packets(&subpage()), "{connection}");
    }
    ask(&mut channel, SNAPSHOT + 2, FROM, (0x100, ANY), 3);
    let effects = channel.take_effects();
    assert_eq!(told(&effects), [] as [Value; 0]);
    assert_eq!(after_welcome(&effects, SNAPSHOT + 2).1, page_packets(&subpage()));
}

/// REQ-6881 item 4 step 1: with no program, DENIED 503.
#[test]
fn no_program_is_denied_503() {
    let mut channel = running();
    channel.program_ended();
    let _ = channel.take_effects();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let effects = channel.take_effects();
    assert_eq!(denied_in(&after_welcome(&effects, SNAPSHOT).1), Some((503, "100".into())));
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_NORMAL]);
    // A program that ends while one waits: DENIED 503 too.
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let _ = channel.take_effects();
    channel.program_ended();
    assert_eq!(denied_in(&after_welcome(&channel.take_effects(), SNAPSHOT).1), Some((503, "100".into())));
}

/// REQ-6881 items 1 and 3: SNAPSHOT without SELECT, a second SELECT, a
/// SELECT of page 0 and any input break the protocol.
#[test]
fn a_second_select_or_any_input_breaks_the_protocol() {
    let broke = |items: &[Item<'_>]| {
        let mut channel = running();
        channel.opened(SNAPSHOT, None, 0);
        channel.received(SNAPSHOT, &bare(items), 0);
        let effects = channel.take_effects();
        assert!(snapshots_told(&effects).len() <= 1);
        closes(&effects, SNAPSHOT)
    };
    let offer = Item::Event(Event::Hello { version: VERSION, caps: SNAPSHOT_OFFER, agent: None });
    let without_select = Item::Event(Event::Hello { version: VERSION, caps: SNAPSHOT_CAP, agent: None });
    assert_eq!(broke(&[without_select, select_with(0x100, ANY, "")]), [CLOSE_BROKE_PROTOCOL]);
    assert_eq!(broke(&[offer, select_with(0x100, ANY, ""), select_with(0x200, ANY, "")]).first(), Some(&CLOSE_BROKE_PROTOCOL));
    assert_eq!(broke(&[offer, select_with(0, ANY, "")]), [CLOSE_BROKE_PROTOCOL]);
    assert_eq!(broke(&[offer, Item::Event(Event::FocusIn)]), [CLOSE_BROKE_PROTOCOL]);
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let _ = channel.take_effects();
    channel.received(SNAPSHOT, &bare(&[key(super::ON, neoteletext_input::event::Action::Press, false)]), 1);
    let effects = channel.take_effects();
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_BROKE_PROTOCOL]);
    assert!(told(&effects).is_empty(), "the program hears nothing of it");
    channel.from_program(&answer(1, &Value::Null), 2);
    assert!(all_sent_to(&channel.take_effects(), SNAPSHOT).is_empty(), "a connection that broke the protocol is sent nothing more");
}

/// T1, T17 (tab-thumbnails.md section 13.2), REQ-6900 item 1: nothing of a
/// viewer reaches the program. A receiver that sends an agent text and a
/// query with its snapshot request is heard as `snapshot` with its four
/// fields and nothing else, through every path: asked, declined, described.
#[test]
fn nothing_of_a_viewer_reaches_the_program() {
    const SECRET_QUERY: &str = "user=alice&token=s3cr3t";
    const SECRET_AGENT: &str = "Private/1.0 (alice)";
    let mut channel = running();
    channel.opened(SNAPSHOT, Some(IpAddr::from(FROM)), 0);
    let hello = Item::Event(Event::Hello { version: VERSION, caps: SNAPSHOT_OFFER | AGENT, agent: Some(SECRET_AGENT) });
    channel.received(SNAPSHOT, &bare(&[hello, select_with(0x100, 1, SECRET_QUERY)]), 0);
    let mut lines: Vec<Value> = told(&channel.take_effects());
    channel.from_program(&declined(1), 1);
    lines.extend(told(&channel.take_effects()));
    channel.deadline(1 + DESCRIPTION_WAIT_MS);
    lines.extend(told(&channel.take_effects()));
    channel.closed(SNAPSHOT, 2 + DESCRIPTION_WAIT_MS);
    lines.extend(told(&channel.take_effects()));
    assert_eq!(
        lines,
        [json!({"type": "snapshot", "snapshot": 1, "page": "100", "sub": "0001", "path": "100/0001"}), json!({"type": "describe", "page": "100"})]
    );
    let text = format!("{lines:?}");
    for secret in ["alice", "s3cr3t", "Private", "192.0.2.7", "\"connection\"", "viewer", "remembered", "query"] {
        assert!(!text.contains(secret), "{secret} reached the program: {text}");
    }
    assert_eq!(channel.viewers(), 0, "no viewer was made");
}

/// REQ-6881 item 2, REQ-6882: snapshots make no viewer and count against no
/// viewer limit: after `MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR` snapshots
/// from one address within the hour, a new viewer from it is still
/// admitted.
#[test]
fn a_snapshot_makes_no_viewer_and_counts_against_no_viewer_limit() {
    let mut channel = running();
    let per_window = u64::from(MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE);
    for index in 0..u64::from(MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR) {
        let now = index / per_window * SNAPSHOT_WINDOW_MS;
        let connection = 1_000 + index;
        ask(&mut channel, connection, FROM, (0x100, ANY), now);
        channel.closed(connection, now);
        let _ = channel.take_effects();
    }
    assert_eq!((channel.viewers(), channel.stats().viewers_total, channel.stats().snapshots), (0, 0, u64::from(MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR)));
    let viewer = super::viewers::new_viewer(&mut channel, 1, IpAddr::from(FROM), &[0x5A; 32], 5 * SNAPSHOT_WINDOW_MS);
    assert!(super::viewers::handle(&viewer), "a viewer is still new to the address: {viewer}");
}

/// REQ-6882: viewers' connections count against no snapshot limit: with
/// more viewers' connections open from an address than
/// `MOST_OPEN_SNAPSHOTS_PER_ADDRESS`, a snapshot from it is admitted.
#[test]
fn snapshots_and_viewers_count_apart() {
    let mut channel = running();
    for connection in 1..=u64::try_from(MOST_OPEN_SNAPSHOTS_PER_ADDRESS).unwrap() + 1 {
        channel.opened(connection, Some(IpAddr::from(FROM)), 0);
        channel.received(connection, &message(&channel, connection, &[hello()]), 0);
    }
    let _ = channel.take_effects();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    assert_eq!(snapshots_told(&channel.take_effects()).len(), 1);
    assert_eq!(channel.snapshots.counts().0, 1);
}

/// REQ-6882 item 1: the 121st snapshot from an address in a minute is
/// DENIED 429 with no place, and the program is told nothing of it; the
/// window's end admits the next.
#[test]
fn the_121st_snapshot_from_an_address_in_a_minute_is_denied_429() {
    let mut channel = running();
    let bound = u64::from(MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE);
    for index in 0..bound {
        // Distinct places of no answer, so each is asked, and closed at once.
        ask(&mut channel, 1_000 + index, FROM, (0x100, ANY), 0);
        channel.closed(1_000 + index, 0);
    }
    let _ = channel.take_effects();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 1);
    let effects = channel.take_effects();
    let (caps, rest) = after_welcome(&effects, SNAPSHOT);
    assert_eq!(caps, SNAPSHOT_OFFER, "WELCOME agrees SNAPSHOT, so the receiver knows the server knows snapshots");
    assert_eq!(denied_in(&rest), Some((429, String::new())));
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_NORMAL]);
    assert_eq!(told(&effects), [] as [Value; 0]);
    channel.closed(SNAPSHOT, 1);
    ask(&mut channel, SNAPSHOT + 1, FROM, (0x200, ANY), SNAPSHOT_WINDOW_MS);
    assert_eq!(snapshots_told(&channel.take_effects()).len(), 1, "a new minute");
}

/// REQ-6882 item 2: a ninth snapshot open from one address is DENIED 429.
#[test]
fn a_ninth_open_snapshot_from_an_address_is_denied_429() {
    let mut channel = running();
    for index in 0..u64::try_from(MOST_OPEN_SNAPSHOTS_PER_ADDRESS).unwrap() {
        ask(&mut channel, 1_000 + index, FROM, (0x100, ANY), 0);
    }
    let _ = channel.take_effects();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    assert_eq!(denied_in(&after_welcome(&channel.take_effects(), SNAPSHOT).1), Some((429, String::new())));
    channel.closed(1_000, 1);
    ask(&mut channel, SNAPSHOT + 1, FROM, (0x100, ANY), 1);
    assert!(denied_in(&after_welcome(&channel.take_effects(), SNAPSHOT + 1).1).is_none(), "one closed makes room");
}

/// REQ-6882 item 3: the channel's open snapshots are bounded, from every
/// address together.
#[test]
fn the_channels_open_snapshots_are_bounded() {
    let mut channel = running();
    for index in 0..u64::try_from(MOST_OPEN_SNAPSHOTS).unwrap() {
        let [_, _, high, low] = u32::try_from(index).unwrap().to_be_bytes();
        ask(&mut channel, 1_000 + index, [10, 1, high, low], (0x100, ANY), 0);
    }
    let _ = channel.take_effects();
    assert_eq!(channel.snapshots.counts().0, MOST_OPEN_SNAPSHOTS);
    ask(&mut channel, SNAPSHOT, [10, 2, 0, 1], (0x100, ANY), 0);
    assert_eq!(denied_in(&after_welcome(&channel.take_effects(), SNAPSHOT).1), Some((429, String::new())));
}

/// REQ-6882 items 7 and 8: within the burst the program is asked; past its
/// rate the description is asked instead, and its not being public ends
/// with DENIED 429.
#[test]
fn the_program_is_sent_at_most_its_snapshot_rate_and_the_rest_go_to_the_description() {
    let mut channel = running();
    for index in 0..SNAPSHOTS_BURST {
        let page = 0x200 + u16::try_from(index).unwrap();
        ask(&mut channel, 1_000 + index, [10, 0, 0, u8::try_from(index).unwrap()], (page, ANY), 0);
        let number = channel.snapshots.last_number();
        channel.from_program(&line(&json!({"type": "snapshot_declined", "snapshot": number})), 0);
    }
    let effects = channel.take_effects();
    assert_eq!(snapshots_told(&effects).len(), usize::try_from(SNAPSHOTS_BURST).unwrap());
    ask(&mut channel, SNAPSHOT, FROM, (0x300, ANY), 0);
    let effects = channel.take_effects();
    assert_eq!(told(&effects), [json!({"type": "describe", "page": "300"})], "the description, not the program's drawing");
    channel.from_program(&line(&json!({"type": "description", "page": "300", "public": false})), 1);
    assert_eq!(denied_in(&after_welcome(&channel.take_effects(), SNAPSHOT).1), Some((429, "300".into())));
    // A tenth of a second later one more may be asked.
    ask(&mut channel, SNAPSHOT + 1, FROM, (0x301, ANY), 100);
    assert_eq!(snapshots_told(&channel.take_effects()).len(), 1);
}

/// REQ-6882 item 8: at most 16 snapshots are awaited at once.
#[test]
fn at_most_sixteen_snapshots_are_awaited() {
    let mut channel = running();
    let bound = u64::try_from(MOST_SNAPSHOTS_AWAITED).unwrap();
    for index in 0..=bound {
        let page = 0x200 + u16::try_from(index).unwrap();
        ask(&mut channel, 1_000 + index, [10, 0, 0, u8::try_from(index).unwrap()], (page, ANY), 0);
    }
    let effects = channel.take_effects();
    assert_eq!(snapshots_told(&effects).len(), MOST_SNAPSHOTS_AWAITED);
    assert_eq!(told(&effects).iter().filter(|line| line["type"] == "describe").count(), 1);
    assert_eq!(channel.snapshots.counts().1, MOST_SNAPSHOTS_AWAITED);
}

/// REQ-6882 item 9: an answer is kept for its place for a minute, and a
/// decline too.
#[test]
fn an_answer_is_cached_for_a_minute_and_a_decline_too() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    channel.from_program(&answer(1, &Value::Null), 0);
    let _ = channel.take_effects();
    ask(&mut channel, SNAPSHOT + 1, FROM, (0x100, ANY), SNAPSHOT_CACHE_MS - 1);
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0]);
    ask(&mut channel, SNAPSHOT + 2, [198, 51, 100, 2], (0x100, ANY), SNAPSHOT_CACHE_MS);
    assert_eq!(snapshots_told(&channel.take_effects()).len(), 1, "asked again after the minute");

    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x200, ANY), 0);
    channel.from_program(&declined(1), 0);
    channel.from_program(&line(&json!({"type": "description", "page": "200", "public": false})), 0);
    let _ = channel.take_effects();
    ask(&mut channel, SNAPSHOT + 1, FROM, (0x200, ANY), 1);
    let effects = channel.take_effects();
    assert_eq!(told(&effects), [] as [Value; 0], "neither the program's drawing nor its description is asked again");
    assert_eq!(denied_in(&after_welcome(&effects, SNAPSHOT + 1).1), Some((404, "200".into())));
}

/// REQ-6881 item 6: a snapshot connection is closed at
/// `SNAPSHOT_CONNECTION_MS` from its opening, with DENIED 503 when a SELECT
/// stands; the program is told nothing, and a snapshot awaited for it
/// stays awaited and its answer is kept.
#[test]
fn a_snapshot_is_closed_at_its_time_limit() {
    let mut channel = running();
    channel.opened(SNAPSHOT, None, 0);
    channel.received(SNAPSHOT, &bare(&[Item::Event(Event::Hello { version: VERSION, caps: SNAPSHOT_OFFER, agent: None })]), 0);
    let _ = channel.take_effects();
    assert_eq!(channel.next_wake(), Some(SNAPSHOT_CONNECTION_MS));
    channel.deadline(SNAPSHOT_CONNECTION_MS);
    let effects = channel.take_effects();
    assert_eq!((all_sent_to(&effects, SNAPSHOT), closes(&effects, SNAPSHOT)), (Vec::new(), alloc::vec![CLOSE_NORMAL]), "no SELECT stood");

    let mut channel = running();
    channel.opened(SNAPSHOT, None, 0);
    channel.received(SNAPSHOT, &bare(&[Item::Event(Event::Hello { version: VERSION, caps: SNAPSHOT_OFFER, agent: None })]), 0);
    let late = SNAPSHOT_CONNECTION_MS - 1;
    channel.received(SNAPSHOT, &bare(&[select_with(0x100, ANY, "")]), late);
    assert_eq!(snapshots_told(&channel.take_effects()).len(), 1);
    channel.deadline(SNAPSHOT_CONNECTION_MS);
    let effects = channel.take_effects();
    assert_eq!(denied_in(&after_welcome(&effects, SNAPSHOT).1), Some((503, "100".into())));
    assert_eq!(closes(&effects, SNAPSHOT), [CLOSE_NORMAL]);
    assert_eq!(told(&effects), [] as [Value; 0]);
    channel.closed(SNAPSHOT, SNAPSHOT_CONNECTION_MS);
    channel.from_program(&answer(1, &Value::Null), SNAPSHOT_CONNECTION_MS + 1);
    assert_eq!(refusals(&channel.take_effects()), [], "still awaited");
    ask(&mut channel, SNAPSHOT + 1, FROM, (0x100, ANY), SNAPSHOT_CONNECTION_MS + 2);
    let effects = channel.take_effects();
    assert_eq!(told(&effects), [] as [Value; 0], "its answer was kept");
    assert_eq!(after_welcome(&effects, SNAPSHOT + 1).1, page_packets(&subpage()));
}

/// REQ-6881 item 2: a message to `"all"`, or naming a snapshot
/// connection's number, reaches no snapshot connection; a snapshot's number
/// used as a connection is refused `unknown_connection` (REQ-6900 item 4).
#[test]
fn a_message_to_all_reaches_no_snapshot_connection() {
    let mut channel = running();
    ask(&mut channel, SNAPSHOT, FROM, (0x100, ANY), 0);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "page", "connections": "all", "page": subpage()})), 1);
    channel.from_program(&line(&json!({"type": "page", "connections": [SNAPSHOT], "page": subpage()})), 1);
    channel.from_program(&line(&json!({"type": "author_log", "connections": "all", "level": "info", "text": "hello"})), 1);
    let effects = channel.take_effects();
    assert!(all_sent_to(&effects, SNAPSHOT).is_empty(), "{effects:?}");
    for named in [1, SNAPSHOT] {
        channel.from_program(&line(&json!({"type": "reply", "connection": named, "request": 1, "magazines": []})), 2);
        assert_eq!(refusals(&channel.take_effects())[0].0, json!("unknown_connection"), "{named}");
        channel.from_program(&line(&json!({"type": "request_state", "connection": named})), 2);
        assert_eq!(refusals(&channel.take_effects())[0].0, json!("unknown_connection"), "{named}");
    }
}

/// REQ-6900 item 6, REQ-6882 item 6: `start` gives the program
/// `snapshot_limit_ms`.
#[test]
fn start_gives_the_program_the_snapshot_limit() {
    let mut channel = running();
    channel.program_ended();
    channel.program_started(0);
    let start = told(&channel.take_effects()).into_iter().find(|line| line["type"] == "start").unwrap();
    assert_eq!(start["budgets"]["snapshot_limit_ms"], json!(SNAPSHOT_ANSWER_MS));
}
