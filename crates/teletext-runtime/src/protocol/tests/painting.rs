//! Painting as a show as the reader takes it (neoteletext-common's
//! DEC-7031 items 4 and 6; REQ-7024, REQ-7025, REQ-7026, TODO-7027): the
//! `line_speed` and `default_rate` fields of `regis` and `naplps`, each
//! refusal with the constant it names, the `painted` trigger written as
//! the engine's trigger 22, its refusals, and the schema that states them.

use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use neoteletext_rules::limits::{MAX_PAINT_WATCHED, value_of};
use neoteletext_rules::table::{Target, TargetKind, Trigger, TriggerTarget};
use neoteletext_web::consts::{MAX_DEFAULT_PAINT_RATE, MAX_LINE_SPEED, MIN_LINE_SPEED};
use serde_json::{Value, json};

use super::{accepted, base64, line, refused, with};
use crate::protocol::{ProgramMessage, Rule, read, schema};

/// A `regis` message that begins a stream, valid.
fn regis() -> Value {
    json!({"type": "regis", "connections": [1], "page": "100", "sub": "0001", "restart": true, "data": base64(b"P[1,1]")})
}

/// A `naplps` message that begins a stream, valid.
fn naplps() -> Value {
    json!({"type": "naplps", "connections": [1], "page": "100", "sub": "0001", "restart": true, "data": base64(b"\x0e")})
}

/// `message`'s two fields as the reader gave them: (line speed, rate).
fn fields(message: &ProgramMessage) -> (Option<u32>, Option<u32>) {
    match message {
        ProgramMessage::Regis(regis) => (regis.line_speed, regis.default_rate),
        ProgramMessage::Naplps(naplps) => (naplps.line_speed, None),
        other => panic!("{other:?}"),
    }
}

/// DEC-7031 item 6: a line speed one below `MIN_LINE_SPEED` or one above
/// `MAX_LINE_SPEED` is refused `invalid_paint` at the field, on both
/// messages, the detail naming both constants and their values.
#[test]
fn a_line_speed_outside_its_range_is_refused_naming_its_constants() {
    for message in [regis(), naplps()] {
        for speed in [MIN_LINE_SPEED - 1, MAX_LINE_SPEED + 1] {
            let refusal = refused(&with(&message, "/line_speed", json!(speed)), "line_speed", Rule::InvalidPaint);
            for named in [format!("MIN_LINE_SPEED ({MIN_LINE_SPEED})"), format!("MAX_LINE_SPEED ({MAX_LINE_SPEED})")] {
                assert!(refusal.detail.contains(&named), "{speed} on {}: {refusal}", message["type"]);
            }
            assert!(refusal.detail.starts_with(&speed.to_string()), "{refusal}");
        }
    }
}

/// DEC-7031 item 6: a default rate of 0 or one above
/// `MAX_DEFAULT_PAINT_RATE` is refused `invalid_paint` at the field, the
/// detail naming the constant and its value.
#[test]
fn a_default_rate_outside_its_range_is_refused_naming_its_constant() {
    for rate in [0, u32::from(MAX_DEFAULT_PAINT_RATE) + 1] {
        let refusal = refused(&with(&regis(), "/default_rate", json!(rate)), "default_rate", Rule::InvalidPaint);
        assert!(refusal.detail.contains(&format!("MAX_DEFAULT_PAINT_RATE ({MAX_DEFAULT_PAINT_RATE})")), "{rate}: {refusal}");
        assert!(refusal.detail.starts_with(&format!("{rate}: 1 to")), "{refusal}");
    }
}

/// DEC-7031 item 6: each field is for the stream a message begins, so a
/// message without `restart`, or with `restart: false`, that states one
/// is refused `invalid_paint` at that field, though its value is in range.
#[test]
fn each_paint_field_is_refused_without_restart() {
    let cases = [(regis(), "line_speed", json!(MIN_LINE_SPEED)), (regis(), "default_rate", json!(1)), (naplps(), "line_speed", json!(MAX_LINE_SPEED))];
    for (message, field, value) in cases {
        let stated = with(&message, &format!("/{field}"), value);
        for not_begun in [super::without(&stated, "/restart"), with(&stated, "/restart", json!(false))] {
            let refusal = refused(&not_begun, field, Rule::InvalidPaint);
            assert!(refusal.detail.contains("restart: true"), "{refusal}");
        }
    }
}

/// DEC-7031 item 6: a default rate is ReGIS's alone; on `naplps` it is an
/// unknown field.
#[test]
fn a_default_rate_on_naplps_is_an_unknown_field() {
    refused(&with(&naplps(), "/default_rate", json!(1)), "default_rate", Rule::UnknownField);
}

/// DEC-7031 item 6: the bounds themselves are accepted, read as stated,
/// alone and together; a message stating neither reads none.
#[test]
fn the_paint_fields_at_their_bounds_are_accepted() {
    for speed in [MIN_LINE_SPEED, MAX_LINE_SPEED] {
        assert_eq!(fields(&accepted(&with(&regis(), "/line_speed", json!(speed)))), (Some(speed), None));
        assert_eq!(fields(&accepted(&with(&naplps(), "/line_speed", json!(speed)))), (Some(speed), None));
    }
    for rate in [1, u32::from(MAX_DEFAULT_PAINT_RATE)] {
        assert_eq!(fields(&accepted(&with(&regis(), "/default_rate", json!(rate)))), (None, Some(rate)));
    }
    let both = with(&with(&regis(), "/line_speed", json!(MIN_LINE_SPEED)), "/default_rate", json!(MAX_DEFAULT_PAINT_RATE));
    assert_eq!(fields(&accepted(&both)), (Some(MIN_LINE_SPEED), Some(u32::from(MAX_DEFAULT_PAINT_RATE))));
    assert_eq!(fields(&accepted(&regis())), (None, None));
    assert_eq!(fields(&accepted(&with(&regis(), "/line_speed", Value::Null))), (None, None), "null is left out");
}

/// DEC-7031 item 6: a value past `u32::MAX`, which the field's type cannot
/// hold, is refused by the reader's type, `wrong_type` at the field, before
/// the range is checked; a negative one too.
#[test]
fn a_paint_value_past_the_fields_type_is_a_wrong_type() {
    let past = u64::from(u32::MAX) + 1;
    for (message, field) in [(regis(), "line_speed"), (regis(), "default_rate"), (naplps(), "line_speed")] {
        refused(&with(&message, &format!("/{field}"), json!(past)), field, Rule::WrongType);
        refused(&with(&message, &format!("/{field}"), json!(-1)), field, Rule::WrongType);
    }
}

/// A `rules` message for page 101/0001 of `rules`, each running group `g`.
fn table_of(rules: &[Value]) -> Value {
    json!({"type": "rules", "connections": "all", "page": "101", "sub": "0001",
           "table": {"rows": {"drawings": [1, 2, 3]}, "groups": [{"name": "g", "commands": [{"type": "report"}]}], "rules": rules}})
}

/// The engine's table that `message`'s image decodes to, by the receivers'
/// own decoder.
fn decoded(message: &Value) -> alloc::boxed::Box<neoteletext_rules::Table> {
    let ProgramMessage::Rules(rules) = read(&line(message)).unwrap_or_else(|refusal| panic!("refused: {refusal}")) else { panic!("not rules") };
    let compiled = crate::protocol::check::compiled(&rules).unwrap().unwrap();
    let mut table = alloc::boxed::Box::new(neoteletext_rules::Table::new(0x101, 1));
    neoteletext_rules::decode(compiled.image(), 0x101, 1, neoteletext_web::records::body_check, &mut table).unwrap();
    table
}

/// The triggers of the decoded table's rules, in order.
fn triggers(message: &Value) -> Vec<Vec<Trigger>> {
    decoded(message).rules.iter().map(|rule| rule.triggers.iter().copied().collect()).collect()
}

/// DEC-7031 item 4 (REQ-7026): `"page"`, `{"object": n}` and, in a rule
/// with rows, `{"object": {"plus": k, "row": "p"}}` are each written as
/// the engine's trigger 22, which its decoder reads as `Trigger::Painted`
/// with that target.
#[test]
fn each_painted_target_is_the_engines_painted_trigger() {
    let message = table_of(&[
        json!({"name": "whole", "on": {"event": "painted", "target": "page"}, "run": "g"}),
        json!({"name": "one", "on": {"event": "painted", "target": {"object": 301}}, "run": "g"}),
        json!({"name": "last", "on": {"event": "painted", "target": {"object": 65_535}}, "run": "g"}),
        json!({"name": "each", "rows": "drawings", "on": {"event": "painted", "target": {"object": {"plus": 10, "row": "p"}}}, "run": "g"}),
    ]);
    assert_eq!(
        triggers(&message),
        [
            [Trigger::Painted { target: TriggerTarget::PAGE }],
            [Trigger::Painted { target: TriggerTarget::of(Target::object(301)) }],
            [Trigger::Painted { target: TriggerTarget::of(Target::object(u16::MAX)) }],
            [Trigger::Painted { target: TriggerTarget::row(TargetKind::Object, 10) }],
        ]
    );
}

/// DEC-7031 item 4: a painted trigger among others in one rule's `on`
/// keeps its place and its target, and the others are as they were.
#[test]
fn a_painted_trigger_mixes_with_other_triggers() {
    let message = table_of(&[json!({"name": "mixed", "on": [
        {"event": "open"},
        {"event": "painted", "target": {"object": 7}},
        {"event": "press", "target": {"object": 7}},
        {"event": "painted", "target": "page"},
    ], "run": "g"})]);
    let found = &triggers(&message)[0];
    assert_eq!(found.len(), 4);
    assert_eq!(found[1], Trigger::Painted { target: TriggerTarget::of(Target::object(7)) });
    assert_eq!(found[3], Trigger::Painted { target: TriggerTarget::PAGE });
    assert!(!matches!(found[0], Trigger::Painted { .. }) && !matches!(found[2], Trigger::Painted { .. }), "{found:?}");
    let alone = table_of(&[
        json!({"name": "a", "on": {"event": "open"}, "run": "g"}),
        json!({"name": "b", "on": {"event": "press", "target": {"object": 7}}, "run": "g"}),
    ]);
    let others = triggers(&alone);
    assert_eq!((found[0], found[2]), (others[0][0], others[1][0]), "the other triggers are written as they are alone");
}

/// DEC-7031 item 4: `{"object": "any"}`, a control and a rectangle are
/// refused `invalid_rules` at the target; the word `"any"` is not one a
/// target takes, `not_listed`; drawing 0 is `out_of_range`.
#[test]
fn each_target_a_painted_trigger_cannot_have_is_refused() {
    let with_rect = |target: Value| {
        let mut message = table_of(&[json!({"name": "r", "on": {"event": "painted", "target": target}, "run": "g"})]);
        message["table"]["rects"] = json!([{"name": "pad", "x": 0, "y": 0, "width": 10, "height": 10}]);
        message
    };
    for target in [json!({"object": "any"}), json!({"control": 1}), json!({"control": "any"}), json!({"rect": "pad"})] {
        refused(&with_rect(target), "table.rules[0].on.target", Rule::InvalidRules);
    }
    refused(&with_rect(json!("any")), "table.rules[0].on.target", Rule::NotListed);
    refused(&with_rect(json!({"object": 0})), "table.rules[0].on.target.object", Rule::OutOfRange);
    // The row form outside a rule with rows, as every trigger's.
    refused(&with_rect(json!({"object": {"plus": 1, "row": "p"}})), "table.rules[0].on.target", Rule::InvalidRules);
    // The trigger names its target.
    refused(&table_of(&[json!({"name": "r", "on": {"event": "painted"}, "run": "g"})]), "table.rules[0].on.target", Rule::MissingField);
}

/// DEC-7031 item 4: a table naming `MAX_PAINT_WATCHED` distinct drawings
/// in painted triggers is accepted; one more is refused
/// `rules_over_limit`, naming the constant and its value; the same drawing
/// named twice counts once.
#[test]
fn a_table_names_at_most_max_paint_watched_drawings() {
    let watched = usize::try_from(value_of("MAX_PAINT_WATCHED").unwrap()).unwrap();
    assert_eq!(watched, MAX_PAINT_WATCHED);
    let naming = |drawings: &[usize]| {
        let rules: Vec<Value> = drawings
            .iter()
            .enumerate()
            .map(|(at, drawing)| json!({"name": format!("r{at}"), "on": {"event": "painted", "target": {"object": drawing}}, "run": "g"}))
            .collect();
        table_of(&rules)
    };
    let at: Vec<usize> = (1..=watched).collect();
    assert_eq!(triggers(&naming(&at)).len(), watched);
    let mut repeated = at.clone();
    repeated.push(1);
    assert_eq!(triggers(&naming(&repeated)).len(), watched + 1, "a drawing named again is not another");
    let over: Vec<usize> = (1..=watched + 1).collect();
    let refusal = refused(&naming(&over), "table", Rule::RulesOverLimit);
    assert_eq!(refusal.detail, format!("MAX_PAINT_WATCHED: at most {MAX_PAINT_WATCHED}"), "{refusal}");
}

/// DEC-7031 item 6: the schema states the three new properties with their
/// bounds, the rule `invalid_paint`, and the `painted` trigger's branch.
#[test]
fn the_schema_states_the_paint_fields_the_rule_and_the_trigger() {
    let schema = schema();
    let messages = schema["$defs"]["ProgramMessage"]["oneOf"].as_array().unwrap();
    let of = |kind: &str| messages.iter().find(|message| message["properties"]["type"]["const"] == kind).unwrap_or_else(|| panic!("{kind}"));
    let bounds = |property: &Value| (property["minimum"].clone(), property["maximum"].clone(), property["type"].clone());
    let nullable = json!(["integer", "null"]);
    for kind in ["regis", "naplps"] {
        assert_eq!(bounds(&of(kind)["properties"]["line_speed"]), (json!(MIN_LINE_SPEED), json!(MAX_LINE_SPEED), nullable.clone()), "{kind}");
    }
    assert_eq!(bounds(&of("regis")["properties"]["default_rate"]), (json!(1), json!(MAX_DEFAULT_PAINT_RATE), nullable));
    assert!(of("naplps")["properties"].get("default_rate").is_none());
    let text = serde_json::to_string(&schema).unwrap();
    assert_eq!(text.matches("\"const\":\"invalid_paint\"").count(), 1, "the rule, once");
    let painted: Vec<&Value> =
        schema["$defs"]["RuleTrigger"]["oneOf"].as_array().unwrap().iter().filter(|branch| branch["properties"]["event"]["const"] == "painted").collect();
    assert_eq!(painted.len(), 1);
    assert_eq!(painted[0]["properties"]["target"]["$ref"], json!("#/$defs/RuleTarget"));
    assert_eq!(painted[0]["required"], json!(["event", "target"]));
    assert_eq!(painted[0]["additionalProperties"], json!(false));
    // The published file is the schema generated (schema.rs holds them
    // equal), so the file states them too.
    let published = include_str!("../../../../../protocol/schema-16.json");
    assert!(published.contains("\"const\": \"invalid_paint\"") && published.contains("\"const\": \"painted\""));
}
