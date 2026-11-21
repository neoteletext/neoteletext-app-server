//! Every limit of a page's rule table (page-rules.md RULE-57; REQ-6608),
//! sent as program messages: a table or command exactly at the limit is
//! accepted, and one over is refused whole, the refusal naming the limit's
//! constant and its value as `neoteletext_rules::limits` holds them. Each
//! figure is taken from the engine's constants by name, never written
//! here.

#![allow(clippy::arithmetic_side_effects, reason = "a test: its helpers build tables at a limit and one over it")]

extern crate std;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use neoteletext_rules::limits;
use serde_json::{Value, json};

use super::line;
use crate::protocol::{ProgramMessage, Refusal, Rule, read};

/// The limit `name`'s value, as a count.
fn count(name: &str) -> usize {
    usize::try_from(limits::value_of(name).unwrap()).unwrap()
}

/// `table` as a `rules` message for page 101, sub-code 0001.
fn rules(table: &Value) -> Value {
    json!({"type":"rules","connections":"all","page":"101","sub":"0001","table":table})
}

/// What `message` becomes: its table, compiled, or the refusal.
fn outcome(message: &Value) -> Result<Option<crate::rules::Compiled>, Refusal> {
    match read(&line(message))? {
        ProgramMessage::Rules(rules) => crate::protocol::check::compiled(&rules),
        _ => Ok(None),
    }
}

/// The rules for a refusal of a limit: `rules_over_limit` for what only
/// the table's check can count, `too_long` or `out_of_range` where the
/// schema states the bound, and `not_listed` for a designator past the
/// fixed timers or variables (RULE-39).
const LIMIT_RULES: [Rule; 4] = [Rule::RulesOverLimit, Rule::TooLong, Rule::OutOfRange, Rule::NotListed];

/// `at` is accepted; `over` is refused for the limit `name`, its detail
/// naming the constant and its value.
#[track_caller]
fn pair(name: &str, at: &Value, over: &Value) {
    if let Err(refusal) = outcome(at) {
        panic!("{name}: at the limit, refused: {refusal}");
    }
    let Err(refusal) = outcome(over) else { panic!("{name}: one over, accepted") };
    let value = limits::value_of(name).unwrap().to_string();
    assert!(LIMIT_RULES.contains(&refusal.rule), "{name}: {refusal}");
    assert!(refusal.detail.contains(name) && refusal.detail.contains(&value), "{name}: the detail names the limit and {value}: {refusal}");
}

/// A group named `name` of `commands`.
fn group(name: &str, commands: &[Value]) -> Value {
    json!({"name":name,"commands":commands})
}

/// A command that reports `v0`.
fn report() -> Value {
    json!({"type":"report","value":{"of":"variable","variable":"v0"}})
}

/// A group that runs the group named `next`, or reports.
fn chain_group(name: &str, next: Option<&str>) -> Value {
    match next {
        Some(next) => group(name, &[json!({"type":"run","group":next})]),
        None => group(name, &[report()]),
    }
}

/// One group `g` that reports the firing, reading no value, and `rules`.
fn with_rules(rules_list: &[Value]) -> Value {
    rules(&json!({"groups":[group("g", &[json!({"type":"report"})])],"rules":rules_list}))
}

/// A rule on `open` running `g` when `when` holds.
fn rule_when(name: &str, when: &Value) -> Value {
    json!({"name":name,"on":{"event":"open"},"when":when,"run":"g"})
}

/// A term that reads `v0`.
fn term() -> Value {
    json!({"read":{"of":"variable","variable":"v0"},"op":"==","value":0})
}

/// The character key `index`: a distinct ideograph for each index.
fn key(index: usize) -> Value {
    /// The first of the CJK Unified Ideographs, a run of over 20,000
    /// single characters.
    const FIRST: u32 = 0x4E00;
    let character = char::from_u32(FIRST + u32::try_from(index).unwrap()).unwrap();
    json!({"char":character.to_string()})
}

/// The most frames a sprite holds: one for each frame number, 0 to
/// `MAX_FRAME` (ANIM-7).
fn frames_most() -> usize {
    count("MAX_FRAME") + 1
}

/// A sprites command whose sprites hold `frames` frames each, in turn.
fn sprites_of(frames: &[usize]) -> Value {
    let list: Vec<Value> = frames
        .iter()
        .enumerate()
        .map(|(index, &many)| {
            let frames: Vec<Value> = (0..many).map(|number| json!({"source":"local","kind":"passive","number":number})).collect();
            json!({"number":index + 1,"shown":true,"detectable":false,"layer":2,"order":0,"column":10,"row":5,"width":4,"height":3,"frames":frames})
        })
        .collect();
    json!({"type":"sprites","sprites":list})
}

/// A sprites command of `sprites` sprites with `frames` frames each.
fn sprites(sprites: usize, frames: usize) -> Value {
    sprites_of(&alloc::vec![frames; sprites])
}

/// A sprites command whose body is exactly `body` bytes, if one is: the
/// fewest sprites that can hold it, their frames shared out.
fn sprites_sized(body: usize) -> Option<Value> {
    let most_sprites = crate::protocol::MOST_SPRITES;
    (1..=most_sprites).find_map(|many| {
        let base = sprites_body_length(&sprites(many, 1));
        let extra = body.checked_sub(base)?;
        // Each frame is one two-byte object reference (ANIM-7).
        if extra % 2 != 0 || extra / 2 > many * (frames_most() - 1) {
            return None;
        }
        let mut more = extra / 2;
        let frames: Vec<usize> = (0..many)
            .map(|_| {
                let added = more.min(frames_most() - 1);
                more -= added;
                1 + added
            })
            .collect();
        Some(sprites_of(&frames))
    })
}

/// Groups `g0`, `g1`, … whose frames take exactly `total` bytes: commands
/// at the command limit, then the rest filled exactly, each group holding
/// as many as a group may.
fn exact_groups(total: usize) -> Vec<Value> {
    let held = count("MAX_GROUP_COMMANDS");
    let big = sprites_sized(count("MAX_COMMAND_BYTES")).unwrap();
    let big_size = framed(&big);
    for groups in 1..=count("MAX_GROUPS") {
        // Each group adds its own byte of count.
        let commands_bytes = total - groups;
        let bigs = (commands_bytes / big_size).saturating_sub(1);
        let mut all: Vec<Value> = alloc::vec![big.clone(); bigs];
        all.extend(filling(commands_bytes - bigs * big_size, usize::MAX));
        if all.len() <= held * groups {
            return all.chunks(held).enumerate().map(|(index, chunk)| group(&format!("g{index}"), chunk)).collect();
        }
    }
    panic!("no groups take exactly {total} bytes");
}

/// The bytes the groups of `message`'s table take, as framed in the image.
fn groups_bytes(message: &Value) -> usize {
    let table = outcome(message).unwrap_or_else(|refusal| panic!("{refusal}")).unwrap();
    let mut decoded = Box::new(neoteletext_rules::Table::new(0x101, 1));
    neoteletext_rules::decode(table.image(), 0x101, 1, neoteletext_web::records::body_check, &mut decoded).unwrap();
    decoded.groups.as_bytes().len()
}

/// The bytes of `message`'s table image.
fn image_bytes(message: &Value) -> usize {
    outcome(message).unwrap_or_else(|refusal| panic!("{refusal}")).unwrap().image().len()
}

/// The bytes one command takes in a group, framed.
fn framed(command: &Value) -> usize {
    let one = groups_bytes(&rules(&json!({"groups":[group("g", core::slice::from_ref(command))],"rules":[]})));
    let two = groups_bytes(&rules(&json!({"groups":[group("g", &[command.clone(), command.clone()])],"rules":[]})));
    two.checked_sub(one).unwrap()
}

/// Commands whose framed sizes add up to exactly `total`, at most `most`
/// of them, from the sizes of `sprites(1, f)` and two smaller commands.
fn filling(total: usize, most: usize) -> Vec<Value> {
    let mut choices: Vec<(usize, Value)> = alloc::vec![(framed(&report()), report())];
    let set = json!({"type":"variable","op":"set","variable":"v0","value":1});
    choices.push((framed(&set), set));
    for frames in 1..=frames_most() {
        let command = sprites(1, frames);
        choices.push((framed(&command), command));
    }
    // The fewest commands for each total, found by counting up.
    let mut best: Vec<Option<(usize, usize)>> = alloc::vec![None; total.saturating_add(1)];
    best[0] = Some((0, 0));
    for sum in 1..=total {
        best[sum] = choices
            .iter()
            .enumerate()
            .filter_map(|(index, (size, _))| sum.checked_sub(*size).and_then(|rest| best[rest]).map(|(used, _)| (used.saturating_add(1), index)))
            .min();
    }
    let mut commands = Vec::new();
    let mut sum = total;
    while sum > 0 {
        let (_, index) = best[sum].unwrap_or_else(|| panic!("no commands add up to {sum}"));
        commands.push(choices[index].1.clone());
        sum = sum.checked_sub(choices[index].0).unwrap();
    }
    assert!(commands.len() <= most, "{} commands for {total} bytes", commands.len());
    commands
}

/// `MAX_GROUP_COMMANDS`, `MAX_GROUPS`, `MAX_GROUP_DEPTH`, `MAX_RUN_COMMANDS`
/// (RULE-118, RULE-120).
#[test]
fn the_limits_of_groups() {
    let commands = |many: usize| rules(&json!({"groups":[group("g", &alloc::vec![report(); many])],"rules":[]}));
    pair("MAX_GROUP_COMMANDS", &commands(count("MAX_GROUP_COMMANDS")), &commands(count("MAX_GROUP_COMMANDS") + 1));
    let groups = |many: usize| {
        let list: Vec<Value> = (0..many).map(|index| group(&format!("g{index}"), &[report()])).collect();
        rules(&json!({"groups":list,"rules":[]}))
    };
    pair("MAX_GROUPS", &groups(count("MAX_GROUPS")), &groups(count("MAX_GROUPS") + 1));
    let chain = |depth: usize| {
        let list: Vec<Value> =
            (0..depth).map(|level| chain_group(&format!("g{level}"), (level + 1 < depth).then(|| format!("g{}", level + 1)).as_deref())).collect();
        rules(&json!({"groups":list,"rules":[{"name":"r","on":{"event":"open"},"run":"g0"}]}))
    };
    pair("MAX_GROUP_DEPTH", &chain(count("MAX_GROUP_DEPTH")), &chain(count("MAX_GROUP_DEPTH") + 1));
    // A group of one fewer commands than a group holds, run so many times
    // that the runs apply exactly the most one run may; one more command
    // is one over.
    let held = count("MAX_GROUP_COMMANDS");
    let runs = count("MAX_RUN_COMMANDS") / held;
    assert_eq!(runs * held, count("MAX_RUN_COMMANDS"));
    let run_size = |extra: usize| {
        let full: Vec<Value> = (1..held).map(|_| report()).collect();
        let mut top: Vec<Value> = (0..runs).map(|_| json!({"type":"run","group":"full"})).collect();
        top.extend((0..extra).map(|_| report()));
        rules(&json!({"groups":[group("full", &full),group("top", &top)],"rules":[{"name":"r","on":{"event":"open"},"run":"top"}]}))
    };
    pair("MAX_RUN_COMMANDS", &run_size(0), &run_size(1));
}

/// `MAX_COMMAND_BYTES` and `MAX_COMMAND_BINDINGS` (RULE-106, RULE-118).
#[test]
fn the_limits_of_a_command() {
    // Sprites whose body is exactly the most a command holds; every
    // SPRITES body is of an even length, so one over is two bytes more.
    let most = count("MAX_COMMAND_BYTES");
    assert!(sprites_sized(most + 1).is_none());
    let at = sprites_sized(most);
    let over = sprites_sized(most + 2);
    let one_rule = |command: Value| rules(&json!({"rules":[{"name":"r","on":{"event":"key","keys":"any"},"do":[command]}]}));
    pair("MAX_COMMAND_BYTES", &one_rule(at.expect("sprites of exactly the command limit")), &one_rule(over.unwrap()));
    // Each `move` binds `x` and `y`: as many operations as make the most
    // bindings a command takes, and one binding more.
    let bound = |name: &str| json!({"plus":0,"variable":name});
    let moves = |bindings: usize| {
        let ops: Vec<Value> = (0..bindings.div_ceil(2))
            .map(|index| {
                let y = if index * 2 + 1 < bindings { bound("v1") } else { json!(0) };
                json!({"action":"move","target":{"object":index + 1},"x":bound("v0"),"y":y,"relative":false})
            })
            .collect();
        one_rule(json!({"type":"animate","ops":ops}))
    };
    pair("MAX_COMMAND_BINDINGS", &moves(count("MAX_COMMAND_BINDINGS")), &moves(count("MAX_COMMAND_BINDINGS") + 1));
}

/// The length of the SPRITES body `command` places.
fn sprites_body_length(command: &Value) -> usize {
    let message = json!({"type":"sprites","connections":"all","page":"101","sub":"0001","sprites":command["sprites"]});
    let ProgramMessage::Sprites(direct) = read(&line(&message)).unwrap() else { panic!("not sprites") };
    crate::animation::sprites_body(0x101, 1, &direct.sprites).unwrap().len()
}

/// `MAX_GROUPS_BYTES` (RULE-120): groups whose frames come to exactly the
/// limit, and one command more.
#[test]
fn the_limit_of_the_groups_bytes() {
    let most = count("MAX_GROUPS_BYTES");
    let mut groups = exact_groups(most);
    let at = rules(&json!({"groups":groups,"rules":[]}));
    assert_eq!(groups_bytes(&at), most);
    groups.push(group("more", &[report()]));
    pair("MAX_GROUPS_BYTES", &at, &rules(&json!({"groups":groups,"rules":[]})));
}

/// `MAX_TABLE_BYTES` (RULE-41): a table whose image is exactly the limit,
/// and one key more.
#[test]
fn the_limit_of_the_image() {
    let most = count("MAX_TABLE_BYTES");
    let keys_held = count("MAX_KEYS_PER_TRIGGER");
    let triggers = count("MAX_TRIGGERS_PER_RULE");
    // Groups near their own limit, then rules of full key sets, then rows
    // (four bytes each) and `not`s (one byte each) for the rest.
    let groups = exact_groups(count("MAX_GROUPS_BYTES"));
    let full_trigger = |offset: usize| json!({"event":"key","keys":(0..keys_held).map(|index| key(offset + index)).collect::<Vec<_>>()});
    let rule = |index: usize, keys_last: usize, nots: usize| {
        let mut on: Vec<Value> = (0..triggers - 1).map(|trigger| full_trigger(trigger * keys_held)).collect();
        on.push(json!({"event":"key","keys":(0..keys_last).map(|place| key(500 + place)).collect::<Vec<_>>()}));
        let mut when = term();
        for _ in 0..nots {
            when = json!({ "not": when });
        }
        json!({"name":format!("r{index}"),"on":on,"when":when,"run":"g0"})
    };
    let table = |rules_list: &[Value], rows: usize| {
        let mut table = json!({"groups":groups,"rules":rules_list});
        if rows > 0 {
            table["rows"] = json!({"a":(0..rows).map(|row| json!(row)).collect::<Vec<_>>()});
        }
        rules(&table)
    };
    // Whole rules while a whole one more fits.
    let mut list: Vec<Value> = Vec::new();
    loop {
        let mut more = list.clone();
        more.push(rule(more.len(), keys_held, 0));
        if image_bytes(&table(&more, 0)) > most - 300 {
            break;
        }
        list = more;
    }
    // Keys of the last rule's last trigger until the rest is a few rows
    // and `not`s.
    let rows_most = count("MAX_ROWS");
    let nots_most = count("MAX_CONDITION_DEPTH");
    let mut keys_last = 1;
    let mut last_rule = rule(list.len(), keys_last, 0);
    loop {
        let mut with_last = list.clone();
        with_last.push(last_rule.clone());
        let rest = most - image_bytes(&table(&with_last, 0));
        if rest <= 4 * rows_most {
            let rows = rest / 4;
            let nots = rest % 4;
            assert!(nots <= nots_most);
            with_last.pop();
            with_last.push(rule(list.len(), keys_last, nots));
            let at = table(&with_last, rows);
            assert_eq!(image_bytes(&at), most);
            with_last.pop();
            with_last.push(rule(list.len(), keys_last + 1, nots));
            pair("MAX_TABLE_BYTES", &at, &table(&with_last, rows));
            return;
        }
        keys_last += 1;
        assert!(keys_last < keys_held, "the rest is too large to fill with keys");
        last_rule = rule(list.len(), keys_last, 0);
    }
}

/// `MAX_RULES`, `MAX_ROWS`, `MAX_RULE_ROWS`, `MAX_RECTS` (RULE-3, RULE-13,
/// RULE-43, RULE-105).
#[test]
fn the_limits_of_rules_rows_and_rectangles() {
    let many_rules = |many: usize| with_rules(&(0..many).map(|index| json!({"name":format!("r{index}"),"on":{"event":"open"},"run":"g"})).collect::<Vec<_>>());
    pair("MAX_RULES", &many_rules(count("MAX_RULES")), &many_rules(count("MAX_RULES") + 1));
    let rows = |many: usize| rules(&json!({"rows":{"a":(0..many).map(|row| json!(row)).collect::<Vec<_>>()},"rules":[]}));
    pair("MAX_ROWS", &rows(count("MAX_ROWS")), &rows(count("MAX_ROWS") + 1));
    // Rules each a template of the whole row set, then rules without rows
    // counting one each.
    let set = count("MAX_ROWS");
    let templates = count("MAX_RULE_ROWS") / set;
    let rule_rows = |plain: usize| {
        let mut list: Vec<Value> = (0..templates)
            .map(|index| json!({"name":format!("t{index}"),"rows":"a","on":{"event":"press","target":{"object":{"plus":1,"row":"p"}}},"run":"g"}))
            .collect();
        list.extend((0..plain).map(|index| json!({"name":format!("p{index}"),"on":{"event":"open"},"run":"g"})));
        rules(&json!({"groups":[group("g", &[report()])],"rows":{"a":(0..set).map(|row| json!(row)).collect::<Vec<_>>()},"rules":list}))
    };
    let plain = count("MAX_RULE_ROWS") - templates * set;
    pair("MAX_RULE_ROWS", &rule_rows(plain), &rule_rows(plain + 1));
    let rects = |many: usize| {
        let list: Vec<Value> = (0..many).map(|index| json!({"name":format!("q{index}"),"x":0,"y":0,"width":1,"height":1})).collect();
        rules(&json!({"rects":list,"rules":[]}))
    };
    pair("MAX_RECTS", &rects(count("MAX_RECTS")), &rects(count("MAX_RECTS") + 1));
}

/// `MAX_SEQUENCES`, `MAX_SEQUENCE_STEPS`, `MAX_SEQUENCE_TICKS`,
/// `MAX_SEQUENCE_REPEAT` (RULE-103).
#[test]
fn the_limits_of_sequences() {
    let sequence = |name: &str, length: usize, repeat: usize, steps: usize| {
        let list: Vec<Value> = (0..steps).map(|step| json!({"tick":step,"run":"g"})).collect();
        json!({"name":name,"length":length,"repeat":repeat,"steps":list})
    };
    let table = |sequences: &[Value]| rules(&json!({"groups":[group("g", &[report()])],"sequences":sequences,"rules":[]}));
    let many = |count: usize| table(&(0..count).map(|index| sequence(&format!("s{index}"), 1, 1, 1)).collect::<Vec<_>>());
    pair("MAX_SEQUENCES", &many(count("MAX_SEQUENCES")), &many(count("MAX_SEQUENCES") + 1));
    let steps = count("MAX_SEQUENCE_STEPS");
    pair("MAX_SEQUENCE_STEPS", &table(&[sequence("s", steps + 1, 1, steps)]), &table(&[sequence("s", steps + 1, 1, steps + 1)]));
    let ticks = count("MAX_SEQUENCE_TICKS");
    pair("MAX_SEQUENCE_TICKS", &table(&[sequence("s", ticks, 1, 1)]), &table(&[sequence("s", ticks + 1, 1, 1)]));
    let repeat = count("MAX_SEQUENCE_REPEAT");
    pair("MAX_SEQUENCE_REPEAT", &table(&[sequence("s", 1, repeat, 1)]), &table(&[sequence("s", 1, repeat + 1, 1)]));
}

/// `TIMERS`, `VARIABLES` (RULE-14, RULE-15): the last designator, and the one
/// past it.
#[test]
fn the_fixed_timers_and_variables() {
    let timer = |number: usize| with_rules(&[json!({"name":"r","on":{"event":"open"},"do":[{"type":"timer","op":"start","timer":format!("t{number}")}]})]);
    pair("TIMERS", &timer(count("TIMERS") - 1), &timer(count("TIMERS")));
    let variable = |number: usize| {
        with_rules(&[json!({"name":"r","on":{"event":"open"},"do":[{"type":"variable","op":"set","variable":format!("v{number}"),"value":1}]})])
    };
    pair("VARIABLES", &variable(count("VARIABLES") - 1), &variable(count("VARIABLES")));
}

/// A tree of `and`s `depth` operators deep, over one term.
fn nested(depth: usize) -> Value {
    let mut when = term();
    for _ in 0..depth {
        when = json!({"and":[when, term()]});
    }
    when
}

/// `MAX_CONDITION_TERMS`, `MAX_CONDITION_DEPTH`, `MAX_CONDITION_NODES`,
/// `MAX_SET_MEMBERS`, `MAX_AND_OR_CHILDREN` (RULE-18).
#[test]
fn the_limits_of_a_condition() {
    let one = |when: &Value| with_rules(&[rule_when("r", when)]);
    // Terms under one `or` of `or`s, so that no `and` has more children
    // than it may.
    let terms = |many: usize| {
        let children = count("MAX_AND_OR_CHILDREN");
        let parts: Vec<Value> = (0..many)
            .collect::<Vec<_>>()
            .chunks(children / 2)
            .map(|chunk| if chunk.len() == 1 { term() } else { json!({"or":chunk.iter().map(|_| term()).collect::<Vec<_>>()}) })
            .collect();
        json!({ "and": parts })
    };
    pair("MAX_CONDITION_TERMS", &one(&terms(count("MAX_CONDITION_TERMS"))), &one(&terms(count("MAX_CONDITION_TERMS") + 1)));
    // An `and` over a term and an `and`, so deep.
    let depth = count("MAX_CONDITION_DEPTH");
    pair("MAX_CONDITION_DEPTH", &one(&nested(depth)), &one(&nested(depth + 1)));
    // `not`s over one term make nodes without terms; within the depth,
    // spread over the children of one `and`.
    let nodes = |many: usize| {
        let children = count("MAX_AND_OR_CHILDREN");
        let under = depth - 1;
        // The `and` and its children's terms are nodes; each `not` is one
        // more.
        let mut nots_left = many.saturating_sub(1 + children);
        let parts: Vec<Value> = (0..children)
            .map(|_| {
                let mut part = term();
                for _ in 0..under.min(nots_left) {
                    part = json!({ "not": part });
                }
                nots_left = nots_left.saturating_sub(under);
                part
            })
            .collect();
        json!({ "and": parts })
    };
    let node_most = count("MAX_CONDITION_NODES");
    assert!(node_most <= 1 + count("MAX_AND_OR_CHILDREN") * depth, "the nodes can be reached within the depth");
    pair("MAX_CONDITION_NODES", &one(&nodes(node_most)), &one(&nodes(node_most + 1)));
    let set = |many: usize| json!({"read":{"of":"variable","variable":"v0"},"in_set":(0..many).collect::<Vec<_>>()});
    pair("MAX_SET_MEMBERS", &one(&set(count("MAX_SET_MEMBERS"))), &one(&set(count("MAX_SET_MEMBERS") + 1)));
    // One more child than an `and` holds is also one more term than a
    // condition holds; the schema's bound on the children names its own
    // limit first.
    let children = |many: usize| json!({"and":(0..many).map(|_| term()).collect::<Vec<_>>()});
    pair("MAX_AND_OR_CHILDREN", &one(&children(count("MAX_AND_OR_CHILDREN"))), &one(&children(count("MAX_AND_OR_CHILDREN") + 1)));
}

/// `MAX_TRIGGERS_PER_RULE`, `MAX_KEYS_PER_TRIGGER`, `MAX_TIMER_TICKS`, `MAX_CUE`
/// (RULE-9, RULE-10, RULE-107, RULE-109).
#[test]
fn the_limits_of_triggers() {
    let triggers =
        |many: usize| with_rules(&[json!({"name":"r","on":(0..many).map(|index| json!({"event":"key","keys":[key(index)]})).collect::<Vec<_>>(),"run":"g"})]);
    pair("MAX_TRIGGERS_PER_RULE", &triggers(count("MAX_TRIGGERS_PER_RULE")), &triggers(count("MAX_TRIGGERS_PER_RULE") + 1));
    let keys = |many: usize| with_rules(&[json!({"name":"r","on":{"event":"key","keys":(0..many).map(key).collect::<Vec<_>>()},"run":"g"})]);
    pair("MAX_KEYS_PER_TRIGGER", &keys(count("MAX_KEYS_PER_TRIGGER")), &keys(count("MAX_KEYS_PER_TRIGGER") + 1));
    let every = |ticks: usize| with_rules(&[json!({"name":"r","on":{"event":"every","ticks":ticks,"timer":"t0"},"run":"g"})]);
    pair("MAX_TIMER_TICKS", &every(count("MAX_TIMER_TICKS")), &every(count("MAX_TIMER_TICKS") + 1));
    let cue = |number: usize| with_rules(&[json!({"name":"r","on":{"event":"cue","channel":1,"number":number},"run":"g"})]);
    pair("MAX_CUE", &cue(count("MAX_CUE")), &cue(count("MAX_CUE") + 1));
}

/// `MAX_TABLE_VALUES`, `MAX_TABLE_TARGETS`, `MAX_WATCHED` (RULE-33, RULE-47).
#[test]
fn the_limits_of_what_a_table_reads_and_names() {
    // Distinct keys read, eight terms a rule.
    let terms_held = count("MAX_CONDITION_TERMS");
    let values = |many: usize| {
        let list: Vec<Value> = (0..many)
            .collect::<Vec<_>>()
            .chunks(terms_held)
            .enumerate()
            .map(|(index, chunk)| {
                let read: Vec<Value> = chunk.iter().map(|&place| json!({"read":{"of":"key","key":key(place)},"op":"==","value":0})).collect();
                rule_when(&format!("r{index}"), &if read.len() == 1 { read[0].clone() } else { json!({ "and": read }) })
            })
            .collect();
        with_rules(&list)
    };
    pair("MAX_TABLE_VALUES", &values(count("MAX_TABLE_VALUES")), &values(count("MAX_TABLE_VALUES") + 1));
    // Distinct objects shown, a few operations a command.
    let targets = |many: usize| {
        let commands: Vec<Value> = (1..=many)
            .collect::<Vec<_>>()
            .chunks(8)
            .map(|chunk| json!({"type":"animate","ops":chunk.iter().map(|&object| json!({"action":"show","target":{"object":object}})).collect::<Vec<_>>()}))
            .collect();
        rules(&json!({"groups":[group("g", &commands)],"rules":[]}))
    };
    pair("MAX_TABLE_TARGETS", &targets(count("MAX_TABLE_TARGETS")), &targets(count("MAX_TABLE_TARGETS") + 1));
    // Distinct metered values: a synthesizer's envelope level and voices of each
    // part, on audio channels in turn.
    let watched = |many: usize| {
        let reads: Vec<Value> = (0..many)
            .map(|index| {
                let property = if index % 2 == 0 { "envelope_level" } else { "voices" };
                json!({"read":{"of":"synth","channel":index / 32,"family":"synth106","part":(index / 2) % 16 + 1,"property":property},"op":"==","value":0})
            })
            .collect();
        let list: Vec<Value> = reads
            .chunks(terms_held)
            .enumerate()
            .map(|(index, chunk)| rule_when(&format!("r{index}"), &if chunk.len() == 1 { chunk[0].clone() } else { json!({ "and": chunk }) }))
            .collect();
        with_rules(&list)
    };
    pair("MAX_WATCHED", &watched(count("MAX_WATCHED")), &watched(count("MAX_WATCHED") + 1));
}

/// `MAX_PAINT_WATCHED` (neoteletext-common's DEC-7031 item 4): the distinct
/// numbered drawings a table's `painted` triggers name.
#[test]
fn the_limit_of_painted_drawings() {
    let painted = |many: usize| {
        let list: Vec<Value> =
            (1..=many).map(|drawing| json!({"name":format!("r{drawing}"),"on":{"event":"painted","target":{"object":drawing}},"run":"g"})).collect();
        with_rules(&list)
    };
    pair("MAX_PAINT_WATCHED", &painted(count("MAX_PAINT_WATCHED")), &painted(count("MAX_PAINT_WATCHED") + 1));
}

/// `MAX_FRAME`, `MAX_CYCLE_PERIOD` (ANIM-14): an operation's frame and a
/// cycle's period, in a group.
#[test]
fn the_limits_of_frames() {
    let op = |op: Value| with_rules(&[json!({"name":"r","on":{"event":"open"},"do":[{"type":"animate","ops":[op]}]})]);
    let frame = |number: usize| op(json!({"action":"frame","target":{"object":1},"frame":number}));
    pair("MAX_FRAME", &frame(count("MAX_FRAME")), &frame(count("MAX_FRAME") + 1));
    let period = |ticks: usize| op(json!({"action":"cycle","target":{"object":1},"first":0,"last":1,"period":ticks,"once":false}));
    pair("MAX_CYCLE_PERIOD", &period(count("MAX_CYCLE_PERIOD")), &period(count("MAX_CYCLE_PERIOD") + 1));
}

/// RULE-57 (REQ-6608): every limit that bounds a table is tested above,
/// and the rest bound what a receiver does with it, which no program
/// message reaches. The two lists are the whole table.
#[test]
fn every_limit_is_tested_or_a_receivers() {
    const TESTED: [&str; 33] = [
        "MAX_GROUP_COMMANDS",
        "MAX_GROUPS",
        "MAX_GROUP_DEPTH",
        "MAX_RUN_COMMANDS",
        "MAX_COMMAND_BYTES",
        "MAX_GROUPS_BYTES",
        "MAX_RULES",
        "MAX_ROWS",
        "MAX_SEQUENCES",
        "MAX_SEQUENCE_STEPS",
        "MAX_SEQUENCE_REPEAT",
        "TIMERS",
        "VARIABLES",
        "MAX_CONDITION_TERMS",
        "MAX_CONDITION_DEPTH",
        "MAX_CONDITION_NODES",
        "MAX_SET_MEMBERS",
        "MAX_AND_OR_CHILDREN",
        "MAX_TRIGGERS_PER_RULE",
        "MAX_KEYS_PER_TRIGGER",
        "MAX_RULE_ROWS",
        "MAX_COMMAND_BINDINGS",
        "MAX_RECTS",
        "MAX_SEQUENCE_TICKS",
        "MAX_TIMER_TICKS",
        "MAX_CUE",
        "MAX_TABLE_VALUES",
        "MAX_TABLE_TARGETS",
        "MAX_WATCHED",
        "MAX_PAINT_WATCHED",
        "MAX_TABLE_BYTES",
        "MAX_CYCLE_PERIOD",
        "MAX_FRAME",
    ];
    // Per-tick budgets and the receiver's own state (RULE-4, RULE-5,
    // RULE-8, RULE-12), the record's fragment, which the server cuts
    // (RULE-41), the state query's answer, which a receiver writes
    // (RULE-47), and how far ahead of the present page tick a sequence
    // may be started, which only the receiver's clock knows (RULE-103).
    const RECEIVERS: [&str; 10] = [
        "MAX_START_TICKS",
        "MAX_EVENTS_PER_TICK",
        "MAX_EVENT_COMMANDS_PER_TICK",
        "MAX_EVENT_REPORTS_PER_TICK",
        "MAX_QUEUED_EVENTS",
        "MAX_KEYS_DOWN",
        "MAX_POINTER_OBJECTS",
        "MAX_TABLE_FRAGMENT",
        "MAX_STATE_BYTES",
        "MAX_CATCH_UP_TICKS",
    ];
    let mut listed: Vec<&str> = TESTED.iter().chain(RECEIVERS.iter()).copied().collect();
    listed.sort_unstable();
    let mut table: Vec<&str> = limits::RULE_57.iter().map(|(name, _)| *name).collect();
    table.sort_unstable();
    assert_eq!(listed, table);
}
