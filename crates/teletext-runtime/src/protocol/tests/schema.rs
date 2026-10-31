extern crate std;

use alloc::format;

use super::super::{PROTOCOL, schema};

/// Where the published schema is, from this crate.
const PUBLISHED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../protocol/schema-3.json");
/// Set to write the schema afresh rather than compare it.
const BLESS: &str = "TELETEXT_BLESS_SCHEMA";

/// REQ-1479: the published schema is exactly what the types generate.
/// `TELETEXT_BLESS_SCHEMA=1 cargo test -p teletext-runtime schema` writes it.
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
