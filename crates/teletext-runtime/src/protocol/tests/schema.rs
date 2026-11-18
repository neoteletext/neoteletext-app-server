extern crate std;

use alloc::format;

use super::super::{PROTOCOL, schema};

/// Where the published schema is, from this crate.
const PUBLISHED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../protocol/schema-13.json");
/// Set to write the schema afresh rather than compare it.
const BLESS: &str = "TELETEXT_BLESS_SCHEMA";

/// REQ-1479: the published schema is exactly what the types generate.
/// `TELETEXT_BLESS_SCHEMA=1 cargo test -p teletext-runtime schema` writes it.
/// REQ-1476: the messages and their schema, from one set of types.
#[test]
fn the_published_schema_is_the_one_the_types_generate() {
    assert!(PUBLISHED.ends_with(&format!("schema-{PROTOCOL}.json")), "the file is named for the protocol's version");
    let mut generated = serde_json::to_string_pretty(&schema()).unwrap();
    generated.push('\n');
    if std::env::var_os(BLESS).is_some() {
        std::fs::write(PUBLISHED, &generated).unwrap();
    }
    let published = std::fs::read_to_string(PUBLISHED).unwrap();
    assert!(published == generated, "protocol/schema-{PROTOCOL}.json is not what the types generate: run with {BLESS}=1");
}

/// SRV-2, SRV-3: every kind once among the `sound` message's variants,
/// and once in the closed list `SoundKind` that `refused` names.
#[test]
fn the_schema_lists_every_kind_once() {
    let text = serde_json::to_string(&schema()).unwrap();
    for kind in crate::protocol::SoundKind::ALL {
        let quoted = alloc::format!("\"const\":\"{}\"", kind.name());
        assert_eq!(text.matches(&quoted).count(), 2, "{}", kind.name());
    }
    let synthesizers = crate::protocol::SoundKind::ALL.iter().filter(|kind| kind.name().starts_with("synth")).count();
    assert_eq!(synthesizers, 42);
}

/// SRV-3, REQ-1475: every sound kind's branch refuses an unknown field, as
/// the reader does: it is closed (`additionalProperties: false`) and lists
/// the message's `type` among its own properties, so that a closed branch
/// still takes the message it is part of (DEC-6462: the kits follow this
/// schema).
#[test]
fn every_sound_branch_refuses_an_unknown_field() {
    let schema = schema();
    let messages = schema["$defs"]["ProgramMessage"]["oneOf"].as_array().unwrap();
    let sound = messages.iter().find(|message| message["properties"]["type"]["const"] == "sound").unwrap();
    let branches = sound["oneOf"].as_array().unwrap();
    let kinds: alloc::vec::Vec<&str> = branches.iter().map(|branch| branch["properties"]["kind"]["const"].as_str().unwrap()).collect();
    let expected: alloc::vec::Vec<&str> = crate::protocol::SoundKind::ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(kinds, expected, "one branch for each kind, in order");
    for branch in branches {
        let kind = branch["properties"]["kind"]["const"].as_str().unwrap();
        assert_eq!(branch["additionalProperties"], false, "{kind} is open");
        assert_eq!(branch["properties"]["type"], serde_json::json!({ "const": "sound", "type": "string" }), "{kind} lacks the message's type");
        let required = branch["required"].as_array().unwrap();
        assert_eq!(required[..2], [serde_json::json!("type"), serde_json::json!("kind")], "{kind} requires its tags first");
        assert!(branch["properties"].get("unknown").is_none(), "{kind} names the field the test calls unknown");
    }
}

/// SRV-6: `Base64` carries `contentEncoding: "base64"`, and only it.
#[test]
fn bytes_are_marked_base64() {
    let schema = schema();
    assert_eq!(schema["$defs"]["Base64"]["contentEncoding"], "base64");
    let text = serde_json::to_string(&schema).unwrap();
    assert_eq!(text.matches("\"contentEncoding\":").count(), 1);
}

/// The names of definition `name`: a message's `type` for the two message
/// unions, the value itself for a closed list.
fn names(schema: &serde_json::Value, name: &str) -> alloc::vec::Vec<alloc::string::String> {
    let definition = &schema["$defs"][name];
    if let Some(values) = definition["enum"].as_array() {
        return values.iter().map(|value| value.as_str().unwrap().into()).collect();
    }
    definition["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|variant| variant["properties"]["type"]["const"].as_str().or_else(|| variant["const"].as_str()).unwrap().into())
        .collect()
}

/// SRV-2: one protocol 13 holds viewer state's messages (VS-351 to VS-353)
/// and the synthesizers' (SRV-47, SRV-48, SRV-51), no name twice in any of
/// the program's and the server's messages, the kinds and the rules, and
/// none of the synthesizers' names among viewer state's.
#[test]
fn viewer_state_and_the_synthesizers_share_one_protocol() {
    let schema = schema();
    for list in ["ProgramMessage", "ServerMessage", "SoundKind", "Rule", "MessageType"] {
        let all = names(&schema, list);
        let once: alloc::collections::BTreeSet<&str> = all.iter().map(alloc::string::String::as_str).collect();
        assert_eq!(once.len(), all.len(), "{list} names one twice: {all:?}");
    }
    let viewer_state: [(&str, &[&str]); 3] = [
        ("ServerMessage", &["gone", "viewer_began", "viewer_left", "state_refused", "viewers_reset"]),
        ("ProgramMessage", &["request_state", "end_state", "offer_remember"]),
        ("Rule", &["has_viewer", "no_gesture", "unknown_viewer"]),
    ];
    let synthesizers: [(&str, &[&str]); 2] = [
        ("ServerMessage", &["sound_built"]),
        (
            "Rule",
            &[
                "not_taken",
                "never_carried",
                "malformed_midi",
                "running_status",
                "system_exclusive_too_long",
                "block_full",
                "no_room_to_restate",
                "beyond_length",
                "late_event",
                "not_before_until",
                "until_moved_back",
                "advance_too_far",
                "unknown_live_sound",
                "live_sound_changed",
                "no_opening_update",
                "invalid_patch",
                "invalid_bank",
                "unknown_bank",
                "wrong_family",
                "too_many_banks",
                "bank_dump_after_note",
                "bank_not_named",
                "midi_file_unsupported",
                "midi_file_malformed",
                "midi_file_too_large",
            ],
        ),
    ];
    for (list, expected) in viewer_state.iter().chain(&synthesizers) {
        let all = names(&schema, list);
        for name in *expected {
            assert!(all.iter().any(|held| held == name), "{list} lacks {name}");
        }
    }
    for (list, ours) in &synthesizers {
        for (theirs_list, theirs) in &viewer_state {
            if list == theirs_list {
                assert!(ours.iter().all(|name| !theirs.contains(name)), "{list}: a synthesizer name is viewer state's");
            }
        }
    }
    // SRV-48: `refused` names the sound it concerns; it is not a viewer
    // state field.
    let server = &schema["$defs"]["ServerMessage"]["oneOf"];
    let refused = server.as_array().unwrap().iter().find(|variant| variant["properties"]["type"]["const"] == "refused").unwrap();
    assert!(refused["properties"].get("sound").is_some(), "refused carries sound");
    // SRV-2: 2^53 - 1, the largest integer every JSON reader holds exactly.
    assert_eq!(super::super::SAFE_INTEGER_LIMIT, 9_007_199_254_740_991);
}
