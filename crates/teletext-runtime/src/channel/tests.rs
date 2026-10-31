//! A connection's input against its selection: PAGE withdrawn, every stamp
//! checked, a release never dropped, a selected page active whatever its
//! focus, and held input released when the page loses it (neoteletext-
//! common's DEC-1538; REQ-1472, REQ-1553, REQ-1555, REQ-1557, REQ-207).

extern crate std;

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use neoteletext_input::VERSION;
use neoteletext_input::caps::{CURSOR_CAP, SELECT_CAP};
use neoteletext_input::event::{Action, Device, Event, Item, Key, KeyCode, Mouse, MouseAction, Point, Select, Stamp, write};
use neoteletext_web::records::TYPE_REDIRECT;
use serde_json::{Value, json};

use super::{Channel, ChannelConfig, Effect, STALE_INPUT_REPORT_SECONDS};
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
/// `a`, by key.
const A: KeyCode = KeyCode::Character('a');

fn channel() -> Channel {
    let config =
        ChannelConfig { name: "Test".into(), push_address: "ws://127.0.0.1/test/push".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut channel = Channel::new(config);
    channel.program_started(0);
    channel.opened(CONNECTION, 1, 0);
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
    Item::Event(Event::Select(Select::new(page, sub).unwrap()))
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
    assert_eq!(channel.take_effects(), [Effect::Close { connection: CONNECTION }]);
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
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "page": "200", "sub": "3F7F"})), 2);
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
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "page": "200", "sub": "3F7F"})), 2);
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
    channel.from_program(&line(&json!({"type": "redirect", "connection": CONNECTION, "address": "https://elsewhere.example/channel/"})), 2);
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
    channel.opened(2, 1, 3);
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
    channel.from_program(&line(&page), 1);
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
    channel.from_program(&line(&json!({"type": "reply", "connection": CONNECTION, "magazines": magazines, "selected": subpage()})), 1);
    assert!(sends(&channel.take_effects()) > 0);
    channel.received(CONNECTION, &message(&[Item::Text("hi")]), 2);
    let lines = inputs(&channel.take_effects());
    assert_eq!((&lines[0]["page"], &lines[0]["sub"]), (&json!("100"), &json!("0000")), "{lines:?}");
}
