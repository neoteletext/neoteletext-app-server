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
use neoteletext_input::event::{Action, Device, Event, Item, Key, KeyCode, Mouse, MouseAction, Point, Select, Stamp, write};
use neoteletext_web::consts::{CLOSE_BROKE_PROTOCOL, CLOSE_WRONG_VERSION};
use neoteletext_web::records::TYPE_REDIRECT;
use serde_json::{Value, json};

use super::{Channel, ChannelConfig, Effect, FIRST_SELECTION_WAIT_MS, PROGRAM_REPLY_LIMIT_MS, STALE_INPUT_REPORT_SECONDS};
use crate::protocol::tests::{line, subpage, with};

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

/// A channel with the connection welcomed and on page 100, sub-code 0000.
fn selected() -> Channel {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[hello(), select(0x100, 0)]), 0);
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
#[test]
fn page_closes_the_session() {
    let mut channel = selected();
    channel.received(CONNECTION, &[0xFF, 0x0A, 0x80, 0x02], 1);
    assert_eq!(channel.take_effects(), [Effect::Close { connection: CONNECTION, code: CLOSE_BROKE_PROTOCOL }]);
}

/// REQ-1557 item 2: a press stamped with the selection reaches the program
/// with the stamp's page and sub-code.
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
    assert_eq!(sends(&channel.take_effects()), 1, "a key is");
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
#[test]
fn a_program_may_end_a_selection_with_a_status() {
    use neoteletext_web::records::{ChannelRecord, Read, RecordReader};
    let mut channel = channel();
    channel.received(CONNECTION, &message(&[select(0x8FC, ANY)]), 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type": "denied", "connection": CONNECTION, "request": 2, "status": 404})), 2);
    let bytes = sent_to(&channel.take_effects(), CONNECTION);
    let mut reader = RecordReader::new();
    let denied = bytes.as_chunks::<PACKET_BYTES>().0.iter().find_map(|packet| match reader.push(&Packet { bytes: *packet }) {
        Read::Record(ChannelRecord::Denied(record)) => Some((record.status, record.place.into())),
        _ => None,
    });
    assert_eq!(denied, Some((404, String::from("8FC"))));
    assert_eq!(channel.next_wake(), None, "and no deadline is left");
}

/// REQ-1481, DEC-1759 lesson 12: a selection with no program running ends at
/// the deadline, and one the program went away from keeps its own.
#[test]
fn a_selection_ends_at_the_deadline_with_no_program_too() {
    let config =
        ChannelConfig { name: "Test".into(), address: "http://127.0.0.1/test/".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut none = Channel::new(config);
    none.opened(CONNECTION, 1, Vec::new(), 0);
    none.received(CONNECTION, &message(&[select(0x302, ANY)]), 10);
    assert_eq!(sends(&none.take_effects()), 0);
    assert_eq!(none.next_wake(), Some(10 + PROGRAM_REPLY_LIMIT_MS));
    none.deadline(10 + PROGRAM_REPLY_LIMIT_MS);
    assert_eq!(sends(&none.take_effects()), 1, "DENIED 503");
    let mut gone = selected();
    gone.program_ended();
    assert_eq!(gone.next_wake(), Some(PROGRAM_REPLY_LIMIT_MS));
    gone.deadline(PROGRAM_REPLY_LIMIT_MS);
    assert_eq!(sends(&gone.take_effects()), 1, "DENIED 503");
}

/// A redirect to another channel is a selection's answer: no 503 follows it.
#[test]
fn a_redirect_elsewhere_ends_the_selection() {
    let mut channel = selected();
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "request": 2, "address": "https://elsewhere.example/channel/"})), 2);
    assert_eq!(channel.next_wake(), None);
}

/// TODO-1731 item 5: a page redirect keeps the query it is given: the record
/// names it, and the selection it makes has it.
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
