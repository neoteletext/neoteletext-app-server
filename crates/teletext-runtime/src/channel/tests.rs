//! A connection's input against its selection: PAGE withdrawn, every stamp
//! checked, a release never dropped, a selected page active whatever its
//! focus, and held input released when the page loses it (neoteletext-
//! common's DEC-1538; REQ-1472, REQ-1553, REQ-1555, REQ-1557, REQ-207).

extern crate std;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use neoteletext_input::VERSION;
use neoteletext_input::caps::{CURSOR_CAP, SELECT_CAP};
use neoteletext_input::cursor::TYPE_CURSOR;
use neoteletext_input::event::{Action, Device, Event, Item, Key, KeyCode, Mouse, MouseAction, Point, Select, Stamp, write};
use neoteletext_web::consts::{CLOSE_BROKE_PROTOCOL, CLOSE_WRONG_VERSION};
use neoteletext_web::records::{TYPE_LINKS, TYPE_REDIRECT};
use serde_json::{Value, json};

use super::{ACKNOWLEDGEMENT_WAIT_MS, Channel, ChannelConfig, Effect, FIRST_SELECTION_WAIT_MS, PROGRAM_REPLY_LIMIT_MS, STALE_INPUT_REPORT_SECONDS};
use crate::protocol::tests::{line, subpage, with};

mod delivery;
mod sound;

/// The connection and session of a test.
const CONNECTION: u64 = 1;
/// More than a message of the tests.
const MESSAGE_BYTES: usize = 256;
/// Page 100, sub-code 0000.
const ON: Stamp = Stamp { page: 0x100, sub: 0 };
/// Another page.
const OTHER: Stamp = Stamp { page: 0x200, sub: 0 };
/// Another sub-page of page 100.
const OTHER_SUB: Stamp = Stamp { page: 0x100, sub: 1 };
/// No particular sub-page (EN 300 706 clause 9.6.1).
const ANY: u16 = 0x3F7F;
/// `a`, by key.
const A: KeyCode = KeyCode::Character('a');

/// A channel with the connection just open: `connected` is held for its
/// first selection.
fn held() -> Channel {
    let config =
        ChannelConfig { name: "Test".into(), address: "http://127.0.0.1/test/".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut channel = Channel::new(config);
    channel.program_started(0);
    channel.opened(CONNECTION, 1, Vec::new(), 0);
    let _ = channel.take_effects();
    channel
}

/// A channel whose program has been told of the connection, with no
/// selection, so that a selection is `selected` and its number 2.
fn channel() -> Channel {
    let mut channel = held();
    channel.received(CONNECTION, &message(&[select(0, ANY)]), 0);
    let _ = channel.take_effects();
    channel
}

/// The items as one message of the input stream.
fn message(items: &[Item<'_>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for item in items {
        let mut one = [0; MESSAGE_BYTES];
        let length = write(item, &mut one).unwrap();
        bytes.extend_from_slice(&one[..length]);
    }
    bytes
}

fn hello() -> Item<'static> {
    Item::Event(Event::Hello { version: VERSION, caps: CURSOR_CAP | SELECT_CAP, agent: None })
}

fn select(page: u16, sub: u16) -> Item<'static> {
    select_with(page, sub, "")
}

fn select_with(page: u16, sub: u16, parameters: &str) -> Item<'_> {
    Item::Event(Event::Select(Select::new(page, sub).unwrap(), parameters))
}

fn key(on: Stamp, action: Action, lost: bool) -> Item<'static> {
    Item::Event(Event::Key(Key { on, action, key: A, mods: 0, text: None, base: None, lost }))
}

fn mouse(on: Stamp, action: MouseAction, button: u8) -> Item<'static> {
    let at = Point { column: 7, row: 9, pixels: None, mods: 0 };
    Item::Event(Event::Mouse(Mouse { on, action, button, at, device: Device::Mouse }))
}

/// A channel with the connection welcomed and on page 100, sub-code 0000,
/// its HELLO acknowledged.
fn selected() -> Channel {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello(), select(0x100, 0)]), 0);
    channel.deadline(ACKNOWLEDGEMENT_WAIT_MS);
    let _ = channel.take_effects();
    channel
}

/// The lines the program is told.
fn told(effects: &[Effect]) -> Vec<Value> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ToProgram { line } => Some(serde_json::from_slice(line).unwrap()),
            _ => None,
        })
        .collect()
}

/// `input` lines among `effects`.
fn inputs(effects: &[Effect]) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == "input").collect()
}

fn sends(effects: &[Effect]) -> usize {
    effects.iter().filter(|effect| matches!(effect, Effect::Send { .. })).count()
}

/// PAGE is withdrawn: its type is a protocol error (REQ-1556).
/// REQ-1472 verification 8, its first half.
#[test]
fn page_closes_the_session() {
    let mut channel = selected();
    channel.received(CONNECTION, &[0xFF, 0x0A, 0x80, 0x02], 1);
    assert_eq!(channel.take_effects(), [Effect::Close { connection: CONNECTION, code: CLOSE_BROKE_PROTOCOL }]);
}

/// REQ-1557 item 2: a press stamped with the selection reaches the program
/// with the stamp's page and sub-code.
/// REQ-1555 item 3.
#[test]
fn a_matching_stamp_reaches_the_program_with_page_and_sub() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 1);
    let lines = inputs(&channel.take_effects());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!((&lines[0]["page"], &lines[0]["sub"], &lines[0]["lost"]), (&json!("100"), &json!("0000"), &json!(false)));
    assert_eq!(channel.stats().stale_input, 0);
}

/// REQ-1557 item 3: another page, another sub-page of the same page, or no
/// selection at all reach no program, and each counts once.
/// REQ-1555 item 3.
#[test]
fn a_stale_stamp_reaches_nothing_and_counts() {
    let mut channel = selected();
    for (dropped, on) in [OTHER, OTHER_SUB].into_iter().enumerate() {
        channel.received(CONNECTION, &message(&[key(on, Action::Press, false)]), 1);
        assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0]);
        assert_eq!(channel.stats().stale_input, dropped as u64 + 1);
    }
    let mut none = self::channel();
    none.received(CONNECTION, &message(&[hello(), key(ON, Action::Press, false)]), 1);
    assert_eq!(inputs(&none.take_effects()), [] as [Value; 0]);
    assert_eq!(none.stats().stale_input, 1);
}

/// REQ-1557 item 3: the operator is told at most once an interval.
/// REQ-1555 item 3.
#[test]
fn stale_input_logs_once_an_interval() {
    let mut channel = selected();
    let logged = |effects: &[Effect]| effects.iter().filter(|effect| matches!(effect, Effect::Log { event: "stale_input", .. })).count();
    let mut lines = 0;
    for now in 0..5 {
        channel.received(CONNECTION, &message(&[key(OTHER, Action::Press, false)]), now);
        lines += logged(&channel.take_effects());
    }
    assert_eq!(lines, 1, "five within the interval");
    channel.received(CONNECTION, &message(&[key(OTHER, Action::Press, false)]), STALE_INPUT_REPORT_SECONDS * 1_000);
    assert_eq!(logged(&channel.take_effects()), 1, "the next interval's");
    assert_eq!(channel.stats().stale_input, 6);
}

/// REQ-1555 item 4: a release of what is held, stamped with a page no
/// longer selected, reaches the program for the page it names, uncounted.
#[test]
fn a_held_release_with_an_old_stamp_reaches_the_program() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 1);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[key(OTHER, Action::Release, false)]), 2);
    let lines = inputs(&channel.take_effects());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!((&lines[0]["page"], &lines[0]["event"]["action"]), (&json!("200"), &json!("release")));
    assert_eq!(channel.stats().stale_input, 0);
}

/// REQ-1555 item 4: a release of what a change of selection let go already
/// is nothing more, and no drop is counted.
#[test]
fn a_late_release_changes_nothing() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 1);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[select(0x200, 0)]), 2);
    let lost = inputs(&channel.take_effects());
    assert_eq!(lost.len(), 1, "{lost:?}");
    assert_eq!((&lost[0]["page"], &lost[0]["lost"]), (&json!("100"), &json!(true)));
    channel.received(CONNECTION, &message(&[key(ON, Action::Release, false)]), 3);
    assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0]);
    assert_eq!(channel.stats().stale_input, 0);
}

/// REQ-207 item 5: the receiver's L flag reaches the program.
#[test]
fn a_receivers_lost_release_is_marked() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false), key(ON, Action::Release, true)]), 1);
    let lines = inputs(&channel.take_effects());
    assert_eq!((&lines[0]["lost"], &lines[1]["lost"]), (&json!(false), &json!(true)), "{lines:?}");
}

/// REQ-207: `FOCUS_OUT` gives the program what is still held, marked lost,
/// then `blur`.
/// REQ-1555 item 2: a focus event is always acted on.
#[test]
fn focus_out_releases_what_is_held() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 1);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[Item::Event(Event::FocusOut)]), 2);
    let lines = inputs(&channel.take_effects());
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!((&lines[0]["event"]["kind"], &lines[0]["lost"]), (&json!("key"), &json!(true)));
    assert_eq!(lines[1]["event"]["kind"], json!("blur"));
}

/// REQ-207: a pointer's gone lets go of a held button with no click: a
/// release marked lost, at the pointer's last place.
#[test]
fn gone_releases_a_held_button_marked_lost() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[mouse(ON, MouseAction::Press, 0)]), 1);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[mouse(ON, MouseAction::Gone, 1)]), 2);
    let lines = inputs(&channel.take_effects());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!((&lines[0]["event"]["action"], &lines[0]["event"]["column"], &lines[0]["lost"]), (&json!("release"), &json!(7), &json!(true)));
}

/// REQ-207: a program's page redirect is a selection, and lets go too.
#[test]
fn a_page_redirect_releases_what_is_held() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "request": 2, "page": "200", "sub": "3F7F"})), 2);
    let effects = channel.take_effects();
    let order: Vec<String> = told(&effects).iter().map(|line| line["type"].as_str().unwrap().into()).collect();
    assert_eq!(order, ["input", "left", "selected"], "released before the page goes");
    assert_eq!(inputs(&effects)[0]["lost"], json!(true));
}

/// The types of the records on data channel 12 in `bytes` that begin there.
fn record_types(bytes: &[u8]) -> Vec<u8> {
    bytes
        .as_chunks::<PACKET_BYTES>()
        .0
        .iter()
        .filter_map(|bytes| neoteletext_audio::idl::fragment(&Packet { bytes: *bytes })?.ok())
        .filter(|fragment| fragment.index == 0)
        .filter_map(|fragment| fragment.data.get(1).copied())
        .collect()
}

/// What `effects` send `connection`, back to back.
fn sent_to(effects: &[Effect], connection: u64) -> Vec<u8> {
    effects
        .iter()
        .filter_map(|effect| if let Effect::Send { connection: to, bytes } = effect { (*to == connection).then(|| bytes.to_vec()) } else { None })
        .flatten()
        .collect()
}

/// DEC-1689, REQ-1688 item 2: a page redirect reaches the receiver as a
/// REDIRECT record and selects the page, and no cursor record names it.
/// REQ-1472 verification 5, REQ-1473 verification 8, REQ-1502 verification 5, REQ-1032 verification 8.
#[test]
fn a_page_redirect_sends_a_redirect_record_and_selects_the_page() {
    let mut channel = selected();
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "request": 2, "page": "200", "sub": "3F7F"})), 2);
    let effects = channel.take_effects();
    assert_eq!(record_types(&sent_to(&effects, CONNECTION)), [TYPE_REDIRECT], "and no cursor record");
    let lines = told(&effects);
    assert_eq!((&lines.last().unwrap()["type"], &lines.last().unwrap()["page"]), (&json!("selected"), &json!("200")));
}

/// REQ-1688 item 2: a redirect to another channel sends the record, tells
/// the operator, and changes no selection.
/// REQ-1473 verification 8 and REQ-1032 verification 8.
#[test]
fn a_redirect_to_another_channel_changes_no_selection() {
    let mut channel = selected();
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "request": 2, "address": "https://elsewhere.example/channel/"})), 2);
    let effects = channel.take_effects();
    assert_eq!(record_types(&sent_to(&effects, CONNECTION)), [TYPE_REDIRECT]);
    assert!(told(&effects).is_empty(), "the program is told nothing: the viewer has not gone");
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Log { event: "redirect", text, .. } if text.contains("elsewhere.example"))));
    assert_eq!(channel.stats().redirects, 1);
}

/// REQ-207 item 4: a connection lost gives the program the releases the
/// viewer cannot send, before `left`; one reconnecting holds nothing.
#[test]
fn a_lost_connection_releases_what_is_held() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false), mouse(ON, MouseAction::Press, 0)]), 1);
    let _ = channel.take_effects();
    channel.closed(CONNECTION, 2);
    let lines = told(&channel.take_effects());
    assert_eq!((&lines[0]["type"], &lines[0]["lost"], &lines[1]["type"], &lines[1]["lost"]), (&json!("input"), &json!(true), &json!("input"), &json!(true)));
    assert_eq!(lines[2]["type"], json!("left"));
    channel.opened(2, 1, Vec::new(), 3);
    let _ = channel.take_effects();
    channel.closed(2, 4);
    assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0], "the new connection held nothing");
}

/// REQ-1553: losing the focus changes neither the selection nor what is
/// sent: the selected page's next version still arrives.
/// REQ-1472 verification 9.
#[test]
fn a_selected_page_stays_active_without_the_focus() {
    let mut channel = selected();
    let page = json!({"type": "page", "connections": "all", "page": subpage()});
    channel.from_program(&line(&json!({"type": "page", "connections": [CONNECTION], "request": 2, "page": subpage()})), 1);
    assert_eq!(sends(&channel.take_effects()), 1);
    channel.received(CONNECTION, &message(&[Item::Event(Event::FocusOut), mouse(ON, MouseAction::Gone, 0)]), 2);
    let lines = told(&channel.take_effects());
    assert!(!lines.iter().any(|line| line["type"] == "left" || line["type"] == "selected"), "{lines:?}");
    let spaces = page["page"]["rows"][0].clone();
    channel.from_program(&line(&with(&page, "/page/rows/5", spaces)), 3);
    assert_eq!(sends(&channel.take_effects()), 1, "the change is sent with the focus gone");
}

/// INPUT.md section 7.1: a message of no input units is never acknowledged.
#[test]
fn a_selection_is_not_acknowledged() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[select(0x200, 0)]), 1);
    assert_eq!(sends(&channel.take_effects()), 0);
    channel.received(CONNECTION, &message(&[key(OTHER, Action::Press, false)]), 2);
    let _ = channel.take_effects();
    channel.deadline(2 + ACKNOWLEDGEMENT_WAIT_MS);
    assert_eq!(record_types(&sent_to(&channel.take_effects(), CONNECTION)), [TYPE_CURSOR], "a key is");
}

/// A sub-page that answers `selected()`'s selection and shows a change later.
fn answered() -> Channel {
    let mut channel = selected();
    channel.from_program(&line(&json!({"type": "page", "connections": [CONNECTION], "request": 2, "page": subpage()})), 0);
    let _ = channel.take_effects();
    channel
}

/// REQ-1334: a key the page answers costs one cursor record, in the answer's
/// frame, and none alone.
#[test]
fn an_answer_carries_the_acknowledgement() {
    let mut channel = answered();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 1);
    assert_eq!(sends(&channel.take_effects()), 0, "nothing goes before the answer");
    assert_eq!(channel.next_wake(), Some(1 + ACKNOWLEDGEMENT_WAIT_MS));
    let spaces = subpage()["rows"][0].clone();
    channel.from_program(&line(&with(&json!({"type": "page", "connections": "all", "page": subpage()}), "/page/rows/5", spaces)), 2);
    let frame = sent_to(&channel.take_effects(), CONNECTION);
    assert_eq!(record_types(&frame), [TYPE_LINKS, TYPE_CURSOR], "the frame carries it, after its links");
    assert_eq!(channel.next_wake(), None, "and nothing is left to send alone");
}

/// REQ-1334: with nothing published the record goes alone, once however many
/// keys it covers, within the wait of the first of them.
#[test]
fn an_acknowledgement_goes_alone_when_nothing_is_published() {
    let mut channel = answered();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 1);
    channel.received(CONNECTION, &message(&[key(ON, Action::Release, false)]), 2);
    assert_eq!(channel.next_wake(), Some(1 + ACKNOWLEDGEMENT_WAIT_MS), "the first key sets the time");
    let _ = channel.take_effects();
    channel.deadline(ACKNOWLEDGEMENT_WAIT_MS);
    assert_eq!(sends(&channel.take_effects()), 0, "not yet");
    channel.deadline(1 + ACKNOWLEDGEMENT_WAIT_MS);
    assert_eq!(record_types(&sent_to(&channel.take_effects(), CONNECTION)), [TYPE_CURSOR]);
    assert_eq!(channel.next_wake(), None);
}

/// REQ-1346: a cursor `INPUT.md` section 7.1 forbids is refused when the
/// program sends it, and input is still acknowledged.
#[test]
fn a_cursor_off_its_rows_is_refused_and_input_is_still_acknowledged() {
    for row in [0, 24] {
        let mut channel = selected();
        let page = json!({"type": "page", "connections": [CONNECTION], "request": 2, "page": subpage()});
        channel.from_program(&line(&with(&page, "/page/cursor/row", json!(row))), 1);
        let effects = channel.take_effects();
        assert_eq!(sends(&effects), 0, "row {row}");
        assert_eq!((&told(&effects)[0]["type"], &told(&effects)[0]["rule"]), (&json!("refused"), &json!("out_of_range")), "row {row}");
        channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 2);
        let _ = channel.take_effects();
        channel.deadline(2 + ACKNOWLEDGEMENT_WAIT_MS);
        assert_eq!(record_types(&sent_to(&channel.take_effects(), CONNECTION)), [TYPE_CURSOR], "row {row}");
    }
}

/// REQ-1346: whatever a cursor line says, the acknowledgement has its record:
/// a place the engine would not encode is no cursor, and carries no flags.
#[test]
fn an_acknowledgement_never_depends_on_the_cursor_it_goes_with() {
    use neoteletext_formats::page::CursorLine;
    use neoteletext_input::cursor::{BLOCK, NO_CURSOR};
    for row in [0, 24, NO_CURSOR] {
        let line = CursorLine { row, column: 3, flags: BLOCK, acked: 0, line: 0 };
        let cursor = super::cursor_of(0x100, 7, Some(&line));
        assert_eq!((cursor.place, cursor.flags, cursor.acked), (None, 0, 7), "row {row}");
        assert!(cursor.packet(super::SERVICE, 0).is_some(), "row {row}");
    }
}

/// REQ-1471: a receiver with no SELECT is shown the program's answer to
/// `connected`, and raw text is for that page.
#[test]
fn a_receiver_without_select_is_shown_the_initial_page() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello()]), 0);
    let _ = channel.take_effects();
    let magazines = json!([{"magazine": 1, "name": "News", "pages": [{"page": "100", "title": "Front", "section": null, "subpages": 1}]}]);
    channel.from_program(&line(&json!({"type": "reply", "connection": CONNECTION, "request": 1, "magazines": magazines, "selected": subpage()})), 1);
    assert!(sends(&channel.take_effects()) > 0);
    channel.received(CONNECTION, &message(&[Item::Text("hi")]), 2);
    let lines = inputs(&channel.take_effects());
    assert_eq!((&lines[0]["page"], &lines[0]["sub"]), (&json!("100"), &json!("0000")), "{lines:?}");
}

/// REQ-1481, DEC-1730: a program that does not answer a selection in time
/// leaves the viewer told so, with status 503 and the place, and the
/// connection stays open.
#[test]
fn a_program_that_does_not_answer_gets_the_viewer_a_503() {
    use neoteletext_web::records::denied::STATUS_UNAVAILABLE;
    use neoteletext_web::records::{ChannelRecord, Read, RecordReader};

    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello(), select_with(0x302, 2, "")]), 1);
    channel.take_effects();
    channel.deadline(1 + PROGRAM_REPLY_LIMIT_MS);
    let bytes = sent_to(&channel.take_effects(), CONNECTION);
    let mut reader = RecordReader::new();
    let mut denied = None;
    for packet in bytes.as_chunks::<PACKET_BYTES>().0 {
        let packet = Packet { bytes: *packet };
        if let Read::Record(ChannelRecord::Denied(record)) = reader.push(&packet) {
            denied = Some((record.status, record.place.to_string()));
        }
    }
    assert_eq!(denied, Some((STATUS_UNAVAILABLE, "302/0002".to_string())));
}

/// DEC-1730: `selected` carries the place as an address names it.
/// REQ-1674 item 2: `selected` carries the query, with `path`.
#[test]
fn selected_names_the_place() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello(), select_with(0x302, 2, "a=1")]), 1);
    let lines = told(&channel.take_effects());
    let selected = lines.iter().find(|line| line["type"] == "selected").unwrap();
    assert_eq!((&selected["path"], &selected["query"]), (&json!("302/0002"), &json!("a=1")));
}

/// The `selected` lines the program was told, as page and parameters.
fn selections(effects: &[Effect]) -> Vec<(Value, Value)> {
    told(effects).iter().filter(|line| line["type"] == "selected").map(|line| (line["page"].clone(), line["query"].clone())).collect()
}

/// REQ-1693 item 6: a page's parameters reach the program with their
/// selection, and choosing anything else leaves them behind.
/// REQ-1674: the query belongs to its selection alone.
#[test]
fn page_parameters_go_with_their_selection_alone() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello()]), 0);
    let _ = channel.take_effects();
    let parameters = "a=1&b=%20x";
    channel.received(CONNECTION, &message(&[select_with(0x302, ANY, parameters)]), 0);
    assert_eq!(selections(&channel.take_effects()), [(json!("302"), json!(parameters))]);
    channel.received(CONNECTION, &message(&[select(0x303, ANY)]), 0);
    assert_eq!(selections(&channel.take_effects()), [(json!("303"), json!(null))]);
    channel.received(CONNECTION, &message(&[select(0x302, ANY)]), 0);
    assert_eq!(selections(&channel.take_effects()), [(json!("302"), json!(null))]);
}

/// REQ-1693 item 6: parameters are part of the selection, so the same page
/// with others is told, and the same again is not.
/// REQ-1090: a selection is its page and its query.
#[test]
fn the_same_page_with_other_parameters_is_a_selection() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello(), select_with(0x302, ANY, "a=1")]), 0);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[select_with(0x302, ANY, "a=1")]), 0);
    assert_eq!(selections(&channel.take_effects()), []);
    channel.received(CONNECTION, &message(&[select_with(0x302, ANY, "a=2")]), 0);
    assert_eq!(selections(&channel.take_effects()), [(json!("302"), json!("a=2"))]);
}

/// A program that starts again is told the selection with its parameters.
/// REQ-1674 item 2 and REQ-1034: `connected` tells a program that starts again.
#[test]
fn connected_carries_the_selections_parameters() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello(), select_with(0x302, ANY, "a=1")]), 0);
    channel.program_ended();
    let _ = channel.take_effects();
    channel.program_started(1);
    let lines = told(&channel.take_effects());
    let connected = lines.iter().find(|line| line["type"] == "connected").unwrap();
    assert_eq!((&connected["selection"]["page"], &connected["query"]), (&json!("302"), &json!("a=1")));
}

/// A redirect is a selection of its own, which leaves parameters behind.
#[test]
fn a_page_redirect_leaves_parameters_behind() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello(), select_with(0x302, ANY, "a=1")]), 0);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "request": 2, "page": "303", "sub": "3F7F"})), 0);
    assert_eq!(selections(&channel.take_effects()), [(json!("303"), json!(null))]);
}

/// REQ-1693 item 2: the push address is fixed, so `channel.ttx` never names one.
#[test]
fn the_descriptor_names_no_push_address() {
    let descriptor: Value = serde_json::from_slice(&channel().descriptor()).unwrap();
    assert_eq!(descriptor["push"], json!({"input": true}));
}

/// The `connected` lines among `effects`.
fn connecteds(effects: &[Effect]) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == "connected").collect()
}

/// TODO-1731 item 1: `connected` waits for the first SELECT, and carries it,
/// so a shared link goes straight to its page, with no page before it and
/// no `selected` after.
/// REQ-1090: the first SELECT.
#[test]
fn connected_waits_for_the_first_selection() {
    let mut channel = held();
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0], "nothing yet");
    channel.received(CONNECTION, &message(&[hello()]), 0);
    assert_eq!(connecteds(&channel.take_effects()), [] as [Value; 0], "a receiver that can select is waited for");
    channel.received(CONNECTION, &message(&[select_with(0x302, ANY, "a=1")]), 100);
    let lines = told(&channel.take_effects());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["type"], json!("connected"));
    assert_eq!((&lines[0]["path"], &lines[0]["query"], &lines[0]["request"]), (&json!("302"), &json!("a=1"), &json!(1)));
}

/// TODO-1731 item 1: the wait has a bound, the channel's wake says when, and
/// a connection that selected nothing by then is told with no selection.
/// REQ-1090: the first SELECT's bound.
#[test]
fn the_wait_for_a_first_selection_is_bounded() {
    let mut channel = held();
    assert_eq!(channel.next_wake(), Some(FIRST_SELECTION_WAIT_MS));
    channel.deadline(FIRST_SELECTION_WAIT_MS - 1);
    assert_eq!(connecteds(&channel.take_effects()), [] as [Value; 0]);
    channel.deadline(FIRST_SELECTION_WAIT_MS);
    let lines = connecteds(&channel.take_effects());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["selection"], Value::Null);
}

/// TODO-1731 item 1: a receiver that cannot send SELECT is not waited for.
/// REQ-1090: a receiver without the capability.
#[test]
fn a_receiver_that_cannot_select_is_not_waited_for() {
    let mut channel = held();
    let hello = Item::Event(Event::Hello { version: VERSION, caps: CURSOR_CAP, agent: None });
    channel.received(CONNECTION, &message(&[hello]), 0);
    assert_eq!(connecteds(&channel.take_effects()).len(), 1);
}

/// REQ-1471: the cookies a connection opened with reach its program, however
/// often it is told.
#[test]
fn connected_carries_the_cookies_the_connection_opened_with() {
    let config =
        ChannelConfig { name: "Test".into(), address: "http://127.0.0.1/test/".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut channel = Channel::new(config);
    channel.program_started(0);
    let mut cookies = Vec::new();
    crate::session_cookie::program_cookies(b"_session=1.2.3; theme=dark; =nameless; broken; a=b=c", &mut cookies);
    channel.opened(CONNECTION, 1, cookies, 0);
    channel.received(CONNECTION, &message(&[select(0x100, ANY)]), 0);
    let expected = json!([{"name": "theme", "value": "dark"}, {"name": "a", "value": "b=c"}]);
    assert_eq!(connecteds(&channel.take_effects())[0]["cookies"], expected, "and never the session cookie");
    channel.program_ended();
    channel.program_started(1);
    assert_eq!(connecteds(&channel.take_effects())[0]["cookies"], expected, "a program that starts again hears them too");
}

/// DEC-1759 lesson 13: each selection has the next number, and only the
/// connection's newest may answer: a late page does not replace what the
/// viewer moved to, and a page for the newest does.
/// REQ-1476: a selection's number.
#[test]
fn a_late_answer_cannot_replace_the_page_on_screen() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[select_with(0x100, ANY, "a=1")]), 1);
    assert_eq!(told(&channel.take_effects())[0]["request"], json!(2));
    channel.received(CONNECTION, &message(&[select_with(0x100, ANY, "a=2")]), 2);
    assert_eq!(told(&channel.take_effects()).last().unwrap()["request"], json!(3));
    let answer = |request: u64| line(&json!({"type": "page", "connections": [CONNECTION], "request": request, "page": subpage()}));
    channel.from_program(&answer(2), 3);
    assert_eq!(sends(&channel.take_effects()), 0, "the first selection's answer came late");
    assert_eq!(channel.stats().stale_answers, 1);
    assert_eq!(channel.stats().refused, 0, "the program did nothing wrong");
    channel.from_program(&answer(3), 4);
    assert_eq!(sends(&channel.take_effects()), 1, "the second's goes");
}

/// DEC-1759 lesson 13: a late redirect does not move a viewer who chose
/// something else, and a late `subpages` and `denied` change nothing.
/// REQ-1476 and REQ-1688: a late redirect is dropped.
#[test]
fn a_late_redirect_or_refusal_changes_nothing() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[select(0x200, ANY)]), 1);
    channel.received(CONNECTION, &message(&[select(0x300, ANY)]), 2);
    let _ = channel.take_effects();
    let late = [
        json!({"type": "redirect", "connection": CONNECTION, "request": 2, "page": "400", "sub": "3F7F"}),
        json!({"type": "denied", "connection": CONNECTION, "request": 2, "status": 404}),
        json!({"type": "subpages", "connections": [CONNECTION], "request": 2, "page": "300", "version": 1, "subpages": [{"sub": "0000", "title": null}]}),
    ];
    for message in &late {
        channel.from_program(&line(message), 3);
        assert_eq!(channel.take_effects().iter().filter(|effect| matches!(effect, Effect::Send { .. } | Effect::ToProgram { .. })).count(), 0, "{message}");
    }
    assert_eq!(channel.stats().stale_answers, 3);
    assert_eq!(channel.next_wake(), Some(2 + PROGRAM_REPLY_LIMIT_MS), "the newest is still awaited");
}

/// DEC-1759 lesson 13: a reply to a selection already replaced still gives
/// the index, and its sub-page answers nothing.
/// REQ-1476: a reply numbered for a replaced selection.
#[test]
fn a_late_reply_gives_the_index_and_no_page() {
    let mut channel = held();
    channel.received(CONNECTION, &message(&[hello(), select(0x100, ANY)]), 0);
    channel.received(CONNECTION, &message(&[select(0x200, ANY)]), 1);
    let _ = channel.take_effects();
    let magazines = json!([{"magazine": 1, "name": "News", "pages": [{"page": "100", "title": "Front", "section": null, "subpages": 1}]}]);
    channel.from_program(&line(&json!({"type": "reply", "connection": CONNECTION, "request": 1, "magazines": magazines, "selected": subpage()})), 2);
    let effects = channel.take_effects();
    assert_eq!(sends(&effects), 1, "the index, and no page");
    assert_eq!(channel.stats().stale_answers, 1);
    assert_eq!(channel.stats().pages_sent, 0);
}

/// DEC-1759 lesson 13: a change is for what is shown: it does not answer a
/// selection, so a connection still awaiting one is sent only its answer.
/// REQ-1476: a change does not answer a selection.
#[test]
fn an_unnumbered_change_does_not_answer_a_selection() {
    let mut channel = selected();
    let change = line(&json!({"type": "page", "connections": "all", "page": subpage()}));
    channel.from_program(&change, 1);
    assert_eq!(sends(&channel.take_effects()), 0);
    channel.from_program(&line(&json!({"type": "page", "connections": [CONNECTION], "request": 2, "page": subpage()})), 2);
    assert_eq!(sends(&channel.take_effects()), 1);
    channel.from_program(&change, 3);
    assert_eq!(channel.stats().pages_sent, 1, "the same again changes nothing, and passes");
}

/// DEC-1759 lesson 12: a selection ends with a page, with DENIED from the
/// program at once, or at the deadline: here with DENIED.
/// REQ-1476: `denied`.
#[test]
fn a_program_may_end_a_selection_with_a_status() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[select(0x8FC, ANY)]), 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "denied", "connection": CONNECTION, "request": 2, "status": 404})), 2);
    assert_eq!(denied_in(&sent_to(&channel.take_effects(), CONNECTION)), Some((404, String::from("8FC"))));
    assert_eq!(channel.next_wake(), None, "and no deadline is left");
}

/// The status and place of the DENIED record in `bytes`.
fn denied_in(bytes: &[u8]) -> Option<(u16, String)> {
    use neoteletext_web::records::{ChannelRecord, Read, RecordReader};
    let mut reader = RecordReader::new();
    bytes.as_chunks::<PACKET_BYTES>().0.iter().find_map(|packet| match reader.push(&Packet { bytes: *packet }) {
        Read::Record(ChannelRecord::Denied(record)) => Some((record.status, record.place.into())),
        _ => None,
    })
}

/// `INPUT.md` section 5.4: every selection has an ending, so a SELECT that
/// repeats one DENIED ended is asked of the program again and has its own,
/// as `neottx probe <channel>8FC` needs (REQ-1882). It leaves no page.
#[test]
fn a_selection_repeated_after_denied_has_its_own_ending() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[select(0x8FC, ANY)]), 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "denied", "connection": CONNECTION, "request": 2, "status": 404})), 2);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[select(0x8FC, ANY)]), 3);
    let lines = told(&channel.take_effects());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!((&lines[0]["type"], &lines[0]["request"], &lines[0]["path"]), (&json!("selected"), &json!(3), &json!("8FC")), "{lines:?}");
    assert_eq!(channel.next_wake(), Some(3 + PROGRAM_REPLY_LIMIT_MS));
    channel.from_program(&line(&json!({"type": "denied", "connection": CONNECTION, "request": 3, "status": 404})), 4);
    assert_eq!(denied_in(&sent_to(&channel.take_effects(), CONNECTION)), Some((404, String::from("8FC"))));
}

/// `INPUT.md` section 5.4: the deadline's DENIED 503 is an ending too, and
/// with no program running the repeat ends at the deadline it is given.
#[test]
fn a_selection_repeated_after_the_deadline_has_its_own_ending() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[select(0x8FC, ANY)]), 1);
    channel.deadline(1 + PROGRAM_REPLY_LIMIT_MS);
    let _ = channel.take_effects();
    channel.program_ended();
    channel.received(CONNECTION, &message(&[select(0x8FC, ANY)]), 2 + PROGRAM_REPLY_LIMIT_MS);
    channel.deadline(2 + PROGRAM_REPLY_LIMIT_MS);
    assert_eq!(denied_in(&sent_to(&channel.take_effects(), CONNECTION)), Some((503, String::from("8FC"))));
}

/// `INPUT.md` section 15.6: while a selection stands, a SELECT that repeats
/// it is no new selection: not while it is awaited, and not once it is
/// shown, nor when DENIED was followed by a page, which ended it again.
#[test]
fn a_selection_that_stands_is_not_asked_again() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[select(0x100, ANY)]), 1);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[select(0x100, ANY)]), 2);
    assert_eq!(told(&channel.take_effects()), Vec::<Value>::new(), "while awaited");
    channel.from_program(&line(&json!({"type": "denied", "connection": CONNECTION, "request": 2, "status": 503})), 3);
    channel.from_program(&line(&json!({"type": "page", "connections": [CONNECTION], "request": 2, "page": subpage()})), 4);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[select(0x100, 0)]), 5);
    assert_eq!(told(&channel.take_effects()), Vec::<Value>::new(), "the page DENIED was followed by stands");
}

/// REQ-1481, DEC-1759 lesson 12: a selection with no program running ends at
/// once, and one the program went away from keeps its deadline.
#[test]
fn a_selection_has_an_ending_with_no_program_too() {
    let config =
        ChannelConfig { name: "Test".into(), address: "http://127.0.0.1/test/".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut none = Channel::new(config);
    none.opened(CONNECTION, 1, Vec::new(), 0);
    none.received(CONNECTION, &message(&[select(0x302, ANY)]), 10);
    assert_eq!(none.next_wake(), Some(10), "at once");
    none.deadline(10);
    assert_eq!(sends(&none.take_effects()), 1, "DENIED 503");
    let mut gone = selected();
    gone.program_ended();
    assert_eq!(gone.next_wake(), Some(PROGRAM_REPLY_LIMIT_MS));
    gone.deadline(PROGRAM_REPLY_LIMIT_MS);
    assert_eq!(sends(&gone.take_effects()), 1, "DENIED 503");
}

/// A redirect to another channel is a selection's answer: no 503 follows it.
/// REQ-1688 item 2: no DENIED follows.
#[test]
fn a_redirect_elsewhere_ends_the_selection() {
    let mut channel = selected();
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "request": 2, "address": "https://elsewhere.example/channel/"})), 2);
    assert_eq!(channel.next_wake(), None);
}

/// TODO-1731 item 5: a page redirect keeps the query it is given: the record
/// names it, and the selection it makes has it.
/// REQ-1688: a page redirect keeps its query.
#[test]
fn a_page_redirect_keeps_the_query_it_is_given() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[select_with(0x302, 9, "a=1")]), 0);
    let _ = channel.take_effects();
    let redirect = json!({"type": "redirect", "connection": CONNECTION, "request": 2, "page": "302", "sub": "3F7F", "query": "a=1"});
    channel.from_program(&line(&redirect), 1);
    let effects = channel.take_effects();
    assert_eq!(selections(&effects), [(json!("302"), json!("a=1"))]);
    let mut reader = neoteletext_web::records::RecordReader::new();
    let bytes = sent_to(&effects, CONNECTION);
    let carried = bytes.as_chunks::<PACKET_BYTES>().0.iter().any(|packet| {
        matches!(
            reader.push(&Packet { bytes: *packet }),
            neoteletext_web::records::Read::Record(neoteletext_web::records::ChannelRecord::Redirect(
                neoteletext_web::records::redirect::RedirectRecord::Page { parameters: Some("a=1"), .. }
            ))
        )
    });
    assert!(carried, "the receiver's SELECT keeps it");
}

/// INPUT.md section 6.1: another version is told to stop.
/// REQ-1036: another version closes with its own stop code.
#[test]
fn another_version_is_closed_with_a_stop_code() {
    let mut channel = held();
    let hello = Item::Event(Event::Hello { version: VERSION - 1, caps: SELECT_CAP, agent: None });
    channel.received(CONNECTION, &message(&[hello]), 0);
    assert_eq!(channel.take_effects(), [Effect::Close { connection: CONNECTION, code: CLOSE_WRONG_VERSION }]);
}

/// DEC-1759 lesson 16: an unknown extension is skipped, so the connection
/// stays and what follows it is read; an unknown core event, a reserved type
/// and a message cut short are protocol errors, closed with the stop code.
/// REQ-1036: a protocol error closes with the stop code.
#[test]
fn unknown_events_are_skipped_or_refused_as_the_protocol_says() {
    const UNKNOWN_EXTENSION: [u8; 4] = [0xFF, 0x7F, 1, 0];
    let mut channel = selected();
    let mut skipped = UNKNOWN_EXTENSION.to_vec();
    skipped.extend_from_slice(&message(&[key(ON, Action::Press, false)]));
    channel.received(CONNECTION, &skipped, 1);
    let effects = channel.take_effects();
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Close { .. })), "{effects:?}");
    assert_eq!(inputs(&effects).len(), 1, "what follows it is read");
    for refused in [&[0xFF, 0x0B][..], &[0xFF, 0x80], &[0xFF, 0x01], &[0xE2, 0x82, 0xFF, 0x05], &[0xE2, 0x82]] {
        let mut channel = selected();
        channel.received(CONNECTION, refused, 1);
        assert!(channel.take_effects().contains(&Effect::Close { connection: CONNECTION, code: CLOSE_BROKE_PROTOCOL }), "{refused:02X?}");
    }
}

/// DEC-1759 lesson 17: a message holds whole items, so the next message's
/// bytes can never finish one: a key cut in two is refused at the cut.
/// REQ-1036: a protocol error closes with the stop code.
#[test]
fn an_item_cut_by_a_message_is_refused() {
    let whole = message(&[key(ON, Action::Press, false)]);
    let (start, _) = whole.split_at(whole.len() - 1);
    let mut channel = selected();
    channel.received(CONNECTION, start, 1);
    assert!(channel.take_effects().contains(&Effect::Close { connection: CONNECTION, code: CLOSE_BROKE_PROTOCOL }));
}

/// INPUT.md section 6.2: a capability nobody defined yet is ignored: WELCOME
/// says only what the channel uses.
#[test]
fn an_unknown_capability_is_not_agreed() {
    use neoteletext_input::control::{Control, FRAGMENT_DATA, from_packet};
    const UNDEFINED_CAPABILITY: u32 = 1 << 30;
    let mut channel = held();
    let hello = Item::Event(Event::Hello { version: VERSION, caps: SELECT_CAP | UNDEFINED_CAPABILITY, agent: None });
    channel.received(CONNECTION, &message(&[hello]), 0);
    let bytes = sent_to(&channel.take_effects(), CONNECTION);
    let mut buffer = [0; FRAGMENT_DATA];
    let packet = Packet { bytes: bytes.as_slice().try_into().unwrap() };
    assert_eq!(from_packet(&packet, &mut buffer), Some(Ok(Control::Welcome { version: VERSION, caps: SELECT_CAP, resume: None })));
}

/// DEC-1759 lesson 19: capabilities are the only signal: what a receiver
/// calls itself changes nothing the channel does.
#[test]
fn a_receivers_agent_text_changes_nothing() {
    let run = |agent: Option<&'static str>| {
        let mut channel = held();
        let caps = CURSOR_CAP | SELECT_CAP | if agent.is_some() { neoteletext_input::caps::AGENT } else { 0 };
        channel.received(CONNECTION, &message(&[Item::Event(Event::Hello { version: VERSION, caps, agent }), select(0x302, ANY)]), 0);
        channel.take_effects()
    };
    let told = |agent| format!("{:?}", run(agent));
    assert_eq!(told(Some("NeoTeletext/0.1 (macOS)")), told(Some("Mozilla/5.0 (a web browser)")));
    assert_eq!(told(Some("NeoTeletext/0.1 (macOS)")), told(None));
}

/// A row's value as bytes.
fn vector_bytes(value: &str) -> Vec<u8> {
    let (bytes, length) = neoteletext_web::wire_vectors::bytes_of(value).unwrap();
    bytes[..length].to_vec()
}

/// DEC-1759 lesson 3: every row of the shared table, against the channel as
/// a whole: what a receiver sends does as the table says, and what the
/// channel sends is byte for byte the table's.
/// REQ-1090: the table's `select_*` rows, accepted and refused.
#[test]
fn every_row_of_the_wire_vectors_holds() {
    use neoteletext_web::consts::{CLOSE_RESTARTING, PUSH_SUBPROTOCOL, close_says_stop};
    use neoteletext_web::wire_vectors::vectors;
    let row = |name: &str| vectors().find(|vector| vector.name == name).unwrap();
    for vector in vectors() {
        match (vector.kind, vector.name) {
            ("version", "input") => assert_eq!(vector.value, VERSION.to_string()),
            ("version", _) => assert_eq!(vector.value, PUSH_SUBPROTOCOL),
            ("close", name) => {
                let code: u16 = vector.value.parse().unwrap();
                assert_eq!(close_says_stop(code), vector.meaning == "stop", "{name}");
                let ours = [("broke_protocol", CLOSE_BROKE_PROTOCOL), ("wrong_version", CLOSE_WRONG_VERSION), ("restarting", CLOSE_RESTARTING)];
                if let Some((_, ours)) = ours.iter().find(|(named, _)| *named == name) {
                    assert_eq!(code, *ours, "{name}");
                }
            }
            ("input", name) => {
                let mut channel = held();
                channel.received(CONNECTION, &vector_bytes(vector.value), 0);
                let effects = channel.take_effects();
                assert!(!effects.iter().any(|effect| matches!(effect, Effect::Close { .. })), "{name} is read: {effects:?}");
                if let Some(selection) = vector.meaning.strip_prefix("select ") {
                    let connected = &connecteds(&effects)[0];
                    let (path, query) = selection.split_once('?').map_or((selection, Value::Null), |(path, query)| (path, json!(query)));
                    assert_eq!((&connected["path"], &connected["query"]), (&json!(path), &query), "{name}");
                }
            }
            ("refused", name) => {
                let mut channel = held();
                channel.received(CONNECTION, &vector_bytes(vector.value), 0);
                assert!(channel.take_effects().contains(&Effect::Close { connection: CONNECTION, code: CLOSE_BROKE_PROTOCOL }), "{name}");
            }
            _ => {}
        }
    }
    // What the channel writes, on a connection with nothing else sent.
    let mut channel = held();
    channel.received(CONNECTION, &vector_bytes(row("hello").value), 0);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), vector_bytes(row("welcome").value));
    let mut channel = held();
    channel.received(CONNECTION, &vector_bytes(row("select_sub_page").value), 0);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "denied", "connection": CONNECTION, "request": 1, "status": 404})), 1);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), vector_bytes(row("denied_not_found").value));
    let mut channel = held();
    channel.deadline(FIRST_SELECTION_WAIT_MS);
    channel.deadline(FIRST_SELECTION_WAIT_MS + PROGRAM_REPLY_LIMIT_MS);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), vector_bytes(row("denied_channel").value));
    for (name, redirect) in [
        ("redirect_place", json!({"page": "302", "sub": "3F7F"})),
        ("redirect_keeps_query", json!({"page": "302", "sub": "0002", "query": "a=1"})),
        ("redirect_address", json!({"address": "https://b.example/news/"})),
    ] {
        let mut channel = held();
        channel.deadline(FIRST_SELECTION_WAIT_MS);
        let _ = channel.take_effects();
        let mut message = redirect;
        message["type"] = json!("redirect");
        message["connection"] = json!(CONNECTION);
        message["request"] = json!(1);
        channel.from_program(&line(&message), FIRST_SELECTION_WAIT_MS);
        assert_eq!(sent_to(&channel.take_effects(), CONNECTION), vector_bytes(row(name).value), "{name}");
    }
}
