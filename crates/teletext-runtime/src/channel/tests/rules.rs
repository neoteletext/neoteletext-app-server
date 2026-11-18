//! The page rule table on a channel (neoteletext-common
//! `docs/page-rules.md` RULE-37 to RULE-40, RULE-46 to RULE-48, RULE-53;
//! REQ-6532): the tables a connection is sent and when, what a receiver's
//! reports become for the program, the state query, and the faults.

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_input::event::{Action, Device, Event, Item, Mouse, MouseAction, Point, Stamp};
use neoteletext_input::rules::{FaultCode, RuleBody, RuleEvent, RulesFault};
use neoteletext_rules::table::{Target, Value as RuleValue};
use neoteletext_web::consts::CLOSE_BROKE_PROTOCOL;
use neoteletext_web::wire_vectors::vectors;
use serde_json::{Value, json};

use super::super::rules::{STATE_ANSWER_LIMIT_MS, STATE_QUERY_INTERVAL_MS};
use super::{
    ACKNOWLEDGEMENT_WAIT_MS, ANY, CONNECTION, Channel, ChannelConfig, DEFAULT_RULE_NAMES, DefaultRules, Effect, InitialPage, ON, TICK, Vec, all_sent_to, bare,
    hello, inputs, key, line, message, record_types, rules_sent, select, selected, sent_to, subpage, told, with,
};
use crate::rules::{Compiled, MAX_KEPT_TABLES, no_table};

/// Page 101, sub-page 0001: where the engine's wire vectors of the rule
/// table are stamped.
const VECTORS: Stamp = Stamp { page: 0x101, sub: 1 };

/// A channel of `default_rules`, with the connection welcomed and told to
/// its program, and nothing selected.
fn channel_of(default_rules: DefaultRules, wheel: bool) -> Channel {
    let config = ChannelConfig {
        name: "Test".into(),
        address: "http://127.0.0.1/test/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel,
        default_rules,
    };
    let mut channel = Channel::new(config, &crate::viewer::testing::secret(), crate::viewer::testing::random());
    channel.program_started(0);
    channel.opened(CONNECTION, None, 0);
    channel.received(CONNECTION, &bare(&[hello(), select(0, ANY)]), 0);
    channel.deadline(ACKNOWLEDGEMENT_WAIT_MS);
    let _ = channel.take_effects();
    channel
}

/// A row of the engine's wire vectors, as bytes.
fn vector(kind: &str, name: &str) -> Vec<u8> {
    let row = vectors().find(|row| row.kind == kind && row.name == name).unwrap_or_else(|| panic!("no {kind} {name}"));
    let (bytes, length) = neoteletext_web::wire_vectors::bytes_of(row.value).unwrap();
    bytes[..length].to_vec()
}

/// A `rules` message for `page`.
fn rules(connections: &Value, page: &str, table: &Value) -> Vec<u8> {
    line(&json!({"type": "rules", "connections": connections, "page": page, "table": table}))
}

/// The table of RULE-38's example: one variable, one timer and three
/// rules, the third of which reads a sound's level.
fn example_table() -> Value {
    let note = |event: &str| json!({"action": "midi", "channel": 0, "message": {"event": event, "channel": 1, "key": 60, "velocity": 100}});
    json!({
        "variables": [{"name": "octave", "min": 0, "max": 8, "initial": 4}],
        "timers": [{"name": "beat", "running": true}],
        "rules": [
            {"name": "c_down", "on": {"event": "key", "keys": [{"physical": "a"}]}, "do": [note("note_on")], "echo": true},
            {"name": "c_up", "on": {"event": "key_up", "keys": [{"physical": "a"}]}, "do": [note("note_off")]},
            {"name": "hot", "on": {"event": "becomes", "condition": {"read": {"of": "sound", "channel": 0, "property": "level"}, "op": ">", "value": 40}},
             "do": [{"action": "send"}, {"action": "send", "value": {"of": "variable", "name": "octave"}}]}
        ]
    })
}

/// A channel whose connection shows page 101, sub-page 0001, with the
/// example table as the first it was sent for the page: generation 1, as
/// the engine's wire vectors have it.
fn on_the_vectors_page() -> Channel {
    let mut channel = channel_of(DefaultRules::None, false);
    channel.from_program(&rules(&json!([CONNECTION]), "101", &example_table()), 1);
    channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 2);
    let effects = channel.take_effects();
    assert_eq!(rules_sent(&effects, CONNECTION).len(), 1);
    channel
}

/// The `refused` lines among `effects`: each one's rule, path and request.
fn refusals(effects: &[Effect]) -> Vec<(Value, Value, Value)> {
    told(effects).iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["path"].clone(), line["request"].clone())).collect()
}

/// REQ-6532, RULE-53: a `send_all` channel's page that the program gave no
/// table has the default table, of twelve rules or with the wheel
/// thirteen, byte for byte the engine's `default_send_all`; a `none`
/// channel's page has the record that says it has no table. RULE-40: it is
/// sent when the connection selects the page, and its generation is 1.
#[test]
fn a_page_with_no_table_of_the_programs_has_the_channels_default() {
    for (default_rules, wheel, row) in
        [(DefaultRules::SendAll, false, "default_send_all"), (DefaultRules::SendAll, true, "default_send_all_wheel"), (DefaultRules::None, false, "no_table")]
    {
        let mut channel = channel_of(default_rules, wheel);
        channel.received(CONNECTION, &bare(&[select(0x101, ANY)]), 1);
        let sent = rules_sent(&channel.take_effects(), CONNECTION);
        let mut expected = vector("rules", row);
        // The table's first record for the page: the vector of no table is
        // written at generation 2.
        expected[2..4].copy_from_slice(&1_u16.to_be_bytes());
        assert_eq!(sent, [expected], "{row}");
    }
    assert_eq!(Compiled::send_all(false).unwrap().body(0x101, 1), vector("rules", "default_send_all"));
    assert_eq!(Compiled::send_all(true).unwrap().body(0x101, 1), vector("rules", "default_send_all_wheel"));
    assert_eq!(no_table(0x101, 2), vector("rules", "no_table"));
    assert_eq!(Compiled::send_all(false).unwrap().names().rules, DEFAULT_RULE_NAMES[..12]);
    assert_eq!(Compiled::send_all(true).unwrap().names().rules, DEFAULT_RULE_NAMES);
}

/// REQ-6532, RULE-40: a connection that selects a page is sent the page's
/// table before the page's own packets, so that it is in place when the
/// page opens.
#[test]
fn selection_sends_the_table_before_the_page() {
    let mut channel = channel_of(DefaultRules::SendAll, false);
    channel.received(CONNECTION, &bare(&[select(0x100, 0)]), 1);
    let at_selection = channel.take_effects();
    assert_eq!(rules_sent(&at_selection, CONNECTION).len(), 1, "at the selection, before the program has answered");
    channel.from_program(&line(&json!({"type": "page", "connections": [CONNECTION], "request": 2, "page": subpage()})), 2);
    let with_the_page = channel.take_effects();
    assert_eq!(rules_sent(&with_the_page, CONNECTION), [] as [Vec<u8>; 0], "it is in place: not sent again with the page");
    assert_ne!(sent_to(&with_the_page, CONNECTION), [] as [u8; 0]);
}

/// REQ-6532, RULE-40: the initial page a program's reply gives a
/// connection that selected nothing is a selection the server learns with
/// the page, so the table goes in the page's own message, first.
#[test]
fn a_page_the_reply_selects_has_its_table_first() {
    let mut channel = Channel::new(
        ChannelConfig {
            name: "Test".into(),
            address: "http://127.0.0.1/test/".into(),
            initial_page: InitialPage::new(0x100).unwrap(),
            epoch_at_start_ms: 0,
            other_addresses: Vec::new(),
            wheel: false,
            default_rules: DefaultRules::SendAll,
        },
        &crate::viewer::testing::secret(),
        crate::viewer::testing::random(),
    );
    channel.program_started(0);
    channel.opened(CONNECTION, None, 0);
    channel.received(CONNECTION, &bare(&[hello(), select(0, ANY)]), 0);
    let _ = channel.take_effects();
    let magazines = json!([{"magazine": 1, "name": "Test", "pages": [{"page": "100", "subpages": 1, "title": "Home", "section": null}]}]);
    channel.from_program(&line(&json!({"type": "reply", "connection": CONNECTION, "request": 1, "magazines": magazines, "selected": subpage()})), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = all_sent_to(&effects, CONNECTION);
    let types = record_types(&sent);
    assert!(types.contains(&neoteletext_web::records::TYPE_RULES), "{types:?}");
    let first_rules = sent.chunks(PACKET_BYTES).position(|packet| record_types(packet) == [neoteletext_web::records::TYPE_RULES]).unwrap();
    let first_row = sent.chunks(PACKET_BYTES).position(|packet| record_types(packet).is_empty() && sent_to_is_page(packet)).unwrap();
    assert!(first_rules < first_row, "the table's record at packet {first_rules}, the page's first packet at {first_row}");
}

/// Whether a packet is one of a page's own: not a packet 4/31 of a record.
fn sent_to_is_page(packet: &[u8]) -> bool {
    let packet = neoteletext_core::packet::Packet { bytes: packet.try_into().unwrap() };
    neoteletext_audio::idl::fragment(&packet).is_none()
}

/// REQ-6532, RULE-40: a table the program gives for the page a connection
/// has selected is sent at once, a replacement raises the generation by
/// one, the same table again is not sent, and a table for another page is
/// kept and sent when the connection selects that page.
#[test]
fn a_table_is_sent_at_once_or_at_its_pages_selection() {
    let mut channel = channel_of(DefaultRules::None, false);
    channel.received(CONNECTION, &bare(&[select(0x130, ANY)]), 1);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION), [no_table(0x130, 1)]);
    let vote = json!({"rules": [{"name": "vote", "on": {"event": "release", "target": {"control": 3}, "button": 0}, "do": [], "echo": true}]});
    channel.from_program(&rules(&json!("all"), "130", &vote), 2);
    let sent = rules_sent(&channel.take_effects(), CONNECTION);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0][..4], [0x01, 0x30, 0, 2], "page 130, the generation after the record at its selection");
    assert_eq!(neoteletext_rules::check(&sent[0]).map(|header| header.has_table), Ok(true));
    // The same table again changes nothing a receiver holds.
    channel.from_program(&rules(&json!("all"), "130", &vote), 3);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION), [] as [Vec<u8>; 0]);
    // Another table replaces it, one generation on.
    let other = json!({"rules": [{"name": "vote", "on": {"event": "press", "target": {"control": 3}}, "do": [], "echo": true}]});
    channel.from_program(&rules(&json!("all"), "130", &other), 4);
    let sent = rules_sent(&channel.take_effects(), CONNECTION);
    assert_eq!(sent[0][..4], [0x01, 0x30, 0, 3]);
    // A table for another page is never sent to a connection that has
    // this page selected: it waits for that page's selection.
    channel.from_program(&rules(&json!([CONNECTION]), "140", &vote), 5);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION), [] as [Vec<u8>; 0]);
    channel.received(CONNECTION, &bare(&[select(0x140, ANY)]), 6);
    let sent = rules_sent(&channel.take_effects(), CONNECTION);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0][..4], [0x01, 0x40, 0, 1], "page 140's first");
    // Removing it puts back what the channel has for a page with no table.
    channel.from_program(&rules(&json!([CONNECTION]), "140", &Value::Null), 7);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION), [no_table(0x140, 2)]);
    // Coming back to page 130, the table kept for it is sent again.
    channel.received(CONNECTION, &bare(&[select(0x130, ANY)]), 8);
    let sent = rules_sent(&channel.take_effects(), CONNECTION);
    assert_eq!(sent[0][..4], [0x01, 0x30, 0, 4]);
    // Another sub-page of the page is the same page: its table stays in
    // place, with its variables (RULE-17).
    channel.received(CONNECTION, &bare(&[select(0x130, 2)]), 9);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION), [] as [Vec<u8>; 0]);
}

/// REQ-6532, RULE-53: `"table": null` on a `send_all` channel puts the
/// default table back.
#[test]
fn removing_a_table_puts_the_default_back() {
    let mut channel = channel_of(DefaultRules::SendAll, false);
    channel.received(CONNECTION, &bare(&[select(0x101, ANY)]), 1);
    let _ = channel.take_effects();
    channel.from_program(&rules(&json!("all"), "101", &example_table()), 2);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION).len(), 1);
    channel.from_program(&rules(&json!("all"), "101", &Value::Null), 3);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION), [Compiled::send_all(false).unwrap().body(0x101, 3)]);
}

/// REQ-6532, RULE-40: a viewer who joins late is sent the table the
/// program gave them when they select the page; a connection the table was
/// not for has the channel's default.
#[test]
fn a_late_joiner_gets_the_table() {
    const LATE: u64 = 2;
    let mut channel = channel_of(DefaultRules::None, false);
    channel.opened(LATE, None, 10);
    channel.received(LATE, &bare(&[hello(), select(0x200, ANY)]), 10);
    let _ = channel.take_effects();
    channel.from_program(&rules(&json!("all"), "101", &example_table()), 11);
    assert_eq!(all_sent_to(&channel.take_effects(), LATE), [] as [u8; 0], "it shows another page");
    channel.received(LATE, &bare(&[select(0x101, 1)]), 12);
    let sent = rules_sent(&channel.take_effects(), LATE);
    assert_eq!(sent.len(), 1);
    assert_eq!(neoteletext_rules::check(&sent[0]).map(|header| (header.generation, header.has_table)), Ok((1, true)));
    // A connection that opens after the table was given was not in its
    // audience.
    channel.opened(3, None, 20);
    channel.received(3, &bare(&[hello(), select(0x101, 1)]), 20);
    assert_eq!(rules_sent(&channel.take_effects(), 3), [no_table(0x101, 1)]);
}

/// REQ-6532, RULE-40: the generation passes from 65,535 to 1, never 0, and
/// a connection keeps at most 64 pages' tables, the oldest dropped first.
#[test]
fn generations_wrap_past_zero_and_the_oldest_table_is_dropped() {
    let mut channel = channel_of(DefaultRules::None, false);
    channel.received(CONNECTION, &bare(&[select(0x130, ANY)]), 1);
    channel.connections.get_mut(&CONNECTION).unwrap().rules.generations.insert(0x130, u16::MAX);
    let _ = channel.take_effects();
    channel.from_program(&rules(&json!("all"), "130", &example_table()), 2);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION)[0][2..4], [0, 1]);
    for page in 0..MAX_KEPT_TABLES {
        channel.from_program(&rules(&json!("all"), &alloc::format!("2{page:02X}"), &example_table()), 3);
    }
    let kept = &channel.connections[&CONNECTION].rules;
    assert_eq!(kept.kept.len(), MAX_KEPT_TABLES);
    assert!(kept.kept(0x130).is_none(), "the oldest, page 130's, was dropped");
    assert!(kept.kept(0x200).is_some() && kept.kept(0x23F).is_some());
}

/// REQ-6532, RULE-37: each kind of input carried in a `RULE_EVENT` gives the
/// `event` object protocol 13 gave for the bare event, with the name of the
/// rule that sent it, its tick and its target.
#[test]
fn input_names_its_rule_its_tick_and_its_target() {
    let mut channel = selected();
    let at = Point { column: 7, row: 9, pixels: None, mods: 0 };
    let hover = Item::Event(Event::Hover { on: ON, leave: false, column: 4, row: 6 });
    let items = [
        (key(ON, Action::Press, false), "keys", json!({"kind": "key", "action": "press", "key": "a", "base": null, "mods": 0, "text": null})),
        (key(ON, Action::Release, false), "keys_up", json!({"kind": "key", "action": "release", "key": "a", "base": null, "mods": 0, "text": null})),
        (Item::Text("hi"), "text", json!({"kind": "text", "text": "hi"})),
        (Item::Event(Event::Paste { on: ON, text: "pasted" }), "text", json!({"kind": "paste", "text": "pasted"})),
        (
            Item::Event(Event::Mouse(Mouse { on: ON, action: MouseAction::Press, button: 0, at, device: Device::Mouse })),
            "press",
            json!({"kind": "mouse", "action": "press", "button": 0, "buttons": null, "mods": 0, "column": 7, "row": 9, "px": null, "py": null}),
        ),
        (hover, "hover_in", json!({"kind": "hover", "action": "enter", "column": 4, "row": 6})),
        (Item::Event(Event::FocusOut), "blur", json!({"kind": "blur"})),
    ];
    for (item, rule, event) in items {
        channel.received(CONNECTION, &message(&channel, CONNECTION, core::slice::from_ref(&item)), 5);
        let heard = inputs(&channel.take_effects());
        let expected = json!({
            "type": "input", "connection": CONNECTION, "page": "100", "sub": "0000", "lost": false, "rule": rule, "tick": TICK, "target": null, "event": event
        });
        assert_eq!(heard.last(), Some(&expected), "{rule}");
    }
    // A pointer trigger on "any" names the control or object it fired for.
    let mut carried = [0; 64];
    let length = neoteletext_input::event::write(&hover, &mut carried).unwrap();
    let generation = channel.rules_generation(CONNECTION, ON.page).unwrap();
    let named = RuleEvent { on: ON, generation, rule: 6, tick: 9, target: Target::control(3), body: RuleBody::Input(&carried[..length]) };
    channel.received(CONNECTION, &bare(&[Item::Event(Event::RuleEvent(named))]), 6);
    let heard = inputs(&channel.take_effects());
    assert_eq!((&heard[0]["rule"], &heard[0]["tick"], &heard[0]["target"]), (&json!("hover_in"), &json!(9), &json!({"control": 3})));
}

/// REQ-6532, RULE-37, RULE-85: the engine's wire vectors of `RULE_EVENT`, as
/// the program is told them: an input trigger's event as `input`, a rule
/// that only fired and a `send` of a value as `rule_event`, each naming its
/// rule and what it read as the table named them (RULE-36).
#[test]
fn rule_events_reach_the_program_by_name() {
    let mut channel = on_the_vectors_page();
    for (row, expected) in [
        (
            "fired",
            json!({"type": "rule_event", "connection": CONNECTION, "page": "101", "sub": "0001", "rule": "c_down", "tick": 412, "event": {"kind": "fired"}}),
        ),
        (
            "value",
            json!({"type": "rule_event", "connection": CONNECTION, "page": "101", "sub": "0001", "rule": "c_down", "tick": 412,
                   "event": {"kind": "value", "read": {"of": "variable", "name": "octave"}, "value": 5}}),
        ),
        (
            "input_key",
            json!({"type": "input", "connection": CONNECTION, "page": "101", "sub": "0001", "lost": false, "rule": "c_down", "tick": 412, "target": null,
                   "event": {"kind": "key", "action": "press", "key": "a", "base": null, "mods": 0, "text": "a"}}),
        ),
    ] {
        channel.received(CONNECTION, &vector("rule_event", row), 5);
        let effects = channel.take_effects();
        assert!(!effects.iter().any(|effect| matches!(effect, Effect::Close { .. })), "{row}: {effects:?}");
        let lines: Vec<Value> = told(&effects);
        assert_eq!(lines, [expected], "{row}");
    }
}

/// REQ-6532, RULE-36, RULE-40: a report names its rule by the table of the
/// generation it carries, so one of a generation the connection was never
/// sent, or of a rule that table does not have, tells the program nothing
/// and is counted.
#[test]
fn a_report_of_a_table_never_sent_is_dropped() {
    let mut channel = on_the_vectors_page();
    for (generation, rule) in [(2, 0), (1, 3)] {
        let event = RuleEvent { on: VECTORS, generation, rule, tick: 1, target: Target::NONE, body: RuleBody::Fired };
        channel.received(CONNECTION, &bare(&[Item::Event(Event::RuleEvent(event))]), 5);
        assert_eq!(told(&channel.take_effects()), [] as [Value; 0], "generation {generation}, rule {rule}");
    }
    assert_eq!(channel.stats().stale_input, 2);
    // A value the table has no name for.
    let unnamed = RuleBody::Value { value: RuleValue::variable(5), reading: 1 };
    let event = RuleEvent { on: VECTORS, generation: 1, rule: 0, tick: 1, target: Target::NONE, body: unnamed };
    channel.received(CONNECTION, &bare(&[Item::Event(Event::RuleEvent(event))]), 6);
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0]);
}

/// REQ-6532, REQ-6523, RULE-46: a viewer's input outside a `RULE_EVENT` is a
/// protocol fault under push protocol 9, and closes the connection; the
/// program hears nothing of it.
#[test]
fn bare_input_is_refused() {
    for item in [key(ON, Action::Press, false), Item::Text("hi"), Item::Event(Event::FocusIn)] {
        let mut channel = selected();
        channel.received(CONNECTION, &bare(core::slice::from_ref(&item)), 5);
        let effects = channel.take_effects();
        assert!(effects.contains(&Effect::Close { connection: CONNECTION, code: CLOSE_BROKE_PROTOCOL }), "{item:?}");
        assert_eq!(told(&effects), [] as [Value; 0], "{item:?}");
    }
}

/// REQ-6532, RULE-37: a release the server gives for something held, when
/// the page loses the input, names the rule that sent its press.
#[test]
fn a_lost_release_names_the_rule_that_sent_the_press() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[key(ON, Action::Press, false)]), 1);
    let _ = channel.take_effects();
    channel.closed(CONNECTION, 2);
    let lost = inputs(&channel.take_effects());
    assert_eq!(lost.len(), 1);
    assert_eq!(
        (&lost[0]["lost"], &lost[0]["rule"], &lost[0]["tick"], &lost[0]["event"]["action"]),
        (&json!(true), &json!("keys"), &json!(TICK), &json!("release"))
    );
}

/// A `query_state` of page 101 for the test's connection.
fn query(request: u64) -> Vec<u8> {
    line(&json!({"type": "query_state", "connection": CONNECTION, "page": "101", "request": request}))
}

/// REQ-6532, RULE-38, RULE-85: a state query goes to the connection as a
/// `RULES_QUERY` record, and the engine's wire vector of `PAGE_STATE` comes
/// back to the program as `page_state`, every number as the name the
/// table gave it: RULE-38's example.
#[test]
fn a_state_query_is_answered_by_name() {
    let mut channel = on_the_vectors_page();
    // The vector answers query 42.
    channel.connections.get_mut(&CONNECTION).unwrap().rules.identifier = 41;
    channel.from_program(&query(7), 10);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = sent_to(&effects, CONNECTION);
    assert_eq!(record_types(&sent), [neoteletext_web::records::TYPE_RULES_QUERY]);
    let mut reader = neoteletext_web::records::RecordReader::new();
    let asked = sent.as_chunks::<PACKET_BYTES>().0.iter().find_map(|packet| match reader.push(&neoteletext_core::packet::Packet { bytes: *packet }) {
        neoteletext_web::records::Read::Record(neoteletext_web::records::ChannelRecord::RulesQuery(query)) => Some((query.page.mpp(), query.identifier)),
        _ => None,
    });
    assert_eq!(asked, Some((0x101, 42)));
    channel.received(CONNECTION, &vector("page_state", "answer"), 20);
    let lines = told(&channel.take_effects());
    let expected = json!({
        "type": "page_state", "connection": CONNECTION, "page": "101", "sub": "0001", "request": 7, "tick": 9120, "table": true,
        "variables": {"octave": 5},
        "timers": {"beat": {"running": true, "elapsed": 18}},
        "enabled": ["c_down", "c_up", "hot"],
        "targets": [],
        "values": [{"read": {"of": "sound", "channel": 0, "property": "level"}, "value": 41}]
    });
    assert_eq!(lines, [expected]);
    // It answers once: the same answer again answers nothing.
    channel.received(CONNECTION, &vector("page_state", "answer"), 21);
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0]);
    channel.deadline(10 + STATE_ANSWER_LIMIT_MS);
    assert_eq!(refusals(&channel.take_effects()), [], "it was answered: nothing is refused later");
}

/// REQ-6532, RULE-38, RULE-39: a state query is refused `wrong_page` for a
/// page the connection has not selected, `unknown_connection` for one not
/// open, and `too_frequent` within a second of the last for the connection
/// and page; each refusal carries the query's `request`.
#[test]
fn a_state_query_is_refused_as_the_rules_say() {
    let mut channel = on_the_vectors_page();
    channel.from_program(&line(&json!({"type": "query_state", "connection": CONNECTION, "page": "102", "request": 1})), 10);
    assert_eq!(refusals(&channel.take_effects()), [(json!("wrong_page"), json!("page"), json!(1))]);
    channel.from_program(&line(&json!({"type": "query_state", "connection": 9, "page": "101", "request": 2})), 10);
    assert_eq!(refusals(&channel.take_effects()), [(json!("unknown_connection"), json!("connection"), json!(2))]);
    channel.from_program(&query(3), 10);
    assert_eq!(refusals(&channel.take_effects()), []);
    channel.from_program(&query(4), 10 + STATE_QUERY_INTERVAL_MS - 1);
    assert_eq!(refusals(&channel.take_effects()), [(json!("too_frequent"), json!(""), json!(4))]);
    channel.from_program(&query(5), 10 + STATE_QUERY_INTERVAL_MS);
    assert_eq!(refusals(&channel.take_effects()), [], "a second later");
    for (bad, rule, path) in
        [(json!(0), "out_of_range", "request"), (json!(4_294_967_296_u64), "out_of_range", "request"), (json!("7"), "wrong_type", "request")]
    {
        channel.from_program(&line(&json!({"type": "query_state", "connection": CONNECTION, "page": "101", "request": bad})), 5_000);
        let refused = refusals(&channel.take_effects());
        assert_eq!((&refused[0].0, &refused[0].1), (&json!(rule), &json!(path)), "{bad}");
    }
}

/// REQ-6532, RULE-38: a state query that gets no answer within ten
/// seconds, or whose connection ends or leaves the page first, is refused
/// `not_answered` with its `request`, so that a missing answer is never
/// mistaken for an empty state.
#[test]
fn a_state_query_not_answered_is_refused() {
    let not_answered = |request: u64| [(json!("not_answered"), json!(""), json!(request))];
    let mut channel = on_the_vectors_page();
    channel.from_program(&query(11), 10);
    let _ = channel.take_effects();
    assert_eq!(channel.connections[&CONNECTION].rules.next_due(), Some(10 + STATE_ANSWER_LIMIT_MS));
    assert!(channel.next_wake().is_some_and(|wake| wake <= 10 + STATE_ANSWER_LIMIT_MS));
    channel.deadline(10 + STATE_ANSWER_LIMIT_MS - 1);
    assert_eq!(refusals(&channel.take_effects()), []);
    channel.deadline(10 + STATE_ANSWER_LIMIT_MS);
    assert_eq!(refusals(&channel.take_effects()), not_answered(11));
    // It leaves the page.
    channel.from_program(&query(12), 20_000);
    channel.received(CONNECTION, &bare(&[select(0x200, ANY)]), 20_001);
    assert_eq!(refusals(&channel.take_effects()), not_answered(12));
    // The connection ends.
    channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 30_000);
    channel.from_program(&query(13), 30_001);
    let _ = channel.take_effects();
    channel.closed(CONNECTION, 30_002);
    assert_eq!(refusals(&channel.take_effects()), not_answered(13));
}

/// REQ-6532, RULE-38: a page with no table in that view answers `table`
/// false and every list empty.
#[test]
fn a_page_with_no_table_answers_an_empty_state() {
    let mut channel = channel_of(DefaultRules::None, false);
    channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 1);
    channel.from_program(&query(5), 2);
    let _ = channel.take_effects();
    let state = neoteletext_rules::state::PageState { identifier: 1, table: None };
    let mut body = [0; 64];
    let length = neoteletext_rules::state::encode(&state, &mut body).unwrap();
    let answer = neoteletext_input::rules::PageStateEvent { on: VECTORS, body: &body[..length] };
    channel.received(CONNECTION, &bare(&[Item::Event(Event::PageState(answer))]), 3);
    let expected = json!({
        "type": "page_state", "connection": CONNECTION, "page": "101", "sub": "0001", "request": 5, "tick": 0, "table": false,
        "variables": {}, "timers": {}, "enabled": [], "targets": [], "values": []
    });
    assert_eq!(told(&channel.take_effects()), [expected]);
}

/// REQ-6532, RULE-48, RULE-85: each fault a receiver reports reaches the
/// program by its name, and a table a receiver could not decode is written
/// to the operator's log as an error.
#[test]
fn a_receivers_fault_reaches_the_program_by_name() {
    let mut channel = on_the_vectors_page();
    channel.received(CONNECTION, &vector("rules_fault", "tick_overrun"), 5);
    let expected = json!({"type": "rules_fault", "connection": CONNECTION, "page": "101", "generation": 3, "fault": "tick_overrun", "count": 12});
    assert_eq!(told(&channel.take_effects()), [expected]);
    let names = [
        (FaultCode::Undecodable, "undecodable"),
        (FaultCode::TickOverrun, "tick_overrun"),
        (FaultCode::SendsDropped, "sends_dropped"),
        (FaultCode::EventsDropped, "events_dropped"),
        (FaultCode::ActionsSkipped, "actions_skipped"),
        (FaultCode::QueryDropped, "query_dropped"),
        (FaultCode::LocalOverrun, "local_overrun"),
        (FaultCode::AnimationSlowed, "animation_slowed"),
    ];
    for (fault, name) in names {
        assert_eq!(fault.name(), name, "the engine's name for it");
        let report = RulesFault { on: VECTORS, generation: 1, fault, count: 2 };
        channel.received(CONNECTION, &bare(&[Item::Event(Event::RulesFault(report))]), 6);
        let effects = channel.take_effects();
        assert_eq!(told(&effects)[0]["fault"], json!(name));
        let errors: Vec<&str> = effects
            .iter()
            .filter_map(|effect| if let Effect::Log { level: crate::channel::Level::Error, event, .. } = effect { Some(*event) } else { None })
            .collect();
        assert_eq!(errors, if fault == FaultCode::Undecodable { alloc::vec!["rules_undecodable"] } else { Vec::new() }, "{name}");
    }
}

/// REQ-6532, RULE-39: a record of a rule table cannot be sent as raw
/// packets, past the check.
#[test]
fn a_rules_record_in_raw_packets_is_refused() {
    let mut channel = channel_of(DefaultRules::SendAll, false);
    channel.received(CONNECTION, &bare(&[select(0x101, ANY)]), 1);
    let record = super::split_rules(&all_sent_to(&channel.take_effects(), CONNECTION)).1;
    let first = &record[..PACKET_BYTES];
    channel.from_program(&line(&json!({"type": "packets", "connections": [CONNECTION], "packets": crate::protocol::tests::base64(first)})), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects).iter().map(|(rule, ..)| rule.clone()).collect::<Vec<_>>(), [json!("invalid_packet")]);
    assert_eq!(all_sent_to(&effects, CONNECTION), [] as [u8; 0]);
}

/// REQ-6532, RULE-39: a faulty `rules` message is refused whole, and
/// nothing of it is sent or kept.
#[test]
fn a_faulty_table_is_refused_whole() {
    let mut channel = channel_of(DefaultRules::None, false);
    channel.received(CONNECTION, &bare(&[select(0x101, ANY)]), 1);
    let _ = channel.take_effects();
    let faulty = with(&example_table(), "/rules/1/name", json!("c_down"));
    channel.from_program(&rules(&json!("all"), "101", &faulty), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [(json!("duplicate_name"), json!("table.rules[1].name"), Value::Null)]);
    assert_eq!(all_sent_to(&effects, CONNECTION), [] as [u8; 0]);
    assert!(channel.connections[&CONNECTION].rules.kept(0x101).is_none());
}
