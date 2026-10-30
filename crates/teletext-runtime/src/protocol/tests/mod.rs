//! The protocol's reader against REQ-1475's verification, by message type:
//! each valid form accepted, and each kind of fault refused whole, with
//! the path of the fault and its rule. The fixtures are JSON values, each
//! test changes one field of a valid message, so a refusal can only be the
//! one the test means.

extern crate std;

use alloc::string::String;
use alloc::vec::Vec;

use serde_json::{Value, json};

use super::{ProgramMessage, Refusal, Rule, read};

mod applies_nothing;
mod author_log;
mod cookie;
mod goto;
mod index;
mod log;
mod packets;
mod page;
mod reply;
mod schema;
mod sound;
mod subpages;

/// The line a program would send for `value`.
fn line(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

/// `value` is read whole.
fn accepted(value: &Value) -> ProgramMessage {
    read(&line(value)).unwrap_or_else(|refusal| panic!("refused {value}: {refusal}"))
}

/// `value` is refused, at `path`, for `rule`.
#[track_caller]
fn refused(value: &Value, path: &str, rule: Rule) -> Refusal {
    let refusal = match read(&line(value)) {
        Ok(message) => panic!("accepted {value}: {message:?}"),
        Err(refusal) => refusal,
    };
    assert_eq!((refusal.path.as_str(), refusal.rule), (path, rule), "{refusal}");
    refusal
}

/// `value` with the field at the JSON pointer `pointer` set to `new`,
/// added when absent.
fn with(value: &Value, pointer: &str, new: Value) -> Value {
    let mut changed = value.clone();
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    let holder = if parent.is_empty() { &mut changed } else { changed.pointer_mut(parent).unwrap() };
    match holder {
        Value::Object(object) => {
            object.insert(key.into(), new);
        }
        Value::Array(items) => items[key.parse::<usize>().unwrap()] = new,
        other => panic!("{pointer} is in {other}"),
    }
    changed
}

/// `value` without the field at `pointer`.
fn without(value: &Value, pointer: &str) -> Value {
    let mut changed = value.clone();
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    let holder = if parent.is_empty() { &mut changed } else { changed.pointer_mut(parent).unwrap() };
    holder.as_object_mut().unwrap().remove(key).unwrap();
    changed
}

/// A number field at its bound accepted, and one past it refused: by the
/// type's own width (`wrong_type`) or by the protocol's bound
/// (`out_of_range`).
#[track_caller]
fn bounds(value: &Value, pointer: &str, path: &str, last: u64, rule: Rule) {
    accepted(&with(value, pointer, json!(last)));
    refused(&with(value, pointer, json!(last.saturating_add(1))), path, rule);
}

/// Exact base64 of `bytes` (RFC 4648 section 4).
fn base64(bytes: &[u8]) -> String {
    let mut out = alloc::vec![0_u8; neoteletext_web::base64::encoded_len(bytes.len())];
    let written = neoteletext_web::base64::encode(bytes, &mut out).unwrap();
    out.truncate(written);
    String::from_utf8(out).unwrap()
}

/// A text field: each forbidden kind of character (C0, DEL, C1, a
/// bidirectional formatting character) refused, one character too many
/// refused, text not in normalisation form C refused, and its longest
/// accepted. `most` is in characters, or bytes where `bytes` is set.
#[track_caller]
fn text_field(value: &Value, pointer: &str, path: &str, most: usize) {
    for bad in ["a\u{1}b", "a\u{7f}b", "a\u{85}b", "a\u{202e}b", "a\u{2066}b"] {
        refused(&with(value, pointer, json!(bad)), path, Rule::ForbiddenCharacter);
    }
    accepted(&with(value, pointer, json!("x".repeat(most))));
    refused(&with(value, pointer, json!("x".repeat(most.saturating_add(1)))), path, Rule::TooLong);
    // "é" as e and a combining acute accent: NFD, not NFC.
    refused(&with(value, pointer, json!("cafe\u{301}")), path, Rule::NotNfc);
    accepted(&with(value, pointer, json!("caf\u{e9}")));
}

/// Base64 that is not exact, or of the wrong length, refused at `path`.
#[track_caller]
fn base64_field(value: &Value, pointer: &str, path: &str, good: &[u8], wrong_length: &[u8]) {
    accepted(&with(value, pointer, json!(base64(good))));
    let exact = base64(good);
    if exact.ends_with('=') {
        refused(&with(value, pointer, json!(exact.trim_end_matches('='))), path, Rule::NotBase64);
    }
    refused(&with(value, pointer, json!(alloc::format!(" {exact}"))), path, Rule::NotBase64);
    refused(&with(value, pointer, json!(alloc::format!("{exact}\n"))), path, Rule::NotBase64);
    refused(&with(value, pointer, json!("!!!!")), path, Rule::NotBase64);
    refused(&with(value, pointer, json!(base64(wrong_length))), path, Rule::WrongLength);
}

/// A row of 40 spaces, as exact base64.
fn spaces() -> String {
    base64(&[b' '; 40])
}

/// Thirteen triplets of zeros.
fn triplets() -> Value {
    Value::Array((0..13).map(|_| json!({"address": 0, "mode": 0, "data": 0})).collect())
}

/// A sub-page with every kind of field filled.
fn subpage() -> Value {
    let mut rows: Vec<Value> = alloc::vec![Value::Null; 25];
    rows[0] = json!(spaces());
    rows[1] = json!(spaces());
    json!({
        "page": "100",
        "sub": "0000",
        "rows": rows,
        "links": [null, null, null, null, null, {"page": "100", "sub": "3F7F"}],
        "display_row_24": true,
        "link_extension": {
            "keys": [{"key": "red", "target": "200", "label": "News"}, {"key": "index", "target": "100", "label": null}],
            "numbers": [{"number": "300", "target": "bbc:100"}],
            "texts": [{"row": 3, "column": 30, "length": 10, "target": "https://example.org/a"}]
        },
        "control": {
            "erase": false, "newsflash": false, "subtitle": false, "suppress_header": false,
            "update": false, "interrupted_sequence": false, "inhibit_display": false, "magazine_serial": false
        },
        "character_set": {"region": null, "national_option": 0},
        "enhancements": [{"designation": 0, "triplets": triplets()}],
        "packets": [
            {"packet": "x28", "designation": 0, "triplets": triplets()},
            {"packet": "x27_nibbles", "designation": 1, "nibbles": alloc::vec![0; 39]}
        ],
        "cursor": {"row": 5, "column": 3, "block": false}
    })
}

/// Two magazines of pages.
fn magazines() -> Value {
    json!([
        {"magazine": 1, "name": "News", "pages": [
            {"page": "100", "title": "Front", "section": null, "subpages": 1},
            {"page": "101", "title": "Headlines", "section": "News", "subpages": 3}
        ]},
        {"magazine": 2, "name": "Sport", "pages": [{"page": "200", "title": "Sport", "section": null, "subpages": 1}]}
    ])
}
