//! `log`, a line for the server's operator.

use serde_json::{Value, json};

use super::{accepted, refused, text_field, with, without};
use crate::protocol::Rule;

fn line() -> Value {
    json!({"type": "log", "level": "info", "event": "fetched_posts", "text": "7 posts"})
}

#[test]
fn an_operators_log_line_is_read_whole() {
    accepted(&line());
}

#[test]
fn an_operators_log_line_names_the_field_at_fault() {
    refused(&with(&line(), "/extra", json!(1)), "extra", Rule::UnknownField);
    refused(&without(&line(), "/event"), "event", Rule::MissingField);
    refused(&with(&line(), "/level", json!("trace")), "level", Rule::NotListed);
    for bad in ["Fetched", "1fetched", "fetched posts", "", &"x".repeat(65)] {
        refused(&with(&line(), "/event", json!(bad)), "event", Rule::WrongType);
    }
    accepted(&with(&line(), "/event", json!("x".repeat(64))));
    refused(&with(&line(), "/text", json!("")), "text", Rule::TooShort);
    text_field(&line(), "/text", "text", crate::protocol::OPERATOR_LOG_CHARACTERS);
}
