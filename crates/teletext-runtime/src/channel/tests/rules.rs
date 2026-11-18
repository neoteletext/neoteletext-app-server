//! The page rule table on a channel, second form (page-rules.md RULE-40,
//! RULE-53, RULE-111, RULE-112; REQ-6532, REQ-6615).

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_input::event::{Event, Item};
use serde_json::{Value, json};

use super::{CONNECTION, DEFAULT_RULE_NAMES, Effect, ON, bare, channel, line, message, record_types, rules_sent, select, selected, sent_to, told};

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
    use neoteletext_input::rules::{RuleBody, RuleEvent};
    let mut channel = selected();
    channel.from_program(&flash_table("100", "0000"), 1);
    let _ = channel.take_effects();
    let generation = channel.rules_generation(CONNECTION, 0x100, 0).unwrap();
    let value = neoteletext_rules::table::Value::variable(0);
    let event = RuleEvent {
        on: ON,
        generation,
        rule: neoteletext_rules::limits::NO_RULE,
        row: neoteletext_rules::limits::NO_RULE,
        tick: 9,
        target: neoteletext_rules::table::Target::NONE,
        body: RuleBody::Value { value, reading: 5 },
    };
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
