//! `subpages`, a page's sub-page list (REQ-1503).

use alloc::vec::Vec;

use serde_json::{Value, json};

use super::{accepted, bounds, refused, text_field, with, without};
use crate::protocol::Rule;

fn list() -> Value {
    json!({"type": "subpages", "connections": [1], "page": "101", "version": 3,
           "subpages": [{"sub": "0000", "title": "Front"}, {"sub": "0001", "title": null}, {"sub": "0002", "title": "Third"}]})
}

/// REQ-1476 and REQ-1503: 1 and 8,190 sub-pages, 0000 beside the rest.
#[test]
fn a_sub_page_list_is_read_whole() {
    accepted(&list());
    let most: Vec<Value> = (1..=neoteletext_core::limits::LAST_SUBPAGE_CODE)
        .filter(|&code| neoteletext_core::limits::is_sub_code(code))
        .map(|code| json!({"sub": alloc::format!("{code:04X}"), "title": null}))
        .collect();
    assert_eq!(most.len(), 8_190);
    accepted(&with(&list(), "/subpages", Value::Array(most)));
}

/// REQ-1476: a repeated sub-code, 3F7F and an over-long title.
#[test]
fn a_sub_page_list_names_the_field_at_fault() {
    refused(&with(&list(), "/extra", json!(1)), "extra", Rule::UnknownField);
    refused(&with(&list(), "/subpages/1/extra", json!(1)), "subpages[1].extra", Rule::UnknownField);
    refused(&without(&list(), "/version"), "version", Rule::MissingField);
    // An optional field absent is its stated default, none.
    accepted(&without(&list(), "/subpages/2/title"));
    refused(&without(&list(), "/subpages/2/sub"), "subpages[2].sub", Rule::MissingField);
    bounds(&list(), "/version", "version", u64::from(u16::MAX), Rule::WrongType);
    refused(&with(&list(), "/subpages/1/sub", json!("0000")), "subpages[1].sub", Rule::Duplicate);
    refused(&with(&list(), "/subpages/1/sub", json!("3F7F")), "subpages[1].sub", Rule::OutOfRange);
    refused(&with(&list(), "/subpages", json!([])), "subpages", Rule::OutOfRange);
    refused(&with(&list(), "/page", json!("1FE")), "page", Rule::Reserved);
    text_field(&list(), "/subpages/0/title", "subpages[0].title", crate::protocol::SUBPAGE_TITLE_CHARACTERS);
}
