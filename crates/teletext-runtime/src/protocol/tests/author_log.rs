//! `author_log` (the engine's REQ-INP-026).

use serde_json::{Value, json};

use super::{accepted, refused, with, without};
use crate::protocol::Rule;

fn line() -> Value {
    json!({"type": "author_log", "connections": "all", "level": "W", "text": "the feed is slow", "page": "100"})
}

#[test]
fn an_authors_log_line_is_read_whole() {
    accepted(&line());
    accepted(&with(&line(), "/page", Value::Null));
}

/// Its text is 1 to 200 bytes of UTF-8, clean and in normalisation form C.
#[test]
fn an_authors_log_line_names_the_field_at_fault() {
    refused(&with(&line(), "/extra", json!(1)), "extra", Rule::UnknownField);
    refused(&without(&line(), "/level"), "level", Rule::MissingField);
    refused(&with(&line(), "/level", json!("D")), "level", Rule::NotListed);
    for bad in ["a\u{1}b", "a\u{7f}b", "a\u{85}b", "a\u{202e}b"] {
        refused(&with(&line(), "/text", json!(bad)), "text", Rule::ForbiddenCharacter);
    }
    refused(&with(&line(), "/text", json!("")), "text", Rule::TooShort);
    accepted(&with(&line(), "/text", json!("x".repeat(200))));
    refused(&with(&line(), "/text", json!("x".repeat(201))), "text", Rule::TooLong);
    refused(&with(&line(), "/text", json!("\u{e9}".repeat(101))), "text", Rule::TooLong);
    refused(&with(&line(), "/text", json!("cafe\u{301}")), "text", Rule::NotNfc);
}
