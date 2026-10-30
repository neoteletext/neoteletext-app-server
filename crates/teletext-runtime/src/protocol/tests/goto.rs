//! `goto`.

use serde_json::{Value, json};

use super::{accepted, bounds, refused, with, without};
use crate::protocol::Rule;

fn goto() -> Value {
    json!({"type": "goto", "connection": 4, "page": "1A0", "sub": "3F7F"})
}

#[test]
fn a_goto_is_read_whole() {
    accepted(&goto());
    accepted(&with(&goto(), "/sub", json!("0001")));
}

#[test]
fn a_goto_names_the_field_at_fault() {
    refused(&with(&goto(), "/view", json!(0)), "view", Rule::UnknownField);
    refused(&without(&goto(), "/sub"), "sub", Rule::MissingField);
    refused(&with(&goto(), "/page", json!("1a0")), "page", Rule::WrongType);
    refused(&with(&goto(), "/page", json!("900")), "page", Rule::WrongType);
    refused(&with(&goto(), "/sub", json!("4000")), "sub", Rule::WrongType);
    bounds(&goto(), "/connection", "connection", crate::protocol::SAFE_INTEGER_LIMIT, Rule::OutOfRange);
}
