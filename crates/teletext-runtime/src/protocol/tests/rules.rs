//! The second form of the page rule table in program protocol 15
//! (page-rules.md RULE-35 to RULE-39, RULE-57, RULE-59, RULE-93, RULE-114;
//! REQ-6532, REQ-6608; DEC-6587, DEC-6635): every worked example accepted,
//! encoded and kept as a vector; every refusal; every limit at its bound
//! and one over.

extern crate std;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_rules::limits;
use serde_json::{Value, json};

use super::{accepted, line, refused};
use crate::protocol::{CommandMessage, ProgramMessage, Rule, read};

/// The worked examples of page-rules.md section 7.5, RULE-35's and the
/// music player's table of RULE-93, one a line, as the specification at
/// neoteletext-common rules-2 36ad145 writes them.
const EXAMPLES: &str = include_str!("rules-examples.jsonl");
/// The command messages of RULE-114, one a line.
const COMMANDS: &str = include_str!("command-examples.jsonl");
/// Each example's table image, in hexadecimal, as the server encodes it.
const VECTORS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/protocol/tests/rules-vectors.tsv");
/// Set to write the vectors afresh rather than compare them.
const BLESS: &str = "TELETEXT_BLESS_RULES";

/// The animation messages of animation.md ANIM-21, one a line.
pub(crate) const ANIMATION_EXAMPLES: &str = include_str!("animation-examples.jsonl");

/// REQ-6533, ANIM-21: every example of animation.md's messages is read.
#[test]
fn every_example_of_animation_is_read() {
    let kinds: Vec<String> = ANIMATION_EXAMPLES
        .lines()
        .map(|line| {
            let message = read(line.as_bytes()).unwrap_or_else(|refusal| panic!("{refusal}: {line}"));
            format!("{:?}", message.kind())
        })
        .collect();
    assert_eq!(kinds, ["Frame", "Frame", "Animate", "Sprites", "NaplpsObjects", "SpriteSheet", "Timeline", "TimelineStop"]);
}

/// The examples, read.
fn examples() -> Vec<Value> {
    EXAMPLES.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

/// The table `value` compiles to, or its refusal.
fn compiled(value: &Value) -> Result<crate::rules::Compiled, crate::protocol::Refusal> {
    match read(&line(value))? {
        ProgramMessage::Rules(message) => Ok(crate::protocol::check::compiled(&message)?.unwrap()),
        other => panic!("not a rules message: {other:?}"),
    }
}

/// `table` as a `rules` message for page 101, sub-code 0001.
fn rules(table: &Value) -> Value {
    json!({"type":"rules","connections":"all","page":"101","sub":"0001","table":table})
}

/// The image's hexadecimal digits.
fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// RULE-35, RULE-59, RULE-85 (REQ-6532): every worked example and the music
/// player's table are accepted, and the image each encodes to decodes with
/// the receivers' crate and encodes again to the same bytes. The images are
/// kept as vectors (`TELETEXT_BLESS_RULES=1` writes them).
#[test]
fn every_worked_example_encodes_and_decodes_the_same() {
    let mut vectors = String::from("# page\tsub\timage, as the server encodes each worked example of page-rules.md 7.5, RULE-35 and RULE-93\n");
    for example in examples() {
        let table = compiled(&example).unwrap_or_else(|refusal| panic!("refused {example}: {refusal}"));
        let image = table.image();
        let page = u16::from_str_radix(example["page"].as_str().unwrap(), 16).unwrap();
        let sub = u16::from_str_radix(example["sub"].as_str().unwrap(), 16).unwrap();
        let mut decoded = alloc::boxed::Box::new(neoteletext_rules::Table::new(page, sub));
        neoteletext_rules::decode(image, page, sub, neoteletext_web::records::body_check, &mut decoded).unwrap();
        let mut again = alloc::vec![0_u8; limits::MAX_TABLE_BYTES];
        let length = neoteletext_rules::encode(&decoded, neoteletext_web::records::body_check, &mut again).unwrap();
        assert_eq!(&again[..length], image, "{}", example["page"]);
        vectors.push_str(example["page"].as_str().unwrap());
        vectors.push('\t');
        vectors.push_str(example["sub"].as_str().unwrap());
        vectors.push('\t');
        vectors.push_str(&hex(image));
        vectors.push('\n');
    }
    if std::env::var_os(BLESS).is_some() {
        std::fs::write(VECTORS, &vectors).unwrap();
    }
    assert_eq!(std::fs::read_to_string(VECTORS).unwrap(), vectors, "the vectors are not what the examples encode to: run with {BLESS}=1");
}

/// RULE-93 (REQ-6541, DEC-6635): the music player's table is sixteen
/// rules, twenty-two groups and one sequence, and its rules have their
/// names.
#[test]
fn the_music_players_table_has_its_shape() {
    let player = examples().into_iter().find(|example| example["table"]["rules"].as_array().unwrap().len() == 16).unwrap();
    let table = compiled(&player).unwrap();
    let image = table.image();
    // The image's header (RULE-41): rules at byte 4, groups at 5,
    // sequences at 6, rows at 3.
    assert_eq!((image[3], image[4], image[5], image[6]), (25, 16, 22, 1));
    assert_eq!(table.rule(0).unwrap().0, "start");
    assert_eq!(table.rule(15).unwrap().0, "panic_expire");
}

/// RULE-53 (REQ-6532): the default table is byte for byte the engine's
/// `default_send_all`, with and without its `wheel` rule, as the shared
/// wire vectors hold it.
#[test]
fn the_default_table_is_the_engines() {
    for (wheel, name) in [(false, "default_send_all"), (true, "default_send_all_wheel")] {
        let row = neoteletext_web::wire_vectors::vectors().find(|row| row.kind == "rules" && row.name == name).unwrap();
        let (vector, length) = neoteletext_web::wire_vectors::bytes_of(row.value).unwrap();
        let page = u16::from_str_radix("101", 16).unwrap();
        let table = crate::rules::Compiled::send_all(wheel, page, 1).unwrap();
        assert_eq!(table.image(), &vector[..length], "{name}");
    }
}

/// RULE-114 (REQ-6532): every command message of the specification is
/// accepted, and its record is the same command's bytes in a group.
#[test]
fn every_command_message_is_accepted_and_is_its_group_command() {
    let flash = json!({"name":"flash","commands":[{"type":"report"}]});
    let sequences = json!([
        {"name":"pulse","length":30,"repeat":0,"steps":[{"tick":0,"run":"flash"}]},
        {"name":"flash_once","length":1,"repeat":1,"steps":[{"tick":0,"run":"flash"}]}]);
    for example in COMMANDS.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()) {
        let message = match accepted(&example) {
            ProgramMessage::LocalMidi(message) => CommandMessage::LocalMidi(message),
            ProgramMessage::Variable(message) => CommandMessage::Variable(message),
            ProgramMessage::Timer(message) => CommandMessage::Timer(message),
            ProgramMessage::Report(message) => CommandMessage::Report(message),
            ProgramMessage::Run(message) => CommandMessage::Run(message),
            ProgramMessage::Sequence(message) => CommandMessage::Sequence(message),
            other => panic!("{other:?}"),
        };
        let mut command = example.clone();
        for field in ["connections", "page", "sub"] {
            command.as_object_mut().unwrap().remove(field);
        }
        let table = rules(&json!({"groups":[flash.clone(), {"name":"wrapped","commands":[command]}],"sequences":sequences,"rules":[]}));
        let table = compiled(&table).unwrap_or_else(|refusal| panic!("{example}: {refusal}"));
        let direct = crate::rules::direct(&message, Some(&table), &mut 0).unwrap();
        let built = crate::protocol::check::direct_record(&message, table.names()).unwrap();
        assert!(!direct.is_empty() && built.bindings.is_empty(), "{example}");
        // The group's one command is the same record type and body.
        let image = table.image();
        let framed = image.windows(built.body.len()).any(|window| window == built.body.as_slice());
        assert!(framed, "{example}: the group holds the same body");
    }
}

/// RULE-39, RULE-114 (REQ-6532): what is a group's alone is refused when
/// sent directly.
#[test]
fn a_command_sent_directly_has_constants_alone() {
    let base = json!({"type":"variable","connections":"all","page":"101","sub":"0001","op":"set","variable":"v0","value":{"plus":0,"fired":"key"}});
    refused(&base, "value", Rule::InvalidCommand);
    refused(&json!({"type":"report","connections":"all","page":"101","sub":"0001"}), "value", Rule::InvalidCommand);
    let midi = json!({"type":"local_midi","connections":"all","page":"101","sub":"0001","channel":0,
        "message":{"event":"note_on","channel":1,"key":{"plus":0,"variable":"v0"},"velocity":100}});
    refused(&midi, "message.key", Rule::InvalidCommand);
    refused(&json!({"type":"run","connections":"all","page":"101","sub":"0001","group":"g","at":null}), "at", Rule::UnknownField);
    refused(&json!({"type":"sequence","connections":"all","page":"101","sub":"0001","op":"stop","sequence":null,"at":{"tick":5}}), "at", Rule::InvalidRules);
}

/// The table of one rule on a key with `commands`.
fn one_rule(commands: &Value) -> Value {
    rules(&json!({"rules":[{"name":"r","on":{"event":"key","keys":"any"},"do":commands}]}))
}

/// RULE-39 (REQ-6532, DEC-6635): each fault of a table, refused whole with
/// its rule and path.
#[test]
fn each_fault_of_a_table_is_refused() {
    let held = rules(&json!({"rules":[{"name":"r","on":{"event":"held","keys":"any","ticks":3},"do":[{"type":"report"}]}]}));
    refused(&held, "table.rules[0].on.event", Rule::WrongType);
    let both =
        rules(&json!({"groups":[{"name":"g","commands":[{"type":"report"}]}],"rules":[{"name":"r","on":{"event":"open"},"run":"g","do":[{"type":"report"}]}]}));
    refused(&both, "table.rules[0]", Rule::InvalidRules);
    let by_row = rules(&json!({"rules":[{"name":"r","on":{"event":"key","keys":[{"char":"a"}],"by_row":true},"do":[{"type":"report"}]}]}));
    refused(&by_row, "table.rules[0].on.by_row", Rule::InvalidRules);
    let count = rules(
        &json!({"rows":{"notes":[60,62]},"rules":[{"name":"r","rows":"notes","on":{"event":"key","keys":[{"char":"a"}],"by_row":true},"do":[{"type":"report"}]}]}),
    );
    refused(&count, "table.rules[0].on.keys", Rule::InvalidRules);
    let phase = rules(
        &json!({"rules":[{"name":"r","on":{"event":"key","keys":"any"},"when":{"read":{"of":"timer","timer":"t0","property":"phase"},"op":"==","value":0},"do":[{"type":"report"}]}]}),
    );
    refused(&phase, "table.rules[0].when.read.period", Rule::InvalidRules);
    let becomes = rules(&json!({"rules":[{"name":"r","on":{"event":"becomes"},"do":[{"type":"report"}]}]}));
    refused(&becomes, "table.rules[0].on", Rule::InvalidRules);
    let unknown = rules(&json!({"rules":[{"name":"r","on":{"event":"open"},"run":"nothing"}]}));
    refused(&unknown, "table.rules[0].run", Rule::UnknownName);
    let twice =
        rules(&json!({"rules":[{"name":"r","on":{"event":"open"},"do":[{"type":"report"}]},{"name":"r","on":{"event":"close"},"do":[{"type":"report"}]}]}));
    refused(&twice, "table.rules[1].name", Rule::DuplicateName);
    let page_play = one_rule(&json!([{"type":"sound","kind":"play","channel":0,"handle":1,"generation":1,"gain":100,"fade_ms":5,"page":"101"}]));
    refused(&page_play, "table.rules[0].do[0].page", Rule::InvalidGroup);
    let forbidden = one_rule(&json!([{"type":"page","page":{}}]));
    refused(&forbidden, "table.rules[0].do[0].type", Rule::InvalidGroup);
    let variable_target = one_rule(&json!([{"type":"animate","ops":[{"action":"show","target":{"object":{"plus":1,"variable":"v0"}}}]}]));
    refused(&variable_target, "table.rules[0].do[0].ops[0].target.object.variable", Rule::InvalidGroup);
    let rect = rules(&json!({"rects":[{"name":"page","x":0,"y":0,"width":1,"height":1}],"rules":[]}));
    refused(&rect, "table.rects[0].name", Rule::Reserved);
    let row_outside = rules(&json!({"rules":[{"name":"r","on":{"event":"press","target":{"object":{"plus":1,"row":"p"}}},"do":[{"type":"report"}]}]}));
    refused(&row_outside, "table.rules[0].on.target", Rule::InvalidRules);
    let field = rules(&json!({"rules":[{"name":"r","on":{"event":"becomes"},
        "when":{"read":{"of":"synth","channel":0,"family":"synth106","part":1,"property":"field","field":"nothing"},"op":"==","value":0},"do":[{"type":"report"}]}]}));
    refused(&field, "table.rules[0].when.read.field", Rule::UnknownParameter);
}

/// A `rules` message of `list`.
fn rules_of(list: &[Value]) -> Value {
    rules(&json!({ "rules": list }))
}

/// RULE-35 (REQ-6532): equal `do` lists share one unnamed group, and
/// `echo` adds a report of the firing.
#[test]
fn equal_do_lists_share_a_group() {
    let shared = rules_of(&[
        json!({"name":"a","on":{"event":"open"},"do":[{"type":"report"}]}),
        json!({"name":"b","on":{"event":"close"},"do":[{"type":"report"}]}),
        json!({"name":"c","on":{"event":"focus"},"do":[],"echo":true}),
    ]);
    // An empty `do` is no group: refused at its path.
    refused(&shared, "table.rules[2].do", Rule::TooShort);
    let table = compiled(&rules_of(&[
        json!({"name":"a","on":{"event":"open"},"do":[{"type":"report"}]}),
        json!({"name":"b","on":{"event":"close"},"do":[{"type":"report"}]}),
    ]))
    .unwrap();
    assert_eq!(table.image()[5], 1, "one group for two equal lists");
}

/// The sprites and NAPLPS objects of animation.md's examples (ANIM-21),
/// as the commands of a group: the message without its `connections`,
/// `page` and `sub` (RULE-35).
fn animation_command(kind: &str) -> Value {
    let mut command: Value = ANIMATION_EXAMPLES.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).find(|line| line["type"] == kind).unwrap();
    for field in ["connections", "page", "sub"] {
        command.as_object_mut().unwrap().remove(field);
    }
    command
}

/// RULE-114, RULE-119 (REQ-6532, DEC-6654 item 2 withdrawn): a group's
/// `sprites` and `naplps_objects` are the bodies the same records sent
/// directly carry, written by the engine's writer and read back by its
/// reader, with the table's page and sub-code.
#[test]
fn a_groups_sprites_and_objects_are_the_records_sent_directly() {
    let sprites = animation_command("sprites");
    let objects = animation_command("naplps_objects");
    let table = compiled(&one_rule(&json!([sprites, objects]))).unwrap_or_else(|refusal| panic!("{refusal}"));
    let image = table.image();
    let ProgramMessage::Sprites(direct_sprites) =
        accepted(&json!({"type":"sprites","connections":"all","page":"101","sub":"0001","sprites":sprites["sprites"]}))
    else {
        panic!("not sprites")
    };
    let ProgramMessage::NaplpsObjects(direct_objects) =
        accepted(&json!({"type":"naplps_objects","connections":"all","page":"101","sub":"0001","objects":objects["objects"]}))
    else {
        panic!("not naplps_objects")
    };
    let sprites_body = crate::animation::sprites_body(0x101, 1, &direct_sprites.sprites).unwrap();
    let objects_body = crate::animation::naplps_objects_body(0x101, 1, 1, &direct_objects.objects).unwrap();
    for body in [&sprites_body, &objects_body] {
        assert!(image.windows(body.len()).any(|window| window == body.as_slice()), "the group holds the body");
    }
    let read = neoteletext_web::records::sprites::read(&sprites_body).unwrap();
    assert_eq!((read.page.mpp(), read.subcode.value(), read.count), (0x101, 1, 1));
    let read = neoteletext_web::records::naplps_objects::read(&objects_body).unwrap();
    assert_eq!((read.page.mpp(), read.subcode.value(), read.generation, read.count), (0x101, 1, 1, 2));
}

/// The table of three row sets written out of the order of their names,
/// one of them keys, each used by a rule.
const ROW_SETS: &str = r#"{"type":"rules","connections":"all","page":"101","sub":"0001","table":{"rows":{"zeta":[60,62,64],"alpha":[1,2],"mid":[{"physical":"a"},{"physical":"s"}]},"groups":[{"name":"g","commands":[{"type":"report"}]}],"rules":[{"name":"z","rows":"zeta","on":{"event":"press","target":{"object":{"plus":10,"row":"p"}}},"run":"g"},{"name":"a","rows":"alpha","on":{"event":"press","target":{"object":{"plus":20,"row":"p"}}},"do":[{"type":"report"}]},{"name":"m","rows":"mid","on":{"event":"key","keys":[{"char":"q"},{"char":"w"}],"by_row":true},"run":"g"}]}}"#;
/// Its image as the engine's own reader of the program protocol's JSON,
/// `neottx rules-run`'s (`crates/neoteletext-cli/src/rules_json.rs` at the
/// pinned engine commit 33d90896), writes it: the rows of `alpha`, then of
/// `mid`, then of `zeta` (TODO-6649 step 4).
const ROW_SETS_BY_THE_ENGINE: &str = "020000070302000000240000000000010000000200000061000000730000003c0000003e0000004001000403010382000aff00010100020103820014ff00010002020101020000020100000071010000007700011f00000d01010001000000000000000000011f00000d01010001000000000000000000";

/// RULE-35, RULE-105 (REQ-6532; TODO-6649 step 4): the server lays row
/// sets out one after another in the order of their names, as the engine's
/// JSON reader does: the same table gives the same image in both.
#[test]
fn row_sets_are_laid_out_in_name_order_as_the_engine_does() {
    let table = compiled(&serde_json::from_str(ROW_SETS).unwrap()).unwrap_or_else(|refusal| panic!("{refusal}"));
    assert_eq!(hex(table.image()), ROW_SETS_BY_THE_ENGINE);
}
