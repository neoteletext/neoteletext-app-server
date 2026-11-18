//! The page rule table on a channel, second form (page-rules.md RULE-40,
//! RULE-53, RULE-111, RULE-112; REQ-6532, REQ-6615).

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_input::event::{Device, Event, Item, Mouse, MouseAction, Point, Stamp};
use neoteletext_input::rules::{FaultCode, PageStateEvent, RuleBody, RuleEvent, RulesFault};
use neoteletext_rules::limits::NO_RULE;
use neoteletext_rules::table::Target;
use serde_json::{Value, json};

use super::super::rules::{STATE_ANSWER_LIMIT_MS, STATE_QUERY_INTERVAL_MS};
use super::{
    ACKNOWLEDGEMENT_WAIT_MS, ANY, CONNECTION, Channel, ChannelConfig, DEFAULT_RULE_NAMES, DefaultRules, Effect, InitialPage, ON, all_sent_to, bare, channel,
    hello, inputs, line, message, record_types, rules_sent, select, selected, sent_to, subpage, told, with,
};

/// The default table of `page` and `sub` as the engine builds it, with the
/// `wheel` rule the tests' channel has.
fn default_image(page: u16, sub: u16) -> Vec<u8> {
    crate::rules::Compiled::send_all(true, page, sub).unwrap().image().to_vec()
}

/// RULE-40, RULE-53 (REQ-6532): a selection is sent the channel's default
/// table for its page before anything else, byte for byte the engine's
/// `default_send_all`, at generation 1.
#[test]
fn a_selection_is_sent_the_default_table_first() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x100, 0)]), 0);
    let effects = channel.take_effects();
    assert_eq!(rules_sent(&effects, CONNECTION), [(0x100, 0, 1, Some(default_image(0x100, 0)))]);
}

/// RULE-112, DEC-6599 (REQ-6615): another sub-code of the same page number
/// is another page: it is sent its own default table, at the next
/// generation of the connection.
#[test]
fn another_sub_code_is_another_page() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x100, 1)]), 0);
    let effects = channel.take_effects();
    assert_eq!(rules_sent(&effects, CONNECTION), [(0x100, 1, 2, Some(default_image(0x100, 1)))]);
}

/// RULE-111 case 3 (REQ-6615): when the program's connection ends, each
/// connection is sent its page's default table again, which drops what
/// the receiver held of the program's table.
#[test]
fn the_programs_end_sends_the_default_table_again() {
    let mut channel = selected();
    channel.program_ended();
    let effects = channel.take_effects();
    assert_eq!(rules_sent(&effects, CONNECTION), [(0x100, 0, 2, Some(default_image(0x100, 0)))]);
}

/// The names of the default table's rules are the server's, in the
/// engine's order (RULE-53).
#[test]
fn the_default_tables_rules_have_their_names() {
    let table = crate::rules::Compiled::send_all(true, 0x100, 0).unwrap();
    let names: Vec<String> = (0..DEFAULT_RULE_NAMES.len()).filter_map(|rule| table.rule(rule)).map(|name| name.0).collect();
    assert_eq!(names, DEFAULT_RULE_NAMES);
}

/// A table whose one rule runs the group `flash` on a key, and which has a
/// sequence `pulse`.
fn flash_table(page: &str, sub: &str) -> Vec<u8> {
    line(&json!({"type":"rules","connections":"all","page":page,"sub":sub,"table":{
        "groups":[{"name":"flash","commands":[{"type":"report","value":{"of":"variable","variable":"v0"}}]}],
        "sequences":[{"name":"pulse","length":2,"repeat":1,"steps":[{"tick":0,"run":"flash"}]}],
        "rules":[{"name":"typed","on":{"event":"key","keys":"any"},"do":[{"type":"report"}]}]}}))
}

/// The refusals among `effects`, by rule.
fn refusals(effects: &[Effect]) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == "refused").map(|line| line["rule"].clone()).collect()
}

/// RULE-40, RULE-112 (REQ-6532, REQ-6615): a table to "all" reaches only
/// the connections that have its page and sub-code selected, at once, and
/// nothing of it is kept: a later selection of the page is sent the
/// default table, not the program's.
#[test]
fn a_table_reaches_only_its_page_and_is_kept_for_none() {
    let mut channel = selected();
    channel.from_program(&flash_table("100", "0001"), 1);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION), []);
    channel.from_program(&flash_table("100", "0000"), 2);
    let sent = rules_sent(&channel.take_effects(), CONNECTION);
    assert_eq!(sent.len(), 1);
    assert_eq!((sent[0].0, sent[0].1, sent[0].2), (0x100, 0, 2));
    assert_ne!(sent[0].3.as_deref(), Some(default_image(0x100, 0).as_slice()));
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x200, 0)]), 3);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x100, 0)]), 4);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION), [(0x100, 0, 4, Some(default_image(0x100, 0)))]);
}

/// RULE-114 (REQ-6532): a `run` sent directly names a group of the table
/// in force on the connection; an unknown one is refused `unknown_name`.
#[test]
fn a_run_names_a_group_of_the_table_in_force() {
    let mut channel = selected();
    channel.from_program(&flash_table("100", "0000"), 1);
    let _ = channel.take_effects();
    let run = |group: &str| line(&json!({"type":"run","connections":"all","page":"100","sub":"0000","group":group}));
    channel.from_program(&run("flash"), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    assert!(record_types(&sent_to(&effects, CONNECTION)).contains(&neoteletext_rules::command::TYPE_RUN));
    channel.from_program(&run("nothing"), 3);
    assert_eq!(refusals(&channel.take_effects()), [json!("unknown_name")]);
}

/// RULE-37 (REQ-6532): a receiver's report of a value, with no rule, is
/// given to the program as `rule_event` with `rule` and `row` null and the
/// value named as the table names it.
#[test]
fn a_report_of_a_value_is_a_rule_event() {
    let mut channel = selected();
    channel.from_program(&flash_table("100", "0000"), 1);
    let _ = channel.take_effects();
    let generation = channel.rules_generation(CONNECTION, 0x100, 0).unwrap();
    let value = neoteletext_rules::table::Value::variable(0);
    let event = RuleEvent { on: ON, generation, rule: NO_RULE, row: NO_RULE, tick: 9, target: Target::NONE, body: RuleBody::Value { value, reading: 5 } };
    let bytes = bare(&[Item::Event(Event::RuleEvent(event))]);
    channel.received(CONNECTION, &bytes, 2);
    let lines: Vec<Value> = told(&channel.take_effects()).into_iter().filter(|line| line["type"] == "rule_event").collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (&lines[0]["rule"], &lines[0]["row"], &lines[0]["tick"], &lines[0]["event"]),
        (&Value::Null, &Value::Null, &json!(9), &json!({"kind":"value","read":{"of":"variable","variable":"v0"},"value":5}))
    );
}

/// ANIM-17, ANIM-21, DEC-6599 (REQ-6533): an `animate` for the sub-code
/// 3F7F names no page and is refused.
#[test]
fn an_animate_for_3f7f_is_refused() {
    let mut channel = selected();
    let animate = line(&json!({"type":"animate","connections":"all","page":"100","sub":"3F7F","ops":[{"action":"show","target":{"object":1}}]}));
    channel.from_program(&animate, 1);
    assert_eq!(refusals(&channel.take_effects()), [json!("out_of_range")]);
}

/// A table whose one rule places NAPLPS objects on page 100, sub-code
/// 0000, and the body of its command for a stream of `generation`.
fn objects_table() -> (Vec<u8>, impl Fn(u16) -> Vec<u8>) {
    let objects = json!([{"number":1,"kind":"region","shown":true,"detectable":true,"layer":1,"order":0,"x":0,"y":440,"width":640,"height":40,
        "origin_x":0,"origin_y":0,"frames":[],"cover":0,"hover":null,"pressed":null,"alternative":null}]);
    let table = line(&json!({"type":"rules","connections":"all","page":"100","sub":"0000","table":{
        "rules":[{"name":"r","on":{"event":"key","keys":"any"},"do":[{"type":"naplps_objects","objects":objects}]}]}}));
    let crate::protocol::ProgramMessage::NaplpsObjects(direct) =
        crate::protocol::read(&line(&json!({"type":"naplps_objects","connections":"all","page":"100","sub":"0000","objects":objects}))).unwrap()
    else {
        panic!("not naplps_objects")
    };
    (table, move |generation| crate::animation::naplps_objects_body(0x100, 0, generation, &direct.objects).unwrap())
}

/// ANIM-11, ANIM-21, RULE-119 (REQ-6532, REQ-6533; DEC-6654 item 11): a
/// group's `naplps_objects` is written, for each connection, for the
/// generation of the NAPLPS stream that connection has of the page; with
/// none, for the one its next restart begins.
#[test]
fn a_groups_naplps_objects_are_for_the_connections_stream() {
    let (table, body) = objects_table();
    let holds = |sent: &[(u16, u16, u16, Option<Vec<u8>>)], generation: u16| {
        let image = sent[0].3.clone().unwrap();
        let wanted = body(generation);
        image.windows(wanted.len()).any(|window| window == wanted.as_slice())
    };
    let mut channel = selected();
    channel.from_program(&table, 1);
    let sent = rules_sent(&channel.take_effects(), CONNECTION);
    assert!(holds(&sent, 1), "no stream yet: the generation its first restart begins");
    let restart = |data: &str| line(&json!({"type":"naplps","connections":[CONNECTION],"page":"100","sub":"0000","restart":true,"data":data}));
    channel.from_program(&restart("QQ=="), 2);
    channel.from_program(&restart("QQ=="), 3);
    let _ = channel.take_effects();
    channel.from_program(&table, 4);
    let sent = rules_sent(&channel.take_effects(), CONNECTION);
    assert!(holds(&sent, 2) && !holds(&sent, 1), "the stream's second generation");
}

// ------------------------------------------------- the vectors' page

/// Page 101, sub-code 0001: where the engine's wire vectors of push
/// protocol 10 are stamped.
const VECTORS: Stamp = Stamp { page: 0x101, sub: 1 };

/// A channel of `default_rules`, with the connection welcomed and told to
/// its program, and nothing selected.
fn channel_of(default_rules: DefaultRules) -> Channel {
    let config = ChannelConfig {
        name: "Test".into(),
        address: "http://127.0.0.1/test/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
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
    let row = neoteletext_web::wire_vectors::vectors().find(|row| row.kind == kind && row.name == name).unwrap_or_else(|| panic!("no {kind} {name}"));
    let (bytes, length) = neoteletext_web::wire_vectors::bytes_of(row.value).unwrap();
    bytes[..length].to_vec()
}

/// The table of the vectors' page: rule 0 `c_down` on a key, rule 1 `hot`
/// reading a sound's level, rule 2 `notes` a template of six rows, and two
/// sequences.
fn vectors_table() -> Value {
    json!({
        "rows": {"notes": [60, 62, 64, 65, 67, 69]},
        "groups": [{"name": "g", "commands": [{"type": "report"}]}],
        "sequences": [
            {"name": "pulse", "length": 30, "repeat": 0, "steps": [{"tick": 0, "run": "g"}]},
            {"name": "later", "length": 30, "repeat": 1, "steps": [{"tick": 0, "run": "g"}]}
        ],
        "rules": [
            {"name": "c_down", "on": {"event": "key", "keys": [{"physical": "a"}]}, "run": "g"},
            {"name": "hot", "on": {"event": "becomes"}, "when": {"read": {"of": "sound", "channel": 0, "property": "level"}, "op": ">", "value": 40}, "run": "g"},
            {"name": "notes", "rows": "notes", "on": {"event": "press", "target": {"object": {"plus": 1, "row": "p"}}}, "run": "g"}
        ]
    })
}

/// A channel of no default table whose connection shows the vectors' page
/// with the vectors' table in force, and that table's generation.
fn on_the_vectors_page() -> (Channel, u16) {
    let mut channel = channel_of(DefaultRules::None);
    channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&json!({"type":"rules","connections":"all","page":"101","sub":"0001","table":vectors_table()})), 2);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0]);
    let generation = channel.rules_generation(CONNECTION, VECTORS.page, VECTORS.sub).unwrap();
    (channel, generation)
}

/// A receiver's message from the vectors, given the generation and the
/// state query's identifier this connection has: the vectors are of
/// generation 1 and query 42.
fn restamped(bytes: &[u8], generation: u16, identifier: u32) -> Vec<u8> {
    let (item, _) = neoteletext_input::event::read(bytes).unwrap().unwrap();
    // A restamped message is no longer than the most one may be.
    let mut out = alloc::vec![0; neoteletext_rules::limits::MAX_STATE_BYTES * 2];
    let mut body = [0; neoteletext_rules::limits::MAX_STATE_BYTES];
    let item = match item {
        Item::Event(Event::RuleEvent(event)) => Item::Event(Event::RuleEvent(RuleEvent { generation, ..event })),
        Item::Event(Event::RulesFault(fault)) => Item::Event(Event::RulesFault(RulesFault { generation, ..fault })),
        Item::Event(Event::PageState(answer)) => {
            let mut state = answer.state().unwrap();
            state.identifier = identifier;
            if let Some(table) = state.table.as_mut() {
                table.generation = generation;
            }
            let length = neoteletext_rules::state::encode(&state, &mut body).unwrap();
            Item::Event(Event::PageState(PageStateEvent { on: answer.on, body: &body[..length] }))
        }
        other => other,
    };
    let length = neoteletext_input::event::write(&item, &mut out).unwrap();
    out.truncate(length);
    out
}

/// A `query_state` of the vectors' page for the test's connection.
fn query(request: u64) -> Vec<u8> {
    line(&json!({"type": "query_state", "connection": CONNECTION, "page": "101", "sub": "0001", "request": request}))
}

/// The refusals among `effects`: each one's rule, path and request.
fn refused_with(effects: &[Effect]) -> Vec<(Value, Value, Value)> {
    told(effects).iter().filter(|line| line["type"] == "refused").map(|line| (line["rule"].clone(), line["path"].clone(), line["request"].clone())).collect()
}

/// RULE-38, RULE-47, RULE-85 (REQ-6532): `query_state` goes to the
/// connection as one `RULES_QUERY` record for the page and sub-code, and
/// the engine's `PAGE_STATE` vector comes back as `page_state`: the sixteen
/// variables, the running timers by name, and the sequences by name, the
/// second waiting; it answers once.
#[test]
fn a_state_query_is_answered_by_name() {
    let (mut channel, generation) = on_the_vectors_page();
    channel.from_program(&query(7), 10);
    let effects = channel.take_effects();
    assert_eq!(refused_with(&effects), []);
    let sent = sent_to(&effects, CONNECTION);
    assert_eq!(record_types(&sent), [neoteletext_web::records::TYPE_RULES_QUERY]);
    let mut reader = neoteletext_web::records::RecordReader::new();
    let asked = sent.as_chunks::<PACKET_BYTES>().0.iter().find_map(|packet| match reader.push(&neoteletext_core::packet::Packet { bytes: *packet }) {
        neoteletext_web::records::Read::Record(neoteletext_web::records::ChannelRecord::RulesQuery(query)) => {
            Some((query.page.mpp(), query.subcode.value(), query.identifier))
        }
        _ => None,
    });
    assert_eq!(asked, Some((0x101, 1, 1)), "the page, its sub-code and the connection's first identifier");
    channel.received(CONNECTION, &restamped(&vector("page_state", "answer"), generation, 1), 20);
    let mut variables = alloc::vec![0; neoteletext_rules::limits::VARIABLES];
    variables[0] = 5;
    let expected = json!({
        "type": "page_state", "connection": CONNECTION, "page": "101", "sub": "0001", "request": 7, "tick": 9120, "table": true,
        "variables": variables,
        "timers": {"t0": {"running": true, "elapsed": 18}},
        "sequences": {
            "pulse": {"running": true, "passes": 3, "position": 12, "waiting": false},
            "later": {"running": false, "passes": 0, "position": 0, "waiting": true}
        },
        "targets": [],
        "values": [{"read": {"of": "sound", "channel": 0, "property": "level"}, "value": 41}]
    });
    assert_eq!(told(&channel.take_effects()), [expected]);
    channel.received(CONNECTION, &restamped(&vector("page_state", "answer"), generation, 1), 21);
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0], "answered once");
    channel.deadline(10 + STATE_ANSWER_LIMIT_MS);
    assert_eq!(refused_with(&channel.take_effects()), [], "nothing is refused later");
}

/// RULE-38 (REQ-6532): `query_state` is refused `wrong_page` for a page or
/// sub-code the connection has not selected, `unknown_connection` for one
/// not open, and `too_frequent` within a second of the last for the
/// connection and page, each with the query's `request`.
#[test]
fn a_state_query_is_refused_as_the_rules_say() {
    let (mut channel, _) = on_the_vectors_page();
    channel.from_program(&line(&json!({"type": "query_state", "connection": CONNECTION, "page": "101", "sub": "0002", "request": 1})), 10);
    assert_eq!(refused_with(&channel.take_effects()), [(json!("wrong_page"), json!("page"), json!(1))]);
    channel.from_program(&line(&json!({"type": "query_state", "connection": 9, "page": "101", "sub": "0001", "request": 2})), 10);
    assert_eq!(refused_with(&channel.take_effects()), [(json!("unknown_connection"), json!("connection"), json!(2))]);
    channel.from_program(&query(3), 10);
    assert_eq!(refused_with(&channel.take_effects()), []);
    channel.from_program(&query(4), 10 + STATE_QUERY_INTERVAL_MS - 1);
    assert_eq!(refused_with(&channel.take_effects()), [(json!("too_frequent"), json!(""), json!(4))]);
    channel.from_program(&query(5), 10 + STATE_QUERY_INTERVAL_MS);
    assert_eq!(refused_with(&channel.take_effects()), [], "a second later");
}

/// RULE-38, RULE-111 (REQ-6532): a state query with no answer within its
/// time, or whose connection leaves the page or ends first, is refused
/// `not_answered` with its `request`.
#[test]
fn a_state_query_not_answered_is_refused() {
    let not_answered = |request: u64| [(json!("not_answered"), json!(""), json!(request))];
    let (mut channel, _) = on_the_vectors_page();
    channel.from_program(&query(11), 10);
    let _ = channel.take_effects();
    channel.deadline(10 + STATE_ANSWER_LIMIT_MS - 1);
    assert_eq!(refused_with(&channel.take_effects()), []);
    channel.deadline(10 + STATE_ANSWER_LIMIT_MS);
    assert_eq!(refused_with(&channel.take_effects()), not_answered(11));
    channel.from_program(&query(12), 20_000);
    channel.received(CONNECTION, &bare(&[select(0x200, ANY)]), 20_001);
    assert_eq!(refused_with(&channel.take_effects()), not_answered(12), "it left the page");
    channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 30_000);
    channel.from_program(&query(13), 30_001);
    let _ = channel.take_effects();
    channel.closed(CONNECTION, 30_002);
    assert_eq!(refused_with(&channel.take_effects()), not_answered(13), "the connection ended");
}

/// RULE-38, RULE-47 (REQ-6532): a page with no table in the view answers
/// `table` false, its variables zero and every list empty.
#[test]
fn a_page_with_no_table_answers_an_empty_state() {
    let mut channel = channel_of(DefaultRules::None);
    channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 1);
    channel.from_program(&query(5), 2);
    let _ = channel.take_effects();
    let state = neoteletext_rules::state::PageState { identifier: 1, table: None };
    let mut body = [0; 64];
    let length = neoteletext_rules::state::encode(&state, &mut body).unwrap();
    let answer = PageStateEvent { on: VECTORS, body: &body[..length] };
    channel.received(CONNECTION, &bare(&[Item::Event(Event::PageState(answer))]), 3);
    let expected = json!({
        "type": "page_state", "connection": CONNECTION, "page": "101", "sub": "0001", "request": 5, "tick": 0, "table": false,
        "variables": alloc::vec![0; neoteletext_rules::limits::VARIABLES], "timers": {}, "sequences": {}, "targets": [], "values": []
    });
    assert_eq!(told(&channel.take_effects()), [expected]);
}

/// RULE-37, RULE-85 (REQ-6532): the engine's `RULE_EVENT` vectors as the
/// program is told them, named by the table of their generation: a rule
/// that fired, with and without its row, a run no rule made, a value, and
/// an input event, which is `input` with the rule's name and its row.
#[test]
fn rule_events_reach_the_program_by_name() {
    let (mut channel, generation) = on_the_vectors_page();
    let fired = |rule: Value, row: Value| json!({"type": "rule_event", "connection": CONNECTION, "page": "101", "sub": "0001", "rule": rule, "row": row, "tick": 412, "event": {"kind": "fired"}});
    for (row, expected) in [
        ("fired", fired(json!("c_down"), Value::Null)),
        ("fired_row", fired(json!("notes"), json!(5))),
        ("fired_no_rule", fired(Value::Null, Value::Null)),
        (
            "value",
            json!({"type": "rule_event", "connection": CONNECTION, "page": "101", "sub": "0001", "rule": "c_down", "row": null, "tick": 412,
                   "event": {"kind": "value", "read": {"of": "variable", "variable": "v0"}, "value": 5}}),
        ),
        (
            "input_key",
            json!({"type": "input", "connection": CONNECTION, "page": "101", "sub": "0001", "lost": false, "rule": "c_down", "row": null, "tick": 412,
                   "target": null, "event": {"kind": "key", "action": "press", "key": "a", "base": null, "mods": 0, "text": "a"}}),
        ),
    ] {
        channel.received(CONNECTION, &restamped(&vector("rule_event", row), generation, 0), 5);
        let effects = channel.take_effects();
        assert!(!effects.iter().any(|effect| matches!(effect, Effect::Close { .. })), "{row}: {effects:?}");
        assert_eq!(told(&effects), [expected], "{row}");
    }
}

/// RULE-37 (REQ-6532): an input event a template rule's row sent carries
/// that row in `input`'s `row`, and the object it fired for.
#[test]
fn input_carries_the_row_that_sent_it() {
    let (mut channel, generation) = on_the_vectors_page();
    let at = Point { column: 7, row: 9, pixels: None, mods: 0 };
    let press = Item::Event(Event::Mouse(Mouse { on: VECTORS, action: MouseAction::Press, button: 0, at, device: Device::Mouse }));
    let carried = bare(core::slice::from_ref(&press));
    let event = RuleEvent { on: VECTORS, generation, rule: 2, row: 3, tick: 40, target: Target::object(65), body: RuleBody::Input(&carried) };
    channel.received(CONNECTION, &bare(&[Item::Event(Event::RuleEvent(event))]), 5);
    let heard = inputs(&channel.take_effects());
    assert_eq!(heard.len(), 1);
    assert_eq!((&heard[0]["rule"], &heard[0]["row"], &heard[0]["tick"], &heard[0]["target"]), (&json!("notes"), &json!(3), &json!(40), &json!({"object": 65})));
}

/// RULE-36, RULE-37 (REQ-6532): a report of a generation the connection
/// was never sent, of a rule its table does not have, or of a value it has
/// no name for tells the program nothing and is counted.
#[test]
fn a_report_of_a_table_never_sent_is_dropped() {
    let (mut channel, generation) = on_the_vectors_page();
    for (stamped, rule) in [(generation + 1, 0), (generation, 7)] {
        let event = RuleEvent { on: VECTORS, generation: stamped, rule, row: NO_RULE, tick: 1, target: Target::NONE, body: RuleBody::Fired };
        channel.received(CONNECTION, &bare(&[Item::Event(Event::RuleEvent(event))]), 5);
        assert_eq!(told(&channel.take_effects()), [] as [Value; 0], "generation {stamped}, rule {rule}");
    }
    assert_eq!(channel.stats().stale_input, 2);
    // A sequence the table does not have: it has two, and a receiver
    // reads a value of any below the most a table holds.
    let beyond = u8::try_from(neoteletext_rules::limits::MAX_SEQUENCES - 1).unwrap();
    let unnamed = RuleBody::Value { value: neoteletext_rules::table::Value::sequence(beyond, 0), reading: 1 };
    let event = RuleEvent { on: VECTORS, generation, rule: 0, row: NO_RULE, tick: 1, target: Target::NONE, body: unnamed };
    channel.received(CONNECTION, &bare(&[Item::Event(Event::RuleEvent(event))]), 6);
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0]);
    assert_eq!(channel.stats().stale_input, 3);
}

/// RULE-48, RULE-85 (REQ-6532): the engine's `RULES_FAULT` vector reaches
/// the program as `rules_fault` with its page and sub-code, and each of the
/// nine faults by its name; a table a receiver could not decode is written
/// to the operator's log as an error.
#[test]
fn a_receivers_fault_reaches_the_program_by_name() {
    let (mut channel, _) = on_the_vectors_page();
    channel.received(CONNECTION, &vector("rules_fault", "evaluation_late"), 5);
    let expected =
        json!({"type": "rules_fault", "connection": CONNECTION, "page": "101", "sub": "0001", "generation": 3, "fault": "evaluation_late", "count": 12});
    assert_eq!(told(&channel.take_effects()), [expected]);
    let names = [
        (FaultCode::Undecodable, "undecodable"),
        (FaultCode::ReportsDropped, "reports_dropped"),
        (FaultCode::EventsDropped, "events_dropped"),
        (FaultCode::CommandsSkipped, "commands_skipped"),
        (FaultCode::QueryDropped, "query_dropped"),
        (FaultCode::LocalOverrun, "local_overrun"),
        (FaultCode::AnimationSlowed, "animation_slowed"),
        (FaultCode::EvaluationLate, "evaluation_late"),
        (FaultCode::QueueFull, "queue_full"),
    ];
    for (fault, name) in names {
        assert_eq!(fault.name(), name, "the engine's name for it");
        let report = RulesFault { on: VECTORS, generation: 1, fault, count: 2 };
        channel.received(CONNECTION, &bare(&[Item::Event(Event::RulesFault(report))]), 6);
        let effects = channel.take_effects();
        assert_eq!((&told(&effects)[0]["fault"], &told(&effects)[0]["sub"]), (&json!(name), &json!("0001")));
        let errors: Vec<&str> = effects
            .iter()
            .filter_map(|effect| if let Effect::Log { level: crate::channel::Level::Error, event, .. } = effect { Some(*event) } else { None })
            .collect();
        assert_eq!(errors, if fault == FaultCode::Undecodable { alloc::vec!["rules_undecodable"] } else { Vec::new() }, "{name}");
    }
}

/// RULE-111 case 2 (REQ-6532, REQ-6615): when a connection ends, the
/// server forgets the tables it sent it: the same connection number opened
/// again is sent its page's record afresh at generation 1, and a report of
/// the table it had is dropped.
#[test]
fn a_connections_end_forgets_its_tables() {
    let (mut channel, generation) = on_the_vectors_page();
    assert!(generation > 1);
    channel.closed(CONNECTION, 10);
    let _ = channel.take_effects();
    channel.opened(CONNECTION, None, 11);
    channel.received(CONNECTION, &bare(&[hello(), select(VECTORS.page, VECTORS.sub)]), 11);
    let sent = rules_sent(&channel.take_effects(), CONNECTION);
    assert_eq!(sent.iter().map(|(page, sub, generation, table)| (*page, *sub, *generation, table.is_some())).collect::<Vec<_>>(), [(0x101, 1, 1, false)]);
    let event = RuleEvent { on: VECTORS, generation, rule: 0, row: NO_RULE, tick: 1, target: Target::NONE, body: RuleBody::Fired };
    channel.received(CONNECTION, &bare(&[Item::Event(Event::RuleEvent(event))]), 12);
    assert_eq!(told(&channel.take_effects()).iter().filter(|line| line["type"] == "rule_event").count(), 0);
}

/// RULE-40 (REQ-6532): the generation passes from 65,535 to 1, never 0.
/// Each removal sends a record, at the next generation.
#[test]
fn generations_wrap_past_zero() {
    let mut channel = channel_of(DefaultRules::None);
    channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 1);
    let table = line(&json!({"type":"rules","connections":"all","page":"101","sub":"0001","table":null}));
    let mut last = 0;
    for _ in 0..u16::MAX {
        channel.from_program(&table, 2);
        last = rules_sent(&channel.take_effects(), CONNECTION)[0].2;
    }
    assert_eq!(last, 1, "after 65,535 the next is 1");
}

/// RULE-35, RULE-119 (REQ-6532): a group's `play` of a local sound is a
/// CONTROL command for the table's page, with the play's channel, handle,
/// generation and gain, never waiting for the current sound.
#[test]
fn a_groups_play_is_a_control_for_the_tables_page() {
    let (mut channel, _) = on_the_vectors_page();
    let play = json!({"type":"sound","kind":"play","channel":1,"handle":5,"generation":2,"gain":90,"fade_ms":5,"duck":0,"loop":false,"after":false});
    let table = json!({"rules":[{"name":"r","on":{"event":"key","keys":"any"},"do":[play]}]});
    channel.from_program(&line(&json!({"type":"rules","connections":"all","page":"101","sub":"0001","table":table})), 3);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    let image = rules_sent(&effects, CONNECTION)[0].3.clone().unwrap();
    let mut decoded = alloc::boxed::Box::new(neoteletext_rules::Table::new(VECTORS.page, VECTORS.sub));
    neoteletext_rules::decode(&image, VECTORS.page, VECTORS.sub, neoteletext_web::records::body_check, &mut decoded).unwrap();
    let command = decoded.groups.commands(0).next().unwrap();
    assert_eq!(command.kind, neoteletext_rules::command::TYPE_CONTROL);
    let mut record = [0; 64];
    let length = neoteletext_audio::record::write(command.kind, command.body, &mut record).unwrap();
    let Ok(neoteletext_audio::record::Record::Control(control)) = neoteletext_audio::record::parse(&record[..length]) else { panic!("not a CONTROL") };
    assert_eq!(
        (control.operation, control.channel, control.handle, control.generation, control.gain, control.page),
        (neoteletext_audio::record::Operation::Play, 1, 5, 2, 90, Some(VECTORS.page))
    );
}

/// RULE-39 (REQ-6532): a faulty `rules` message is refused whole and
/// nothing of it is sent; the connection keeps the record it had.
#[test]
fn a_faulty_table_is_refused_whole() {
    let (mut channel, generation) = on_the_vectors_page();
    let faulty = with(&vectors_table(), "/rules/1/name", json!("c_down"));
    channel.from_program(&line(&json!({"type":"rules","connections":"all","page":"101","sub":"0001","table":faulty})), 3);
    let effects = channel.take_effects();
    assert_eq!(refused_with(&effects), [(json!("duplicate_name"), json!("table.rules[1].name"), Value::Null)]);
    assert_eq!(all_sent_to(&effects, CONNECTION), [] as [u8; 0]);
    assert_eq!(channel.rules_generation(CONNECTION, VECTORS.page, VECTORS.sub), Some(generation));
}

/// RULE-39 (REQ-6532): a record of a rule table cannot be sent as raw
/// packets, past the server's check.
#[test]
fn a_rules_record_in_raw_packets_is_refused() {
    let mut channel = channel_of(DefaultRules::SendAll);
    channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 1);
    let record = super::split_rules(&all_sent_to(&channel.take_effects(), CONNECTION)).1;
    let first = &record[..PACKET_BYTES];
    channel.from_program(&line(&json!({"type": "packets", "connections": [CONNECTION], "packets": crate::protocol::tests::base64(first)})), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [json!("invalid_packet")]);
    assert_eq!(all_sent_to(&effects, CONNECTION), [] as [u8; 0]);
}

/// RULE-40, RULE-53 (REQ-6532): `"table": null` puts the channel's default
/// table back, at the next generation; on a channel of no default, the
/// record that says the page has none.
#[test]
fn removing_a_table_puts_the_default_back() {
    for (default_rules, has_table) in [(DefaultRules::SendAll, true), (DefaultRules::None, false)] {
        let mut channel = channel_of(default_rules);
        channel.received(CONNECTION, &bare(&[select(VECTORS.page, VECTORS.sub)]), 1);
        let _ = channel.take_effects();
        channel.from_program(&line(&json!({"type":"rules","connections":"all","page":"101","sub":"0001","table":vectors_table()})), 2);
        let _ = channel.take_effects();
        channel.from_program(&line(&json!({"type":"rules","connections":"all","page":"101","sub":"0001","table":null})), 3);
        let sent = rules_sent(&channel.take_effects(), CONNECTION);
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].2, sent[0].3.is_some()), (3, has_table), "{default_rules:?}");
        if has_table {
            assert_eq!(sent[0].3.as_deref(), Some(crate::rules::Compiled::send_all(false, VECTORS.page, VECTORS.sub).unwrap().image()));
        }
    }
}

/// RULE-40 (REQ-6532): a selection's record is sent before the page's own
/// packets, so that it is in place when the page opens, and is not sent
/// again with the page.
#[test]
fn selection_sends_the_table_before_the_page() {
    let mut channel = channel_of(DefaultRules::SendAll);
    channel.received(CONNECTION, &bare(&[select(0x100, 0)]), 1);
    assert_eq!(rules_sent(&channel.take_effects(), CONNECTION).len(), 1, "at the selection, before the program has answered");
    channel.from_program(&line(&json!({"type": "page", "connections": [CONNECTION], "request": 2, "page": subpage()})), 2);
    let with_the_page = channel.take_effects();
    assert_eq!(rules_sent(&with_the_page, CONNECTION).len(), 0, "in place: not sent again with the page");
    assert_ne!(sent_to(&with_the_page, CONNECTION), [] as [u8; 0]);
}

/// RULE-40 (REQ-6532): the initial page a program's reply gives a
/// connection that selected nothing is a selection the server learns with
/// the page, so the page's record goes in the page's own message, first.
#[test]
fn a_page_the_reply_selects_has_its_record_first() {
    let mut channel = channel_of(DefaultRules::SendAll);
    let magazines = json!([{"magazine": 1, "name": "Test", "pages": [{"page": "100", "subpages": 1, "title": "Home", "section": null}]}]);
    channel.from_program(&line(&json!({"type": "reply", "connection": CONNECTION, "request": 1, "magazines": magazines, "selected": subpage()})), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    let sent = all_sent_to(&effects, CONNECTION);
    let is_rules = |packet: &[u8]| record_types(packet) == [neoteletext_web::records::TYPE_RULES];
    let is_page = |packet: &[u8]| {
        let packet = neoteletext_core::packet::Packet { bytes: packet.try_into().unwrap() };
        neoteletext_audio::idl::fragment(&packet).is_none()
    };
    let first_rules = sent.chunks(PACKET_BYTES).position(is_rules).expect("the page's record");
    let first_row = sent.chunks(PACKET_BYTES).position(is_page).expect("the page");
    assert!(first_rules < first_row, "the record at packet {first_rules}, the page's first packet at {first_row}");
}
