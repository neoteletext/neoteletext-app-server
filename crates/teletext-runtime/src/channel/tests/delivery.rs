//! What a connection is sent: one sub-page at a time and only its own
//! selection, changes as changed rows against the one sub-page kept for it,
//! and everything as whole valid packets. A program written for the test
//! speaks to a channel, and what each connection is sent is read back with
//! the engine's own readers.

use alloc::vec;

use neoteletext_core::consts::{C4_ERASE, COLUMNS, PACKET_BYTES};
use neoteletext_core::packet::{Decoded, Packet, decode, decode_address, encode, is_valid_stream_packet};
use neoteletext_core::types::Magazine;
use neoteletext_formats::item::{Item as FileItem, Reader as _};
use neoteletext_formats::t42::{T42Reader, T42Variant};
use neoteletext_input::cursor::TYPE_CURSOR;
use neoteletext_web::records::{
    ChannelRecord, Read, RecordReader, TYPE_DENIED, TYPE_INDEX, TYPE_LINKS, TYPE_LOG, TYPE_REDIRECT, TYPE_SET_COOKIE, TYPE_SUBPAGES,
};
use serde_json::{Value, json};

use super::{
    ACKNOWLEDGEMENT_WAIT_MS, ANY, Channel, ChannelConfig, Effect, String, Vec, answered, channel, connecteds, hello, inputs, line, message, record_types,
    select, selected, sends, sent_to, subpage, told, with,
};
use crate::channel::SESSION_GRACE_MS;
use crate::protocol::tests::base64;

/// The most sub-pages a page has (the engine's `MAX_SUBPAGES`).
const MOST_SUBPAGES: usize = neoteletext_core::limits::MAX_SUBPAGES;
/// A sub-code's bit the standard does not use (EN 300 706 Table 2): S2 has
/// three bits.
const UNUSED_SUB_CODE_BIT: u16 = 0x0080;
/// The rows of a page's enhancement group, X/26 to X/28.
const ENHANCEMENT_PACKETS: core::ops::RangeInclusive<u8> = 26..=28;
/// A session of the tests.
const SESSION: u64 = 9;
/// An index record as read: its magazine, its name, and each page's number,
/// sub-page count, title and section.
type ReadIndex = (u8, String, Vec<(u16, u16, String, String)>);

fn config(session_grace_ms: u64) -> ChannelConfig {
    ChannelConfig { name: "Test".into(), address: "http://127.0.0.1/test/".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms }
}

/// A channel with one connection for each `(connection, page, sub)`, each
/// with its first selection, numbered 1, its session the connection's number.
fn viewers(selections: &[(u64, u16, u16)]) -> Channel {
    let mut channel = Channel::new(config(0));
    channel.program_started(0);
    for &(connection, page, sub) in selections {
        channel.opened(connection, connection, Vec::new(), 0);
        channel.received(connection, &message(&[hello(), select(page, sub)]), 0);
    }
    let _ = channel.take_effects();
    channel
}

/// [`subpage`], as page `page` and sub-code `sub`.
fn sub_page(page: &str, sub: &str) -> Value {
    with(&with(&subpage(), "/page", json!(page)), "/sub", json!(sub))
}

/// [`subpage`] with no Link Extension links.
fn without_links() -> Value {
    with(&subpage(), "/link_extension", json!({"keys": [], "numbers": [], "texts": []}))
}

/// `page` with a row more of text.
fn changed(page: &Value) -> Value {
    with(page, "/rows/5", subpage()["rows"][0].clone())
}

/// The answer to `connection`'s selection numbered `request`.
fn answer(connection: u64, request: u64, page: &Value) -> Vec<u8> {
    line(&json!({"type": "page", "connections": [connection], "request": request, "page": page}))
}

/// A magazine of one page.
fn news() -> Value {
    json!([{"magazine": 1, "name": "News", "pages": [{"page": "100", "title": "Front", "section": null, "subpages": 1}]}])
}

/// A change, for every connection.
fn change(page: &Value) -> Vec<u8> {
    line(&json!({"type": "page", "connections": "all", "page": page}))
}

/// `bytes` as packets.
fn packets(bytes: &[u8]) -> impl Iterator<Item = Packet> + '_ {
    bytes.as_chunks::<PACKET_BYTES>().0.iter().map(|bytes| Packet { bytes: *bytes })
}

/// The headers in `bytes`: each one's sub-code and whether it erases.
fn headers(bytes: &[u8]) -> Vec<(u16, bool)> {
    packets(bytes)
        .filter_map(|packet| match decode(&packet) {
            Ok(Decoded::Header(header)) => Some((header.subcode?.value(), header.control?.0 & C4_ERASE != 0)),
            _ => None,
        })
        .collect()
}

/// The packet numbers in `bytes`, data channel 12's packet 31 left out.
fn numbers(bytes: &[u8]) -> Vec<u8> {
    packets(bytes).filter_map(|packet| decode_address(&packet).ok()).map(|address| address.number).filter(|number| *number != 31).collect()
}

/// The sub-page lists in `bytes`: each part's version and sub-codes.
fn lists(bytes: &[u8]) -> Vec<(u16, Vec<u16>)> {
    let mut reader = RecordReader::new();
    let mut found = Vec::new();
    for packet in packets(bytes) {
        if let Read::Record(ChannelRecord::SubPages(record)) = reader.push(&packet) {
            found.push((record.version, record.entries().map(|entry| entry.subcode.value()).collect()));
        }
    }
    found
}

/// The sub-page list message of `subs` for page 100, version 1.
fn list(connection: u64, request: u64, subs: impl Iterator<Item = u16>) -> Vec<u8> {
    let subpages: Vec<Value> = subs.map(|sub| json!({"sub": alloc::format!("{sub:04X}"), "title": null})).collect();
    line(&json!({"type": "subpages", "connections": [connection], "request": request, "page": "100", "version": 1, "subpages": subpages}))
}

/// How many of `effects` were refusals told to the program.
fn refusals(effects: &[Effect]) -> usize {
    told(effects).iter().filter(|line| line["type"] == "refused").count()
}

/// What connection `connection` of `channel` has selected: page and sub-code.
fn selection(channel: &Channel, connection: u64) -> Option<(u16, u16)> {
    channel.connections[&connection].selection.map(|selection| (selection.page.0, selection.sub.0))
}

/// REQ-1473 verification 1, 2 and 3: a message reaches the connections it
/// names, each once, and no other.
#[test]
fn a_message_reaches_the_connections_it_names() {
    let mut channel = viewers(&[(1, 0x100, 0), (2, 0x100, 0), (3, 0x200, 0)]);
    let log = |connections: Value| line(&json!({"type": "author_log", "connections": connections, "level": "W", "text": "slow", "page": null}));
    let reached = |effects: &[Effect]| -> Vec<usize> { [1, 2, 3].iter().map(|&to| record_types(&sent_to(effects, to)).len()).collect() };
    channel.from_program(&log(json!([2])), 1);
    assert_eq!(reached(&channel.take_effects()), [0, 1, 0]);
    channel.from_program(&log(json!([1, 3])), 1);
    assert_eq!(reached(&channel.take_effects()), [1, 0, 1]);
    channel.from_program(&log(json!("all")), 1);
    assert_eq!(reached(&channel.take_effects()), [1, 1, 1]);
}

/// REQ-1473 verification 4: nothing waits for a connection that opens later.
#[test]
fn a_connection_opened_later_is_sent_nothing_of_it() {
    let mut channel = viewers(&[(1, 0x100, 0)]);
    channel.from_program(&line(&json!({"type": "author_log", "connections": "all", "level": "W", "text": "slow", "page": null})), 1);
    let _ = channel.take_effects();
    channel.opened(2, 2, Vec::new(), 2);
    channel.received(2, &message(&[hello(), select(0x100, 0)]), 2);
    let effects = channel.take_effects();
    assert!(!record_types(&sent_to(&effects, 2)).contains(&TYPE_LOG), "and its WELCOME is all it is sent");
}

/// REQ-1473 verification 5: a connection that closed is named in vain, and
/// the program has done nothing wrong.
#[test]
fn a_closed_connection_named_is_no_fault() {
    let mut channel = viewers(&[(1, 0x100, 0), (2, 0x100, 0)]);
    channel.closed(2, 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "author_log", "connections": [2, 1], "level": "W", "text": "slow", "page": null})), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), 0);
    assert_eq!(record_types(&sent_to(&effects, 1)), [TYPE_LOG]);
    assert_eq!(sends(&effects), 1);
}

/// REQ-1473 verification 6, REQ-1472's rule of what is forwarded, REQ-1032
/// verification 5, 6 and 10, REQ-1501 verification 4 and REQ-1502
/// verification 3 and 4: a sub-page goes only where it is the selection,
/// however widely it is addressed, and what is not forwarded is no fault.
#[test]
fn a_page_for_all_reaches_only_its_selection() {
    let mut channel = viewers(&[(1, 0x100, 0), (2, 0x100, 1), (3, 0x200, 0)]);
    channel.from_program(&answer(1, 1, &subpage()), 1);
    channel.from_program(&answer(2, 1, &sub_page("100", "0001")), 1);
    channel.from_program(&answer(3, 1, &sub_page("200", "0000")), 1);
    let _ = channel.take_effects();
    channel.from_program(&change(&changed(&sub_page("100", "0001"))), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), 0);
    assert!(!sent_to(&effects, 2).is_empty(), "its selection");
    assert_eq!((sent_to(&effects, 1).len(), sent_to(&effects, 3).len()), (0, 0), "another sub-code, and another page");
    channel.from_program(&change(&sub_page("300", "0000")), 3);
    assert_eq!(sends(&channel.take_effects()), 0, "a page nobody selected goes nowhere");
}

/// REQ-1473 verification 7, REQ-1503 verification 3 and REQ-1480 verification
/// 4's last part: a page's list reaches the connections on that page, once
/// each at a version.
#[test]
fn a_sub_page_list_reaches_the_page_once_a_version() {
    let mut channel = viewers(&[(1, 0x100, 0), (2, 0x200, 0)]);
    let all = |version: u64| {
        let subpages = json!([{"sub": "0000", "title": null}, {"sub": "0001", "title": "Next"}]);
        line(&json!({"type": "subpages", "connections": "all", "request": null, "page": "100", "version": version, "subpages": subpages}))
    };
    channel.from_program(&all(1), 1);
    let effects = channel.take_effects();
    assert_eq!(lists(&sent_to(&effects, 1)), [(1, vec![0, 1])]);
    assert_eq!(sent_to(&effects, 2).len(), 0, "another page's connection");
    channel.from_program(&all(1), 2);
    assert_eq!(sends(&channel.take_effects()), 0, "that version is held");
    channel.from_program(&all(2), 3);
    assert_eq!(lists(&sent_to(&channel.take_effects(), 1)), [(2, vec![0, 1])], "a list that changed");
}

/// REQ-1472 verification 1, REQ-1502 verification 1 and REQ-1032 verification
/// 2: a page selection tells the program 3F7F, and its answer, the page's first
/// sub-page, becomes the selection, whichever sub-code that is.
#[test]
fn the_first_sub_page_answers_a_page_selection_and_becomes_it() {
    for (sub, text) in [(0, "0000"), (3, "0003")] {
        let mut channel = channel();
        channel.received(super::CONNECTION, &message(&[hello(), select(0x100, ANY)]), 0);
        let lines = told(&channel.take_effects());
        let asked = lines.iter().filter(|line| line["type"] == "selected").collect::<Vec<_>>();
        assert_eq!(asked.len(), 1, "{lines:?}");
        assert_eq!((&asked[0]["connection"], &asked[0]["page"], &asked[0]["sub"]), (&json!(super::CONNECTION), &json!("100"), &json!("3F7F")));
        assert!(asked[0].get("view").is_none(), "{asked:?}");
        assert_eq!(selection(&channel, super::CONNECTION), Some((0x100, ANY)));
        channel.from_program(&answer(super::CONNECTION, 2, &sub_page("100", text)), 1);
        assert_eq!(headers(&sent_to(&channel.take_effects(), super::CONNECTION)), [(sub, true)]);
        assert_eq!(selection(&channel, super::CONNECTION), Some((0x100, sub)), "the selection is that sub-page now");
    }
}

/// REQ-1472 verification 2 and 3: a sub-page selection shows that sub-page
/// and its later versions, whatever else of the page the program sends.
#[test]
fn a_sub_page_selection_gets_that_sub_page_and_its_later_versions() {
    let mut channel = viewers(&[(1, 0x100, 2)]);
    channel.from_program(&answer(1, 1, &sub_page("100", "0002")), 1);
    assert_eq!(headers(&sent_to(&channel.take_effects(), 1)), [(2, true)]);
    channel.from_program(&change(&sub_page("100", "0003")), 2);
    assert_eq!(sends(&channel.take_effects()), 0, "another sub-page of the page");
    channel.from_program(&change(&changed(&sub_page("100", "0002"))), 3);
    assert_eq!(headers(&sent_to(&channel.take_effects(), 1)), [(2, false)], "its later version");
}

/// REQ-1472 verification 4 and REQ-1502 verification 2, REQ-1032 verification
/// 4: a step is one SELECT, one `selected`, and one sub-page.
#[test]
fn stepping_is_one_selection_and_one_sub_page() {
    let mut channel = viewers(&[(1, 0x100, 0)]);
    channel.from_program(&answer(1, 1, &subpage()), 1);
    let _ = channel.take_effects();
    channel.received(1, &message(&[select(0x100, 1)]), 2);
    let lines = told(&channel.take_effects());
    assert_eq!(lines.iter().map(|line| line["type"].as_str().unwrap()).collect::<Vec<_>>(), ["selected"], "the same page: no `left`");
    assert_eq!((&lines[0]["sub"], &lines[0]["request"]), (&json!("0001"), &json!(2)));
    channel.from_program(&answer(1, 2, &sub_page("100", "0001")), 3);
    assert_eq!(headers(&sent_to(&channel.take_effects(), 1)), [(1, true)]);
}

/// REQ-1472 verification 6, REQ-1501 verification 3, REQ-1032 verification 1
/// to 3, REQ-1480 verification 4, REQ-1503 verification 1 and 3: the smallest
/// page and the largest cost one sub-page and a list the engine reads back
/// as given, in as many records as it needs, and selected again, no list.
#[test]
fn a_page_of_the_most_sub_pages_costs_one_sub_page_and_its_list() {
    let subs = || (0..=0x3F7E_u16).filter(|sub| sub & UNUSED_SUB_CODE_BIT == 0);
    for count in [1, MOST_SUBPAGES] {
        let mut channel = viewers(&[(1, 0x100, ANY)]);
        channel.from_program(&answer(1, 1, &subpage()), 1);
        channel.from_program(&list(1, 1, subs().take(count)), 1);
        let effects = channel.take_effects();
        assert_eq!(refusals(&effects), 0, "{count}");
        let bytes = sent_to(&effects, 1);
        assert_eq!(headers(&bytes), [(0, true)], "{count}: one sub-page");
        let read: Vec<u16> = lists(&bytes).into_iter().flat_map(|(_, part)| part).collect();
        assert_eq!(read, subs().take(count).collect::<Vec<_>>(), "{count}");

        channel.received(1, &message(&[select(0x200, ANY)]), 2);
        channel.received(1, &message(&[select(0x100, ANY)]), 3);
        let _ = channel.take_effects();
        channel.from_program(&answer(1, 3, &subpage()), 4);
        channel.from_program(&list(1, 3, subs().take(count)), 4);
        let again = sent_to(&channel.take_effects(), 1);
        assert_eq!(headers(&again), [(0, true)], "{count}");
        assert!(!record_types(&again).contains(&TYPE_SUBPAGES), "{count}: it holds that version");
    }
}

/// REQ-1472 verification 7 and REQ-1032 verification 9: one tab's selection
/// is nothing to another of the same session.
#[test]
fn a_second_connection_hears_nothing_of_the_firsts_selection() {
    let mut channel = Channel::new(config(0));
    channel.program_started(0);
    for connection in [1, 2] {
        channel.opened(connection, SESSION, Vec::new(), 0);
        channel.received(connection, &message(&[hello(), select(0x200, 0)]), 0);
    }
    let _ = channel.take_effects();
    channel.received(1, &message(&[select(0x100, ANY)]), 1);
    channel.from_program(&answer(1, 2, &subpage()), 2);
    let effects = channel.take_effects();
    assert_eq!(headers(&sent_to(&effects, 1)), [(0, true)]);
    assert_eq!(sent_to(&effects, 2).len(), 0);
    assert_eq!(selection(&channel, 2), Some((0x200, 0)));
}

/// REQ-1501 verification 1 and REQ-1480 verification 2: a frame holds one
/// header, one sub-page's, for a page of several sub-pages as for one.
#[test]
fn every_page_frame_holds_one_header_packet() {
    let mut channel = viewers(&[(1, 0x100, 0), (2, 0x100, 1), (3, 0x200, 0)]);
    channel.from_program(&answer(1, 1, &sub_page("100", "0000")), 1);
    channel.from_program(&answer(2, 1, &sub_page("100", "0001")), 1);
    channel.from_program(&answer(3, 1, &sub_page("200", "0000")), 1);
    let effects = channel.take_effects();
    for (connection, sub) in [(1, 0), (2, 1), (3, 0)] {
        assert_eq!(headers(&sent_to(&effects, connection)), [(sub, true)], "{connection}");
    }
}

/// REQ-1480 verification 1 and REQ-1462's verification of the server: every
/// kind of message a program sends reaches the viewer as whole valid packets of
/// the kind stated, and control and acknowledgement records with them.
#[test]
fn every_message_is_whole_valid_packets_of_its_kind() {
    let raw = encode::row(Magazine::from_bits(1), 1, &[b' '; COLUMNS]).bytes;
    let kinds: [(Value, &[u8]); 7] = [
        (json!({"type": "index", "connections": [1], "magazines": news()}), &[TYPE_INDEX]),
        (
            json!({"type": "subpages", "connections": [1], "request": null, "page": "100", "version": 1, "subpages": [{"sub": "0000", "title": null}]}),
            &[TYPE_SUBPAGES],
        ),
        (json!({"type": "author_log", "connections": [1], "level": "W", "text": "slow", "page": "100"}), &[TYPE_LOG]),
        (
            json!({"type": "cookie", "connection": 1, "name": "theme", "value": "dark", "path": null, "max_age": null, "same_site": null, "secure": false, "http_only": false}),
            &[TYPE_SET_COOKIE],
        ),
        (json!({"type": "denied", "connection": 1, "request": 1, "status": 404}), &[TYPE_DENIED]),
        (json!({"type": "redirect", "connection": 1, "request": 1, "page": "200", "sub": "3F7F"}), &[TYPE_REDIRECT]),
        (json!({"type": "packets", "connections": [1], "packets": base64(&raw)}), &[]),
    ];
    for (message, kind) in kinds {
        let mut channel = viewers(&[(1, 0x100, 0)]);
        channel.from_program(&line(&message), 1);
        let effects = channel.take_effects();
        assert_eq!(refusals(&effects), 0, "{message}: {:?}", told(&effects));
        let bytes = sent_to(&effects, 1);
        assert!(bytes.len() >= PACKET_BYTES && bytes.len().is_multiple_of(PACKET_BYTES), "{message}");
        assert!(packets(&bytes).all(|packet| is_valid_stream_packet(&packet)), "{message}");
        assert_eq!(record_types(&bytes), kind, "{message}");
    }
    let mut channel = viewers(&[(1, 0x100, 0)]);
    let welcomed = |effects: &[Effect]| sent_to(effects, 1);
    channel.from_program(&answer(1, 1, &subpage()), 1);
    channel.deadline(1 + ACKNOWLEDGEMENT_WAIT_MS);
    let bytes = welcomed(&channel.take_effects());
    assert!(bytes.len().is_multiple_of(PACKET_BYTES));
    assert!(packets(&bytes).all(|packet| is_valid_stream_packet(&packet)));
    assert_eq!(record_types(&bytes), [TYPE_LINKS, TYPE_CURSOR], "the frame's records");
}

/// REQ-1480 verification 3, REQ-1504 verification 4 and REQ-1494 item 2: every
/// frame of a sub-page with Link Extension links, changed rows and unchanged
/// links included, is followed by its LINKS record, and frames of one with none
/// by no record, which is how a receiver drops what it held.
#[test]
fn links_follow_every_frame_of_a_sub_page_that_has_them() {
    let mut channel = viewers(&[(1, 0x100, 0)]);
    let links = |effects: &[Effect]| usize::from(record_types(&sent_to(effects, 1)).contains(&TYPE_LINKS));
    channel.from_program(&answer(1, 1, &subpage()), 1);
    assert_eq!(links(&channel.take_effects()), 1, "whole");
    channel.from_program(&change(&changed(&subpage())), 2);
    assert_eq!(links(&channel.take_effects()), 1, "changed rows, the same links");
    channel.from_program(&change(&changed(&without_links())), 3);
    let effects = channel.take_effects();
    assert_eq!(headers(&sent_to(&effects, 1)), [(0, false)], "links alone changed: a frame says so");
    assert_eq!(links(&effects), 0, "no record: no links");
    channel.from_program(&change(&with(&changed(&without_links()), "/rows/6", subpage()["rows"][0].clone())), 4);
    let effects = channel.take_effects();
    assert_eq!((headers(&sent_to(&effects, 1)).len(), links(&effects)), (1, 0), "changed rows of a sub-page with none");
}

/// REQ-1504 verification 1, 2 and 3, REQ-1480 verification 5 and REQ-1032
/// verification 7: a later version goes as a header without the erase bit
/// and what differs, the enhancement group whole, and nothing when nothing does.
#[test]
fn a_later_version_goes_as_what_changed() {
    let mut channel = viewers(&[(1, 0x100, 0)]);
    let page = without_links();
    channel.from_program(&answer(1, 1, &page), 1);
    let whole = sent_to(&channel.take_effects(), 1);
    assert_eq!(headers(&whole), [(0, true)]);

    let one_row = changed(&page);
    channel.from_program(&change(&one_row), 2);
    let bytes = sent_to(&channel.take_effects(), 1);
    assert_eq!(headers(&bytes), [(0, false)]);
    assert_eq!(numbers(&bytes), [0, 5], "a header and that row");

    let enhanced = with(&one_row, "/enhancements/0/triplets/0/data", json!(1));
    channel.from_program(&change(&enhanced), 3);
    let bytes = sent_to(&channel.take_effects(), 1);
    let group: Vec<u8> = numbers(&whole).into_iter().filter(|number| ENHANCEMENT_PACKETS.contains(number)).collect();
    assert_ne!(group, [] as [u8; 0], "the fixture has a group");
    assert_eq!(numbers(&bytes), [vec![0], group].concat(), "a header and the group whole");

    channel.from_program(&change(&enhanced), 4);
    assert_eq!(sends(&channel.take_effects()), 0, "nothing differs");
}

/// REQ-1504 verification 5 and REQ-1474 verification 3: another sub-code is
/// never compared with what is kept: it goes whole, and is then what is kept.
#[test]
fn another_sub_code_goes_whole_and_is_kept_instead() {
    let mut channel = viewers(&[(1, 0x100, ANY)]);
    channel.from_program(&answer(1, 1, &subpage()), 1);
    channel.received(1, &message(&[select(0x100, 1)]), 2);
    let _ = channel.take_effects();
    assert!(channel.connections[&1].shown.is_none(), "a selection drops what was kept");
    channel.from_program(&answer(1, 2, &sub_page("100", "0001")), 3);
    assert_eq!(headers(&sent_to(&channel.take_effects(), 1)), [(1, true)]);
    channel.from_program(&change(&changed(&sub_page("100", "0001"))), 4);
    assert_eq!(numbers(&sent_to(&channel.take_effects(), 1)).first().copied(), Some(0));
    let kept = channel.connections[&1].shown.as_ref().unwrap();
    assert_eq!((kept.page, kept.sub), (0x100, 1));
}

/// REQ-1504 verification 6 and 7: the first frame after a selection, and the
/// first on a new connection, is whole with the erase bit, even for the
/// very sub-page that was shown, since the receiver's copy may be older.
#[test]
fn the_first_frame_after_a_selection_is_whole() {
    let mut channel = viewers(&[(1, 0x100, 0)]);
    channel.from_program(&answer(1, 1, &subpage()), 1);
    channel.received(1, &message(&[select(0x200, 0)]), 2);
    channel.received(1, &message(&[select(0x100, 0)]), 3);
    let _ = channel.take_effects();
    channel.from_program(&answer(1, 3, &subpage()), 4);
    assert_eq!(headers(&sent_to(&channel.take_effects(), 1)), [(0, true)], "the same sub-page, selected again");
    channel.opened(2, 2, Vec::new(), 5);
    channel.received(2, &message(&[hello(), select(0x100, 0)]), 5);
    let _ = channel.take_effects();
    channel.from_program(&answer(2, 1, &subpage()), 6);
    assert_eq!(headers(&sent_to(&channel.take_effects(), 2)), [(0, true)], "a new connection");
}

/// REQ-1504 verification 8, REQ-1474 verification 4 and REQ-1032 verification
/// 9: two tabs of one session each keep their own sub-page, and a change is
/// taken against what that tab was last sent.
#[test]
fn two_tabs_keep_their_own_sub_page() {
    let mut channel = Channel::new(config(0));
    channel.program_started(0);
    for connection in [1, 2] {
        channel.opened(connection, SESSION, Vec::new(), 0);
        channel.received(connection, &message(&[hello(), select(0x100, 0)]), 0);
    }
    let _ = channel.take_effects();
    let page = without_links();
    channel.from_program(&answer(1, 1, &page), 1);
    let _ = channel.take_effects();
    channel.from_program(&answer(2, 1, &changed(&page)), 2);
    let effects = channel.take_effects();
    assert_eq!(headers(&sent_to(&effects, 2)), [(0, true)], "the second tab was sent nothing yet");
    assert_eq!(sent_to(&effects, 1).len(), 0);
    channel.from_program(&change(&changed(&page)), 3);
    let effects = channel.take_effects();
    assert_eq!(numbers(&sent_to(&effects, 1)), [0, 5], "the first had the older version");
    assert_eq!(sent_to(&effects, 2).len(), 0, "the second had this one");
}

/// REQ-1474 verification 1 and REQ-1504 verification 8: a connection closed
/// leaves nothing it was sent.
#[test]
fn nothing_is_kept_after_a_connection_closes() {
    let mut channel = viewers(&[(1, 0x100, 0), (2, 0x100, 0)]);
    channel.from_program(&answer(1, 1, &subpage()), 1);
    assert!(channel.connections[&1].shown.is_some());
    channel.closed(1, 2);
    assert!(!channel.connections.contains_key(&1));
    assert_eq!(channel.connections.values().filter(|state| state.shown.is_some()).count(), 0);
}

/// REQ-1474 verification 2 and REQ-1038: a channel starting has no content: a
/// viewer is sent no page until the program answers its `connected`.
#[test]
fn a_starting_channel_has_nothing_to_show_until_its_program_answers() {
    let mut channel = viewers(&[(1, 0x100, 0)]);
    assert!(channel.connections[&1].shown.is_none());
    channel.deadline(ACKNOWLEDGEMENT_WAIT_MS);
    assert_eq!(headers(&sent_to(&channel.take_effects(), 1)), [] as [(u16, bool); 0]);
    channel.from_program(&answer(1, 1, &subpage()), ACKNOWLEDGEMENT_WAIT_MS);
    assert_eq!(headers(&sent_to(&channel.take_effects(), 1)), [(0, true)]);
}

/// REQ-1480 verification 6: the magazines of a reply read back, by the
/// engine's reader, as the magazines and pages the program gave.
#[test]
fn the_index_records_read_back_as_the_magazines_given() {
    let mut channel = channel();
    let magazines = json!([{"magazine": 1, "name": "News", "pages": [
        {"page": "100", "title": "Front", "section": null, "subpages": 1},
        {"page": "101", "title": "Headlines", "section": "News", "subpages": 3}
    ]}]);
    channel.from_program(&line(&json!({"type": "reply", "connection": super::CONNECTION, "request": 1, "magazines": magazines, "selected": null})), 1);
    let bytes = sent_to(&channel.take_effects(), super::CONNECTION);
    let mut reader = RecordReader::new();
    let mut read: Vec<ReadIndex> = Vec::new();
    for packet in packets(&bytes) {
        if let Read::Record(ChannelRecord::Index(record)) = reader.push(&packet) {
            let pages = record.pages().map(|page| (page.page, page.subpages, page.title.into(), page.section.into())).collect();
            read.push((record.magazine, record.name.into(), pages));
        }
    }
    let expected = vec![(0x100, 1, "Front".into(), String::new()), (0x101, 3, "Headlines".into(), "News".into())];
    assert_eq!(read, [(1, "News".into(), expected)]);
}

/// REQ-1480 verification 7: what a connection is sent is a T42 file: the
/// engine's file reader takes every packet of a connection's bytes, and finds
/// nothing to skip.
#[test]
fn a_recording_of_a_connection_is_a_t42_file() {
    let mut channel = viewers(&[(1, 0x100, 0)]);
    channel.from_program(&line(&json!({"type": "index", "connections": [1], "magazines": news()})), 1);
    channel.from_program(&answer(1, 1, &subpage()), 2);
    channel.from_program(&change(&changed(&subpage())), 3);
    let recording = sent_to(&channel.take_effects(), 1);
    let mut read = 0;
    let mut other = 0;
    let mut reader = T42Reader::new(T42Variant::Plain, None);
    let mut count = |item: FileItem<'_>| {
        if matches!(item, FileItem::Packet { .. }) {
            read += 1;
        } else {
            other += 1;
        }
    };
    reader.feed(&recording, &mut count);
    reader.finish(&mut count);
    assert_eq!((read, other), (recording.len() / PACKET_BYTES, 0));
}

/// REQ-1064: columns 8 to 39 of row 0 reach the viewer exactly as the program
/// gave them, text and mosaics, and a blank row 0 stays blank: the server
/// writes nothing there.
#[test]
fn row_0_is_the_pages_own() {
    /// Where the page's own columns start (EN 300 706 clause 9.3.1.4): 0 to
    /// 7 are the receiver's.
    const FIRST_OWN_COLUMN: usize = 8;
    let mut written = vec![b' '; COLUMNS];
    for (column, code) in written.iter_mut().enumerate().skip(FIRST_OWN_COLUMN) {
        *code = if column % 2 == 0 { b'A' } else { 0x7F };
    }
    written[FIRST_OWN_COLUMN] = 0x17;
    for row in [written, vec![b' '; COLUMNS]] {
        let mut channel = viewers(&[(1, 0x100, 0)]);
        channel.from_program(&answer(1, 1, &with(&subpage(), "/rows/0", json!(base64(&row)))), 1);
        let bytes = sent_to(&channel.take_effects(), 1);
        let header = packets(&bytes).find_map(|packet| if let Ok(Decoded::Header(header)) = decode(&packet) { Some(header) } else { None }).unwrap();
        let shown: Vec<u8> = header.display.iter().map(|byte| byte & 0x7F).collect();
        assert_eq!(shown, row[FIRST_OWN_COLUMN..], "as given");
    }
}

/// REQ-1065: nothing is sent for the clock's sake: a viewer on a page that
/// does not change is sent nothing for a minute, and the channel asks for no
/// wake that would send it.
#[test]
fn nothing_is_sent_for_the_clock() {
    /// A minute, a second at a time.
    const SECONDS: u64 = 60;
    let mut channel = answered();
    assert_eq!(channel.next_wake(), None);
    for second in 1..=SECONDS {
        channel.deadline(second * 1_000);
        assert_eq!(sends(&channel.take_effects()), 0, "{second}");
    }
}

/// REQ-1033: `start` tells a program the standard's maxima, from the engine's
/// constants, and the server's budgets.
#[test]
fn start_tells_the_standards_maxima_and_the_budgets() {
    use crate::protocol as server;
    use neoteletext_core::limits;
    let mut channel = Channel::new(config(0));
    channel.program_started(0);
    let lines = told(&channel.take_effects());
    let start = &lines[0];
    assert_eq!(start["type"], json!("start"));
    assert_eq!(start["protocol"], json!(server::PROTOCOL));
    let standard = &start["standard"];
    assert_eq!((&standard["magazines"], &standard["pages"]), (&json!(limits::MAGAZINES), &json!(limits::USABLE_PAGE_NUMBERS)));
    assert_eq!(standard["subpages"], json!(limits::MAX_SUBPAGES));
    assert_eq!(standard["keyable_pages"], json!(limits::KEYABLE_PAGES));
    assert_eq!(standard["decimal_subpages"], json!(limits::DECIMAL_SUBCODES));
    let budgets = &start["budgets"];
    assert_eq!(budgets["line_bytes"], json!(server::LONGEST_LINE));
    assert_eq!(budgets["title_characters"], json!(server::TITLE_CHARACTERS));
    assert_eq!(budgets["log_characters"], json!(server::OPERATOR_LOG_CHARACTERS));
    assert_eq!(budgets["named_connections"], json!(server::MOST_NAMED_CONNECTIONS));
    assert_eq!(budgets["author_log_bytes"], json!(neoteletext_formats::log::MAX_LOG_TEXT_BYTES));
    assert_eq!(budgets["reply_limit_ms"], json!(super::PROGRAM_REPLY_LIMIT_MS));
}

/// REQ-1034: a program started again is told of every viewer connected, and
/// what it then sends reaches the viewer on the page, a saved change without a
/// reload.
#[test]
fn a_restarted_program_is_told_of_its_viewers() {
    let mut channel = viewers(&[(1, 0x100, 0), (2, 0x200, 0)]);
    let page = without_links();
    channel.from_program(&answer(1, 1, &page), 1);
    let _ = channel.take_effects();
    channel.program_ended();
    channel.program_started(2);
    let lines = connecteds(&channel.take_effects());
    let told: Vec<(&Value, &Value)> = lines.iter().map(|line| (&line["connection"], &line["selection"]["page"])).collect();
    assert_eq!(told, [(&json!(1), &json!("100")), (&json!(2), &json!("200"))]);
    channel.from_program(&line(&json!({"type": "reply", "connection": 1, "request": 1, "magazines": news(), "selected": changed(&page)})), 3);
    assert_eq!(numbers(&sent_to(&channel.take_effects(), 1)), [0, 5], "the viewer sees the change, as what changed");
}

/// REQ-1040: a session whose connection closes stays for its grace: its
/// program is told `left` at once and `closed` only when the grace passes.
#[test]
fn a_closed_session_waits_for_its_viewer() {
    let mut channel = Channel::new(config(SESSION_GRACE_MS));
    channel.program_started(0);
    channel.opened(1, SESSION, Vec::new(), 0);
    channel.received(1, &message(&[hello(), select(0x100, 0)]), 0);
    let _ = channel.take_effects();
    channel.closed(1, 10);
    let lines = told(&channel.take_effects());
    assert_eq!(lines.iter().map(|line| line["type"].as_str().unwrap()).collect::<Vec<_>>(), ["left"]);
    assert_eq!(channel.next_wake(), Some(10 + SESSION_GRACE_MS));
    channel.deadline(9 + SESSION_GRACE_MS);
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0]);
    assert_eq!(channel.sessions(), 1);
    channel.deadline(10 + SESSION_GRACE_MS);
    let lines = told(&channel.take_effects());
    assert_eq!((&lines[0]["type"], &lines[0]["session"]), (&json!("closed"), &json!(SESSION)));
    assert_eq!(channel.sessions(), 0);
}

/// REQ-1040: a viewer who comes back within the grace continues their
/// session, under its own number, and the session does not end.
#[test]
fn a_viewer_who_comes_back_continues_their_session() {
    let mut channel = Channel::new(config(SESSION_GRACE_MS));
    channel.program_started(0);
    channel.opened(1, SESSION, Vec::new(), 0);
    channel.closed(1, 10);
    let _ = channel.take_effects();
    channel.opened(2, SESSION, Vec::new(), 20);
    channel.received(2, &message(&[hello(), select(0x100, 0)]), 20);
    let lines = connecteds(&channel.take_effects());
    assert_eq!(lines[0]["session"], json!(SESSION));
    assert_eq!(channel.stats().sessions_resumed, 1);
    channel.deadline(10 + SESSION_GRACE_MS);
    assert_eq!(told(&channel.take_effects()).iter().filter(|line| line["type"] == "closed").count(), 0);
}

/// REQ-1643: a pointer's column past 39 is a protocol error, and 39 is not: no
/// message carries a column beyond the page's 40.
#[test]
fn a_column_past_39_is_a_protocol_error() {
    /// A MOUSE press on page 100 at column 39, row 9, from the shared table's
    /// `mouse_press`, and the same at column 40.
    const LAST_COLUMN: [u8; 9] = [0xFF, 0x02, 0x80, 0x02, 0x00, 0x00, 0x00, 39, 9];
    const PAST_IT: [u8; 9] = [0xFF, 0x02, 0x80, 0x02, 0x00, 0x00, 0x00, 40, 9];
    let mut channel = selected();
    channel.received(super::CONNECTION, &LAST_COLUMN, 1);
    let lines = inputs(&channel.take_effects());
    assert_eq!(lines[0]["event"]["column"], json!(39));
    channel.received(super::CONNECTION, &PAST_IT, 2);
    assert_eq!(channel.take_effects(), [Effect::Close { connection: super::CONNECTION, code: neoteletext_web::consts::CLOSE_BROKE_PROTOCOL }]);
}

/// REQ-1553: only a new selection ends a page's updates: after SELECT of no
/// page, nothing more of it is sent.
#[test]
fn a_selection_of_no_page_ends_its_updates() {
    let mut channel = answered();
    channel.received(super::CONNECTION, &message(&[select(0, ANY)]), 1);
    let lines = told(&channel.take_effects());
    assert_eq!(lines[0]["type"], json!("left"));
    channel.from_program(&change(&changed(&subpage())), 2);
    assert_eq!(sends(&channel.take_effects()), 0);
}
