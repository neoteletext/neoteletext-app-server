//! `reply` (REQ-1471, REQ-1477).

use serde_json::{Value, json};

use super::{accepted, bounds, magazines, refused, subpage, text_field, with, without};
use crate::protocol::Rule;

fn reply() -> Value {
    json!({"type": "reply", "connection": 1, "request": 1, "magazines": magazines(), "selected": subpage()})
}

#[test]
fn a_reply_is_read_whole() {
    accepted(&reply());
    accepted(&with(&reply(), "/selected", Value::Null));
    accepted(&with(&with(&reply(), "/magazines", json!([])), "/selected", Value::Null));
}

/// REQ-1475: each fault refused with the path of the field.
#[test]
fn a_reply_names_the_field_at_fault() {
    refused(&with(&reply(), "/extra", json!(1)), "extra", Rule::UnknownField);
    refused(&with(&reply(), "/magazines/0/pages/1/extra", json!(1)), "magazines[0].pages[1].extra", Rule::UnknownField);
    refused(&without(&reply(), "/connection"), "connection", Rule::MissingField);
    refused(&without(&reply(), "/magazines/1/pages/0/subpages"), "magazines[1].pages[0].subpages", Rule::MissingField);
    refused(&with(&reply(), "/connection", json!("1")), "connection", Rule::WrongType);
    refused(&with(&reply(), "/magazines/0/pages/1/page", json!("1a0")), "magazines[0].pages[1].page", Rule::WrongType);
    refused(&with(&reply(), "/type", json!("replies")), "type", Rule::NotListed);
    refused(&without(&reply(), "/type"), "", Rule::MissingField);
}

/// REQ-1477: the magazines' bounds, orders and reserved pages.
#[test]
fn a_replys_magazines_keep_their_bounds() {
    bounds(&reply(), "/connection", "connection", crate::protocol::SAFE_INTEGER_LIMIT, Rule::OutOfRange);
    refused(&with(&reply(), "/connection", json!(0)), "connection", Rule::OutOfRange);
    refused(&with(&reply(), "/magazines/0/magazine", json!(0)), "magazines[0].magazine", Rule::OutOfRange);
    refused(&with(&reply(), "/magazines/1/magazine", json!(9)), "magazines[1].magazine", Rule::OutOfRange);
    refused(&with(&reply(), "/magazines/1/magazine", json!(1)), "magazines[1].magazine", Rule::Duplicate);
    refused(&with(&reply(), "/magazines/0/pages/1/page", json!("1FF")), "magazines[0].pages[1].page", Rule::Reserved);
    refused(&with(&reply(), "/magazines/0/pages/1/page", json!("100")), "magazines[0].pages[1].page", Rule::Duplicate);
    refused(&with(&reply(), "/magazines/0/pages/1/page", json!("0FF")), "magazines[0].pages[1].page", Rule::WrongType);
    refused(&with(&reply(), "/magazines/0/pages/1/page", json!("200")), "magazines[0].pages[1].page", Rule::WrongMagazine);
    refused(&with(&reply(), "/magazines/0/pages", json!([])), "magazines[0].pages", Rule::TooShort);
    bounds(&reply(), "/magazines/0/pages/1/subpages", "magazines[0].pages[1].subpages", 8_190, Rule::OutOfRange);
    refused(&with(&reply(), "/magazines/0/pages/1/subpages", json!(0)), "magazines[0].pages[1].subpages", Rule::OutOfRange);
    refused(&with(&reply(), "/selected/page", json!("300")), "selected.page", Rule::NotInIndex);
}

/// The magazines' 253 pages, 251 in magazine 1, at the bound and past it.
#[test]
fn a_magazine_holds_the_standards_pages() {
    let pages = |magazine: u16, count: usize| -> Value {
        let numbers = (0..=0xFF_u16).map(|page| magazine << 8 | page).filter(|&number| !crate::protocol::PageNumber(number).is_reserved());
        Value::Array(numbers.take(count).map(|number| json!({"page": alloc::format!("{number:03X}"), "title": "", "section": null, "subpages": 1})).collect())
    };
    let one = |magazine: u16, count: usize| json!({"type": "reply", "connection": 1, "request": 1, "magazines": [{"magazine": magazine, "name": "", "pages": pages(magazine, count)}], "selected": null});
    accepted(&one(2, 253));
    accepted(&one(1, 251));
}

/// REQ-1475 item 6, for every text field of the index.
#[test]
fn a_replys_text_keeps_its_rules() {
    text_field(&reply(), "/magazines/0/name", "magazines[0].name", crate::protocol::TITLE_CHARACTERS);
    text_field(&reply(), "/magazines/0/pages/0/title", "magazines[0].pages[0].title", crate::protocol::TITLE_CHARACTERS);
    text_field(&reply(), "/magazines/0/pages/1/section", "magazines[0].pages[1].section", crate::protocol::TITLE_CHARACTERS);
}
