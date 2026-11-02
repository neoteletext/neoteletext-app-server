//! `index` (REQ-1477).

use alloc::vec::Vec;

use serde_json::{Value, json};

use super::{accepted, magazines, refused, text_field, with, without};
use crate::protocol::Rule;

fn index() -> Value {
    json!({"type": "index", "connections": "all", "magazines": magazines()})
}

/// REQ-1476: `index`.
#[test]
fn an_index_is_read_whole() {
    accepted(&index());
    accepted(&with(&index(), "/magazines", json!([])));
}

#[test]
fn an_index_names_the_field_at_fault() {
    refused(&with(&index(), "/magazines/1/extra", json!(1)), "magazines[1].extra", Rule::UnknownField);
    refused(&without(&index(), "/connections"), "connections", Rule::MissingField);
    refused(&with(&index(), "/magazines/0/pages/0/subpages", json!(-1)), "magazines[0].pages[0].subpages", Rule::WrongType);
    let nine: Vec<Value> = (1..=9_u8)
        .map(|magazine| json!({"magazine": magazine, "name": "", "pages": [{"page": alloc::format!("{}00", magazine.min(8)), "title": "", "section": null, "subpages": 1}]}))
        .collect();
    refused(&with(&index(), "/magazines", Value::Array(nine)), "magazines", Rule::TooLong);
    refused(&with(&index(), "/magazines/1/magazine", json!(1)), "magazines[1].magazine", Rule::Duplicate);
    text_field(&index(), "/magazines/1/name", "magazines[1].name", crate::protocol::TITLE_CHARACTERS);
}
