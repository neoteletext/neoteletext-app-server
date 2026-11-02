//! `page` and the typed sub-page (REQ-1478, REQ-1501, REQ-1494).

use serde_json::{Value, json};

use super::{accepted, base64, base64_field, bounds, refused, spaces, subpage, text_field, triplets, with, without};
use crate::protocol::Rule;

fn page() -> Value {
    json!({"type": "page", "connections": "all", "page": subpage()})
}

/// REQ-1476: `page`.
#[test]
fn a_page_is_read_whole() {
    accepted(&page());
    accepted(&with(&page(), "/connections", json!([1, 2, 3])));
    accepted(&with(&page(), "/page/cursor", Value::Null));
    accepted(&with(&page(), "/page/link_extension", json!({"keys": [], "numbers": [], "texts": []})));
}

/// REQ-1475: unknown and missing fields, wrong types and closed lists,
/// each at its path, nested ones included.
#[test]
fn a_page_names_the_field_at_fault() {
    refused(&with(&page(), "/page/control/extra", json!(true)), "page.control.extra", Rule::UnknownField);
    refused(&with(&page(), "/page/packets/1/extra", json!(1)), "page.packets[1].extra", Rule::UnknownField);
    refused(&with(&page(), "/page/link_extension/texts/0/extra", json!(1)), "page.link_extension.texts[0].extra", Rule::UnknownField);
    refused(&without(&page(), "/page/control/erase"), "page.control.erase", Rule::MissingField);
    refused(&without(&page(), "/page/packets/0/designation"), "page.packets[0].designation", Rule::MissingField);
    refused(&with(&page(), "/page/rows/3", json!(7)), "page.rows[3]", Rule::WrongType);
    refused(&with(&page(), "/page/rows", json!([null])), "page.rows", Rule::WrongType);
    refused(&with(&page(), "/page/packets/0/packet", json!("x26")), "page.packets[0].packet", Rule::NotListed);
    refused(&with(&page(), "/page/link_extension/keys/0/key", json!("blue")), "page.link_extension.keys[0].key", Rule::NotListed);
    refused(&with(&page(), "/connections", json!("everyone")), "connections", Rule::WrongType);
    refused(&with(&page(), "/connections", json!([])), "connections", Rule::TooShort);
    refused(&with(&page(), "/page/sub", json!("0g00")), "page.sub", Rule::WrongType);
    refused(&with(&page(), "/page/sub", json!("3F7F")), "page.sub", Rule::OutOfRange);
    refused(&with(&page(), "/page/page", json!("8FE")), "page.page", Rule::Reserved);
}

/// REQ-1478: every number at its bound and one past it.
/// REQ-1346: a cursor on rows 1 to 23 alone.
#[test]
fn a_pages_numbers_keep_their_bounds() {
    let page = page();
    bounds(&page, "/page/enhancements/0/designation", "page.enhancements[0].designation", 15, Rule::OutOfRange);
    bounds(&page, "/page/enhancements/0/triplets/4/address", "page.enhancements[0].triplets[4].address", 63, Rule::OutOfRange);
    bounds(&page, "/page/enhancements/0/triplets/4/mode", "page.enhancements[0].triplets[4].mode", 31, Rule::OutOfRange);
    bounds(&page, "/page/enhancements/0/triplets/4/data", "page.enhancements[0].triplets[4].data", 127, Rule::OutOfRange);
    bounds(&page, "/page/packets/0/triplets/12/data", "page.packets[0].triplets[12].data", 127, Rule::OutOfRange);
    bounds(&page, "/page/packets/1/nibbles/38", "page.packets[1].nibbles[38]", 15, Rule::OutOfRange);
    bounds(&page, "/page/character_set/national_option", "page.character_set.national_option", 7, Rule::OutOfRange);
    bounds(&page, "/page/character_set/region", "page.character_set.region", 15, Rule::OutOfRange);
    bounds(&page, "/page/cursor/row", "page.cursor.row", 23, Rule::OutOfRange);
    refused(&with(&page, "/page/cursor/row", json!(0)), "page.cursor.row", Rule::OutOfRange);
    bounds(&page, "/page/cursor/column", "page.cursor.column", 39, Rule::OutOfRange);
    bounds(&page, "/page/link_extension/texts/0/row", "page.link_extension.texts[0].row", 23, Rule::OutOfRange);
    refused(&with(&page, "/page/link_extension/texts/0/row", json!(0)), "page.link_extension.texts[0].row", Rule::OutOfRange);
    accepted(&with(&with(&page, "/page/link_extension/texts/0/column", json!(0)), "/page/link_extension/texts/0/length", json!(40)));
    refused(&with(&page, "/page/link_extension/texts/0/length", json!(11)), "page.link_extension.texts[0].length", Rule::OutOfRange);
    refused(&with(&page, "/page/enhancements/0/triplets", triplets().as_array().unwrap()[..12].into()), "page.enhancements[0].triplets", Rule::WrongType);
    refused(&with(&page, "/page/packets/1/nibbles", json!(alloc::vec![0; 38])), "page.packets[1].nibbles", Rule::WrongLength);
    refused(&with(&page, "/page/packets/1/designation", json!(4)), "page.packets[1].designation", Rule::OutOfRange);
    let x27 = json!({"packet": "x27", "designation": 3, "triplets": triplets()});
    refused(&with(&page, "/page/packets/0", x27), "page.packets[0].designation", Rule::OutOfRange);
    bounds(&page, "/page/packets/0/designation", "page.packets[0].designation", 15, Rule::OutOfRange);
    let twice = json!([{"designation": 2, "triplets": triplets()}, {"designation": 2, "triplets": triplets()}]);
    refused(&with(&page, "/page/enhancements", twice), "page.enhancements[1].designation", Rule::Duplicate);
}

/// REQ-1478: a row is exact base64 of 40 seven-bit codes.
#[test]
fn a_pages_rows_are_exact() {
    base64_field(&page(), "/page/rows/2", "page.rows[2]", &[b'A'; 40], &[b'A'; 39]);
    refused(&with(&page(), "/page/rows/2", json!(base64(&[b'A'; 41]))), "page.rows[2]", Rule::WrongLength);
    refused(&with(&page(), "/page/rows/2", json!(base64(&[b'A'; 43]))), "page.rows[2]", Rule::TooLong);
    let mut high = [b' '; 40];
    high[7] = 0x80;
    refused(&with(&page(), "/page/rows/2", json!(base64(&high))), "page.rows[2]", Rule::OutOfRange);
    accepted(&with(&page(), "/page/rows/24", json!(spaces())));
}

/// REQ-1494: the Link Extension's targets and labels.
#[test]
fn a_pages_links_keep_their_rules() {
    let page = page();
    for good in ["100", "100:0001", "bbc:100", "bbc:100:0001", "https://example.org/a.t42", "web:https://example.org/"] {
        accepted(&with(&page, "/page/link_extension/keys/0/target", json!(good)));
        if !good.starts_with("web:") {
            accepted(&with(&page, "/page/link_extension/numbers/0/target", json!(good)));
        }
    }
    for bad in ["1 00", "100,200", "ftp://example.org/", "bb c:100", "", &"9".repeat(257)] {
        refused(&with(&page, "/page/link_extension/keys/0/target", json!(bad)), "page.link_extension.keys[0].target", Rule::WrongType);
    }
    refused(&with(&page, "/page/link_extension/numbers/0/target", json!("web:https://example.org/")), "page.link_extension.numbers[0].target", Rule::NotListed);
    refused(&with(&page, "/page/link_extension/keys/1/label", json!("Index")), "page.link_extension.keys[1].label", Rule::NotListed);
    accepted(&with(&page, "/page/link_extension/keys/0/label", json!("123456789")));
    refused(&with(&page, "/page/link_extension/keys/0/label", json!("1234567890")), "page.link_extension.keys[0].label", Rule::TooLong);
    let other = with(&page, "/page/link_extension/keys/0/target", json!("bbc:100"));
    accepted(&with(&other, "/page/link_extension/keys/0/label", json!("12345678")));
    refused(&with(&other, "/page/link_extension/keys/0/label", json!("123456789")), "page.link_extension.keys[0].label", Rule::TooLong);
    let twice = json!([{"key": "red", "target": "200", "label": null}, {"key": "red", "target": "300", "label": null}]);
    refused(&with(&page, "/page/link_extension/keys", twice), "page.link_extension.keys[1].key", Rule::Duplicate);
    text_field(&with(&page, "/page/link_extension/keys/0/label", json!("a")), "/page/link_extension/keys/0/label", "page.link_extension.keys[0].label", 9);
}
