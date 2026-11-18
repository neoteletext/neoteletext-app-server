extern crate std;

use alloc::string::String;
use alloc::vec::Vec;

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
