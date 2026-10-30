//! `cookie` (`SERVER.md` section 8.2).

use serde_json::{Value, json};

use super::{accepted, bounds, refused, text_field, with, without};
use crate::protocol::Rule;

fn cookie() -> Value {
    json!({"type": "cookie", "connection": 2, "name": "pref", "value": "dark", "path": "/demo/", "max_age": 60,
           "same_site": "Lax", "secure": true, "http_only": false})
}

#[test]
fn a_cookie_is_read_whole() {
    accepted(&cookie());
    accepted(&with(&with(&with(&cookie(), "/path", Value::Null), "/max_age", Value::Null), "/same_site", Value::Null));
}

#[test]
fn a_cookie_names_the_field_at_fault() {
    refused(&with(&cookie(), "/expires", json!(0)), "expires", Rule::UnknownField);
    refused(&without(&cookie(), "/secure"), "secure", Rule::MissingField);
    refused(&with(&cookie(), "/same_site", json!("lax")), "same_site", Rule::NotListed);
    refused(&with(&cookie(), "/name", json!("_session")), "name", Rule::Reserved);
    bounds(&cookie(), "/max_age", "max_age", u64::from(u32::MAX), Rule::WrongType);
    bounds(&cookie(), "/connection", "connection", crate::protocol::SAFE_INTEGER_LIMIT, Rule::OutOfRange);
    text_field(&cookie(), "/name", "name", crate::protocol::COOKIE_BYTES);
    text_field(&cookie(), "/value", "value", crate::protocol::COOKIE_BYTES);
    text_field(&cookie(), "/path", "path", crate::protocol::COOKIE_BYTES);
    // Its bound is in bytes: 2,049 characters of two bytes each are too many.
    refused(&with(&cookie(), "/value", json!("\u{e9}".repeat(2_049))), "value", Rule::TooLong);
}
