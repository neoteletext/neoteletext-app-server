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

/// SRV-6: `Base64` carries `contentEncoding: "base64"`, and only it.
#[test]
fn bytes_are_marked_base64() {
    let schema = schema();
    assert_eq!(schema["$defs"]["Base64"]["contentEncoding"], "base64");
    let text = serde_json::to_string(&schema).unwrap();
    assert_eq!(text.matches("\"contentEncoding\":").count(), 1);
}
