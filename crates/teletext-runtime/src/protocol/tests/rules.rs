extern crate std;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use super::super::{ProgramMessage, Rule, read};

/// The worked examples of page-rules.md, one `rules` message a line: the
/// example of RULE-35, the six of section 7.5 in order, and the music
/// player's table of RULE-93.
pub(crate) const RULES_EXAMPLES: &str = include_str!("rules-examples.jsonl");
/// The examples of animation.md ANIM-21, one message a line, in its order.
pub(crate) const ANIMATION_EXAMPLES: &str = include_str!("animation-examples.jsonl");

/// REQ-6532, RULE-35: every worked example of page-rules.md is read.
#[test]
fn every_worked_example_of_the_rule_table_is_read() {
    let lines: Vec<&str> = RULES_EXAMPLES.lines().collect();
    assert_eq!(lines.len(), 8);
    for line in lines {
        let message = read(line.as_bytes()).unwrap_or_else(|refusal| panic!("{refusal}: {line}"));
        assert!(matches!(message, ProgramMessage::Rules(_)));
    }
}

/// REQ-6533, ANIM-21: every example message of animation.md is read.
#[test]
fn every_example_of_animation_is_read() {
    let kinds: Vec<String> = ANIMATION_EXAMPLES
        .lines()
        .map(|line| {
            let message = read(line.as_bytes()).unwrap_or_else(|refusal| panic!("{refusal}: {line}"));
            std::format!("{:?}", message.kind())
        })
        .collect();
    assert_eq!(kinds, ["Frame", "Frame", "Animate", "Sprites", "NaplpsObjects", "SpriteSheet", "Timeline", "TimelineStop"]);
}

/// The refusal of `line`: its rule and path.
fn refused(line: &str) -> (Rule, String) {
    let refusal = read(line.as_bytes()).expect_err(line);
    (refusal.rule, refusal.path)
}

/// REQ-6532, RULE-35: a fault inside a tagged or keyed shape of a table is
/// refused with the path of the fault.
#[test]
fn a_fault_inside_a_table_keeps_its_path() {
    let table = |rule: &str| std::format!(r#"{{"type":"rules","connections":"all","page":"101","table":{{"rules":[{rule}]}}}}"#);
    let cases = [
        (r#"{"name":"a","on":{"event":"press","target":{"control":33}},"do":[]}"#, Rule::OutOfRange, "table.rules[0].on.target.control"),
        (r#"{"name":"a","on":{"event":"press","target":"screen"},"do":[]}"#, Rule::NotListed, "table.rules[0].on.target"),
        (r#"{"name":"a","on":{"event":"jump"},"do":[]}"#, Rule::NotListed, "table.rules[0].on.event"),
        (r#"{"name":"a","on":{"event":"open","extra":1},"do":[]}"#, Rule::UnknownField, "table.rules[0].on.extra"),
        (r#"{"name":"A","on":{"event":"open"},"do":[]}"#, Rule::WrongType, "table.rules[0].name"),
        (r#"{"name":"a","on":{"event":"key","keys":[]},"do":[]}"#, Rule::TooShort, "table.rules[0].on.keys"),
        (r#"{"name":"a","on":{"event":"key","keys":[{"char":"ab"}]},"do":[]}"#, Rule::TooLong, "table.rules[0].on.keys[0].char"),
        (r#"{"name":"a","on":{"event":"every","ticks":0},"do":[]}"#, Rule::OutOfRange, "table.rules[0].on.ticks"),
        (
            r#"{"name":"a","on":{"event":"open"},"when":{"and":[{"read":{"of":"keys_down"},"op":"=","value":1},{"not":{"inside":[{"object":1},"page"]}}]},"do":[]}"#,
            Rule::NotListed,
            "table.rules[0].when.and[0].op",
        ),
        (
            r#"{"name":"a","on":{"event":"open"},"when":{"not":{"read":{"of":"sound","channel":4,"property":"level"},"in_set":[1]}},"do":[]}"#,
            Rule::OutOfRange,
            "table.rules[0].when.not.read.channel",
        ),
        (r#"{"name":"a","on":{"event":"open"},"do":[{"action":"move","target":{"object":1},"x":40000,"y":0}]}"#, Rule::OutOfRange, "table.rules[0].do[0].x"),
        (
            r#"{"name":"a","on":{"event":"open"},"do":[{"action":"midi","channel":0,"message":{"event":"note_on","channel":17,"key":60,"velocity":1}}]}"#,
            Rule::OutOfRange,
            "table.rules[0].do[0].message.channel",
        ),
    ];
    for (rule, expected, path) in cases {
        assert_eq!(refused(&table(rule)), (expected, String::from(path)), "{rule}");
    }
    // The table must be given: leaving it out is not removing it.
    assert_eq!(refused(r#"{"type":"rules","connections":"all","page":"101"}"#), (Rule::MissingField, String::from("table")));
    assert!(read(br#"{"type":"rules","connections":"all","page":"101","table":null}"#).is_ok());
}

/// The names of the worked examples, in the order of [`RULES_EXAMPLES`].
const EXAMPLE_NAMES: [&str; 8] = ["rule_35", "example_1", "example_2", "example_3", "example_4", "example_5", "example_6", "music_player"];
/// Where the examples' encoded tables are kept, from this crate.
const VECTORS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/protocol/tests/rules-vectors.tsv");
/// Set to write the vectors afresh rather than compare them.
const BLESS: &str = "TELETEXT_BLESS_RULES_VECTORS";
/// The comment that heads the vectors' file.
const VECTORS_HEAD: &str = "\
# The RULES record body (page-rules.md RULE-41) of each worked example of
# page-rules.md: RULE-35's, the six of section 7.5 and the music player's
# table of RULE-93, as the server encodes it for the message's page at
# generation 1, in hexadecimal. Two columns, separated by a tab: the
# example and the body. Written by the server's test
# `every_worked_example_is_encoded_as_its_vector` with
# TELETEXT_BLESS_RULES_VECTORS=1, never by hand.
";

/// The table of a `rules` line, compiled, and its page.
fn compiled(line: &str) -> (u16, crate::rules::Compiled) {
    let ProgramMessage::Rules(message) = read(line.as_bytes()).unwrap_or_else(|refusal| panic!("{refusal}: {line}")) else { panic!("not rules: {line}") };
    (message.page.0, super::super::check::compiled(&message).unwrap().unwrap())
}

/// REQ-6532, RULE-35, RULE-59, RULE-85, RULE-93: every worked example of
/// page-rules.md, and the music player's whole table, is accepted and
/// encoded with the engine's crate to the bytes kept as its vector; each
/// passes the check every receiver decodes with, and decodes there to a
/// table that encodes to the same bytes.
#[test]
fn every_worked_example_is_encoded_as_its_vector() {
    let mut written = String::from(VECTORS_HEAD);
    for (name, line) in EXAMPLE_NAMES.iter().zip(RULES_EXAMPLES.lines()) {
        let (page, table) = compiled(line);
        let body = table.body(page, 1);
        assert!(body.len() <= neoteletext_rules::limits::MAX_TABLE_BYTES);
        let header = neoteletext_rules::check(&body).unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
        assert_eq!((header.page, header.generation, header.has_table), (page, 1, true), "{name}");
        let mut decoded = alloc::boxed::Box::new(neoteletext_rules::Table::new());
        neoteletext_rules::decode(&body, &mut decoded).unwrap();
        let mut again = alloc::vec![0; neoteletext_rules::limits::MAX_TABLE_BYTES];
        let length = neoteletext_rules::encode(page, 1, Some(&decoded), &mut again).unwrap();
        assert_eq!(again[..length], body[..], "{name} decodes to the table that was encoded");
        written.push_str(name);
        written.push('\t');
        for byte in &body {
            write!(written, "{byte:02x}").unwrap();
        }
        written.push('\n');
    }
    if std::env::var_os(BLESS).is_some() {
        std::fs::write(VECTORS, &written).unwrap();
    }
    let kept = std::fs::read_to_string(VECTORS).unwrap();
    assert!(kept == written, "rules-vectors.tsv is not what the server encodes: run with {BLESS}=1");
}

/// REQ-6532, RULE-93: the music player's table has 47 rules, none of which
/// sends: its page gives its channel no input at all.
#[test]
fn the_music_players_table_has_47_rules_and_sends_nothing() {
    let (page, table) = compiled(RULES_EXAMPLES.lines().last().unwrap());
    assert_eq!(page, 0x101);
    assert_eq!(table.names().rules.len(), 47);
    let mut decoded = alloc::boxed::Box::new(neoteletext_rules::Table::new());
    neoteletext_rules::decode(&table.body(page, 1), &mut decoded).unwrap();
    let sends = decoded.rules.iter().any(|rule| rule.echo || rule.actions.iter().any(|action| matches!(action, neoteletext_rules::table::Action::Send(_))));
    assert!(!sends);
    // It reads one metered value, audio channel 0's level, and asks for
    // keys alone.
    let index = neoteletext_rules::table::Index::of(&decoded).unwrap();
    assert_eq!((index.watch.len(), index.interest), (1, neoteletext_rules::table::INTEREST_KEYS));
}

/// A `rules` message whose table holds `rules`, and whatever `more`
/// declares.
fn table_of(rules: &str, more: &str) -> String {
    std::format!(r#"{{"type":"rules","connections":"all","page":"101","table":{{"rules":[{rules}]{more}}}}}"#)
}

/// A rule named `name` that fires on `on` and does `actions`.
fn rule(name: &str, on: &str, actions: &str) -> String {
    std::format!(r#"{{"name":"{name}","on":{on},"do":[{actions}]}}"#)
}

const OPEN: &str = r#"{"event":"open"}"#;
const SEND: &str = r#"{"action":"send"}"#;

/// REQ-6532, RULE-39: each fault of a table that the schema cannot state
/// is refused whole with its rule and the path of the fault.
#[test]
fn each_fault_of_a_table_is_refused_with_its_rule_and_path() {
    let level = |channel: u8| std::format!(r#"{{"read":{{"of":"sound","channel":{channel},"property":"level"}},"op":">","value":1}}"#);
    let nine_terms = std::format!(r#"{{"or":[{{"and":[{}]}},{}]}}"#, alloc::vec![level(0); 8].join(","), level(1));
    let four_levels = std::format!(r#"{{"not":{{"not":{{"not":{{"not":{}}}}}}}}}"#, level(0));
    let seventeen_nodes = std::format!(r#"{{"and":[{}]}}"#, (0..8).map(|_| std::format!(r#"{{"not":{}}}"#, level(0))).collect::<Vec<_>>().join(","));
    let becomes = |condition: &str| std::format!(r#"{{"event":"becomes","condition":{condition}}}"#);
    let key_value =
        |key: u32| std::format!(r#"{{"action":"send","value":{{"of":"synth","channel":0,"family":"synth106","part":1,"property":"note","key":{key}}}}}"#);
    // 65 distinct values: sixteen sends in each of five rules.
    let many_values: Vec<String> = (0..5)
        .map(|rule_number| rule(&std::format!("r{rule_number}"), OPEN, &(0..16).map(|at| key_value(rule_number * 16 + at)).collect::<Vec<_>>().join(",")))
        .collect();
    let show = |object: u32| std::format!(r#"{{"action":"show","target":{{"object":{object}}}}}"#);
    let many_targets: Vec<String> = (0..5)
        .map(|rule_number| rule(&std::format!("r{rule_number}"), OPEN, &(1..=16).map(|at| show(rule_number * 16 + at)).collect::<Vec<_>>().join(",")))
        .collect();
    let field = |name: &str| {
        std::format!(r#"{{"action":"send","value":{{"of":"synth","channel":0,"family":"synth106","part":1,"property":"field","field":"{name}"}}}}"#)
    };
    let variables = r#","variables":[{"name":"v","min":0,"max":9,"initial":4}]"#;
    let cases: Vec<(String, Rule, &str)> = alloc::vec![
        // rules_over_limit
        (table_of(&rule("a", &becomes(&nine_terms), SEND), ""), Rule::RulesOverLimit, "table.rules[0].on.condition"),
        (table_of(&rule("a", &becomes(&four_levels), SEND), ""), Rule::RulesOverLimit, "table.rules[0].on.condition"),
        (table_of(&rule("a", &becomes(&seventeen_nodes), SEND), ""), Rule::RulesOverLimit, "table.rules[0].on.condition.and[7].not"),
        (table_of(&many_values.join(","), ""), Rule::RulesOverLimit, "table"),
        (table_of(&many_targets.join(","), ""), Rule::RulesOverLimit, "table"),
        // invalid_rules
        (table_of(&rule("a", OPEN, ""), ""), Rule::InvalidRules, "table.rules[0].do"),
        (
            table_of(&std::format!(r#"{{"name":"a","on":{},"when":{},"do":[{SEND}]}}"#, becomes(&level(0)), level(1)), ""),
            Rule::InvalidRules,
            "table.rules[0].when",
        ),
        (
            table_of(&rule("a", OPEN, r#"{"action":"cycle","target":{"object":1},"first":3,"last":2,"period":6}"#), ""),
            Rule::InvalidRules,
            "table.rules[0].do[0].first",
        ),
        (
            table_of(&rule("a", &becomes(r#"{"read":{"of":"keys_down"},"in_range":[5,4]}"#), SEND), ""),
            Rule::InvalidRules,
            "table.rules[0].on.condition.in_range",
        ),
        (table_of(&rule("a", OPEN, SEND), r#","variables":[{"name":"v","min":5,"max":4,"initial":5}]"#), Rule::InvalidRules, "table.variables[0]"),
        (table_of(&rule("a", OPEN, SEND), r#","variables":[{"name":"v","min":0,"max":4,"initial":5}]"#), Rule::InvalidRules, "table.variables[0]"),
        (table_of(&rule("a", OPEN, r#"{"action":"show","target":{"object":"any"}}"#), ""), Rule::InvalidRules, "table.rules[0].do[0].target"),
        (table_of(&rule("a", OPEN, r#"{"action":"hide","target":"page"}"#), ""), Rule::InvalidRules, "table.rules[0].do[0].target"),
        (table_of(&rule("a", OPEN, r#"{"action":"move","target":{"control":2},"x":1,"y":1}"#), ""), Rule::InvalidRules, "table.rules[0].do[0].target"),
        (table_of(&rule("a", &becomes(r#"{"overlaps":["page",{"object":1}]}"#), SEND), ""), Rule::InvalidRules, "table.rules[0].on.condition.overlaps[0]",),
        (table_of(&rule("a", OPEN, SEND), r#","subpages":[{"sub":"0001","off":["a"]},{"sub":"0001","off":[]}]"#), Rule::InvalidRules, "table.subpages[1].sub",),
        (table_of(&rule("a", r#"{"event":"key","keys":[{"char":"A"}]}"#, SEND), ""), Rule::InvalidRules, "table.rules[0].on.keys[0].char"),
        (
            table_of(&rule("a", OPEN, &key_value(60).replace(r#""property":"note""#, r#""property":"notes""#)), ""),
            Rule::InvalidRules,
            "table.rules[0].do[0].value.key"
        ),
        // unknown_name
        (table_of(&rule("a", OPEN, r#"{"action":"set","variable":"w","value":1}"#), variables), Rule::UnknownName, "table.rules[0].do[0].variable"),
        (table_of(&rule("a", OPEN, r#"{"action":"start_timer","timer":"t"}"#), ""), Rule::UnknownName, "table.rules[0].do[0].timer"),
        (table_of(&rule("a", r#"{"event":"after","ticks":1,"timer":"t"}"#, SEND), ""), Rule::UnknownName, "table.rules[0].on.timer"),
        (table_of(&rule("a", r#"{"event":"wheel","target":{"rect":"pad"}}"#, SEND), ""), Rule::UnknownName, "table.rules[0].on.target.rect"),
        (table_of(&rule("a", OPEN, r#"{"action":"disable","rules":["a","b"]}"#), ""), Rule::UnknownName, "table.rules[0].do[0].rules[1]"),
        (table_of(&rule("a", OPEN, SEND), r#","subpages":[{"sub":"0002","off":["b"]}]"#), Rule::UnknownName, "table.subpages[0].off[0]"),
        // duplicate_name
        (table_of(&std::format!("{},{}", rule("a", OPEN, SEND), rule("a", OPEN, SEND)), ""), Rule::DuplicateName, "table.rules[1].name"),
        (
            table_of(&rule("a", OPEN, SEND), r#","timers":[{"name":"t","running":true},{"name":"t","running":false}]"#),
            Rule::DuplicateName,
            "table.timers[1].name",
        ),
        // `page` names the page timer and the page target (RULE-36).
        (table_of(&rule("a", OPEN, SEND), r#","timers":[{"name":"page","running":true}]"#), Rule::Reserved, "table.timers[0].name"),
        (table_of(&rule("a", OPEN, SEND), r#","rects":[{"name":"page","x":0,"y":0,"width":1,"height":1}]"#), Rule::Reserved, "table.rects[0].name"),
        // unknown_parameter
        (table_of(&rule("a", OPEN, &field("no_such_field")), ""), Rule::UnknownParameter, "table.rules[0].do[0].value.field"),
        // What a `synth` value needs with its property.
        (table_of(&rule("a", OPEN, &key_value(60).replace(r#","key":60"#, "")), ""), Rule::MissingField, "table.rules[0].do[0].value.key"),
        // A key the engine's table does not name.
        (table_of(&rule("a", r#"{"event":"key","keys":[{"name":"NoSuchKey"}]}"#, SEND), ""), Rule::NotListed, "table.rules[0].on.keys[0].name"),
        // Lists past what the schema states.
        (table_of(&rule("a", OPEN, &[SEND; 17].join(",")), ""), Rule::TooLong, "table.rules[0].do"),
        (table_of(&(0..65).map(|number| rule(&std::format!("r{number}"), OPEN, SEND)).collect::<Vec<_>>().join(","), ""), Rule::TooLong, "table.rules",),
    ];
    for (line, expected, path) in cases {
        assert_eq!(refused(&line), (expected, String::from(path)), "{line}");
    }
    // The limit and its value are in the refusal's detail.
    let refusal = read(table_of(&rule("a", &becomes(&nine_terms), SEND), "").as_bytes()).unwrap_err();
    assert_eq!(refusal.detail, "MAX_CONDITION_TERMS: at most 8 terms in a condition");
    // A field the family does have, and a named key, are accepted.
    assert!(read(table_of(&rule("a", OPEN, &field("vcf_frequency")), "").as_bytes()).is_ok());
    assert!(read(table_of(&rule("a", r#"{"event":"key","keys":[{"name":"Enter"},{"char":"é"}]}"#, SEND), "").as_bytes()).is_ok());
}

/// REQ-6532, RULE-39, RULE-57: a table whose encoding passes 8,184 bytes is
/// refused whole, `rules_over_limit`, naming the limit; one within it is
/// accepted.
#[test]
fn a_table_over_the_record_is_refused_whole() {
    // A rule of a key trigger with 16 keys and 16 actions of the longest
    // kinds: 64 of them are past one record's body.
    let keys: Vec<String> = (0..16).map(|at| std::format!(r#"{{"char":"{}"}}"#, char::from(b'a' + at))).collect();
    let on = std::format!(r#"{{"event":"key","keys":[{}]}}"#, keys.join(","));
    let action = r#"{"action":"send","value":{"of":"keys_down"}}"#;
    let condition = r#"{"read":{"of":"keys_down"},"in_set":[1,2,3,4,5,6,7,8]}"#;
    let big = |name: usize| std::format!(r#"{{"name":"r{name}","on":{on},"when":{{"and":[{}]}},"do":[{}]}}"#, [condition; 8].join(","), [action; 16].join(","));
    let table = |count: usize| table_of(&(0..count).map(big).collect::<Vec<_>>().join(","), "");
    let refusal = read(table(64).as_bytes()).unwrap_err();
    assert_eq!((refusal.rule, refusal.path.as_str()), (Rule::RulesOverLimit, "table"));
    assert_eq!(refusal.detail, "MAX_TABLE_BYTES: at most 8184 bytes of an encoded table");
    assert!(read(table(8).as_bytes()).is_ok());
}

/// REQ-6532, RULE-29: every readable patch field the engine's tables name
/// is a name a `synth` value may read, and none is named twice in a
/// family.
#[test]
fn every_readable_field_of_every_family_is_accepted() {
    use crate::protocol::SynthFamily as F;
    let families = [
        (F::Synth6581, "synth6581"),
        (F::Synth8580, "synth8580"),
        (F::Synth7, "synth7"),
        (F::Synth106, "synth106"),
        (F::Synth60, "synth60"),
        (F::Synthd, "synthd"),
        (F::Synth53, "synth53"),
        (F::Synth52, "synth52"),
        (F::Synth51, "synth51"),
        (F::Synth8, "synth8"),
        (F::Synth80, "synth80"),
    ];
    for (family, name) in families {
        let fields = crate::rules::fields_of(family);
        assert!(!fields.is_empty(), "{name}");
        for (number, field) in fields.iter().enumerate() {
            assert_eq!(fields.iter().position(|other| other.name == field.name), Some(number), "{name}.{} is named once", field.name);
            let send = std::format!(
                r#"{{"action":"send","value":{{"of":"synth","channel":0,"family":"{name}","part":1,"property":"field","field":"{}"}}}}"#,
                field.name
            );
            let (_, table) = compiled(&table_of(&rule("a", OPEN, &send), ""));
            // The value reads back by the same name (RULE-36).
            let mut decoded = alloc::boxed::Box::new(neoteletext_rules::Table::new());
            neoteletext_rules::decode(&table.body(0x101, 1), &mut decoded).unwrap();
            let neoteletext_rules::table::Action::Send(Some(value)) = decoded.rules[0].actions[0] else { panic!("{name}.{}", field.name) };
            assert_eq!(usize::from(value.d), number, "{name}.{} is parameter {number}", field.name);
            let named = serde_json::to_value(table.value(value).unwrap()).unwrap();
            assert_eq!(named["field"], serde_json::json!(field.name), "{name}");
            assert_eq!(named["family"], serde_json::json!(name));
        }
    }
}
