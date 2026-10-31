//! `redirect` (DEC-1683, REQ-1688).

use alloc::string::String;

use neoteletext_web::records::redirect::MAX_ADDRESS;
use serde_json::{Value, json};

use super::{accepted, bounds, refused, with, without};
use crate::protocol::Rule;

fn page_redirect() -> Value {
    json!({"type": "redirect", "connection": 4, "page": "1A0", "sub": "3F7F"})
}

fn address_redirect() -> Value {
    json!({"type": "redirect", "connection": 4, "address": "https://news.example/teletext/#302"})
}

/// The longest address a REDIRECT record carries, `extra` bytes over.
fn long_address(extra: usize) -> Value {
    let mut address = String::from("https://news.example/");
    address.extend(core::iter::repeat_n('a', MAX_ADDRESS.saturating_add(extra).saturating_sub(address.len())));
    json!(address)
}

#[test]
fn a_page_redirect_is_read_whole() {
    accepted(&page_redirect());
    accepted(&with(&page_redirect(), "/sub", json!("0001")));
}

#[test]
fn a_page_redirect_names_the_field_at_fault() {
    refused(&with(&page_redirect(), "/view", json!(0)), "view", Rule::UnknownField);
    refused(&without(&page_redirect(), "/sub"), "sub", Rule::MissingField);
    refused(&with(&page_redirect(), "/page", json!("1a0")), "page", Rule::WrongType);
    refused(&with(&page_redirect(), "/page", json!("900")), "page", Rule::WrongType);
    refused(&with(&page_redirect(), "/sub", json!("4000")), "sub", Rule::WrongType);
    bounds(&page_redirect(), "/connection", "connection", crate::protocol::SAFE_INTEGER_LIMIT, Rule::OutOfRange);
}

/// REQ-1688 item 1: another channel's address, up to what the record holds.
#[test]
fn an_address_redirect_is_read_whole() {
    accepted(&address_redirect());
    accepted(&with(&address_redirect(), "/address", long_address(0)));
}

/// REQ-1688 item 1: exactly one form.
#[test]
fn a_redirect_names_a_page_or_an_address_never_both_or_neither() {
    refused(&with(&address_redirect(), "/page", json!("1A0")), "page", Rule::UnknownField);
    refused(&with(&address_redirect(), "/sub", json!("3F7F")), "sub", Rule::UnknownField);
    refused(&with(&page_redirect(), "/address", json!("https://news.example/")), "page", Rule::UnknownField);
    refused(&without(&page_redirect(), "/page"), "page", Rule::MissingField);
    refused(&without(&without(&page_redirect(), "/page"), "/sub"), "page", Rule::MissingField);
}

/// REQ-1688 item 2: an address too long for the record, or one the engine's
/// record refuses, never reaches a viewer.
#[test]
fn an_address_the_record_cannot_carry_is_refused() {
    refused(&with(&address_redirect(), "/address", long_address(1)), "address", Rule::TooLong);
    for address in ["", "file:///etc/passwd", "web:https://news.example/", "wss://news.example/", "https://news.example/a b"] {
        refused(&with(&address_redirect(), "/address", json!(address)), "address", Rule::ForbiddenCharacter);
    }
}
