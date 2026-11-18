//! The picture messages of renderer parity as the reader takes them
//! (neoteletext-common `docs/renderer-parity.md` PAR-16, PAR-19 to
//! PAR-22, PAR-26; REQ-6766, REQ-6767, REQ-6768).

use alloc::vec::Vec;

use serde_json::{Value, json};

use super::{accepted, base64, refused, subpage, text_field, with, without};
use crate::protocol::{DescriptionPicture, PictureAudience, PicturePart, ProgramMessage, Rule};

/// Each picture message, valid, for connection 1.
fn pictures() -> [Value; 7] {
    [
        json!({"type": "regis", "connections": [1], "page": "100", "sub": "0001", "restart": true, "data": base64(b"P[1,1]")}),
        json!({"type": "regis_links", "connections": [1], "page": "100", "sub": "0001", "links": [{"drawing": 1, "target": "200"}]}),
        json!({"type": "regis_cursor", "connections": [1], "page": "100", "sub": "0001", "drawing": 1}),
        json!({"type": "naplps", "connections": [1], "page": "100", "sub": "0001", "restart": true, "data": base64(b"\x0e")}),
        json!({"type": "naplps_objects", "connections": [1], "page": "100", "sub": "0001", "objects": []}),
        json!({"type": "naplps_links", "connections": [1], "page": "100", "sub": "0001", "links": [{"object": 7, "target": "200"}]}),
        json!({"type": "naplps_cursor", "connections": [1], "page": "100", "sub": "0001", "object": 7}),
    ]
}

/// PAR-16: the seven picture messages take `"kept"` and every audience
/// they took before; every other message refuses it.
#[test]
fn the_seven_picture_messages_take_kept_and_no_other_does() {
    for message in pictures() {
        accepted(&message);
        accepted(&with(&message, "/connections", json!("all")));
        let kept = accepted(&with(&message, "/connections", json!("kept")));
        let audience = match &kept {
            ProgramMessage::Regis(message) => &message.connections,
            ProgramMessage::RegisLinks(message) => &message.connections,
            ProgramMessage::RegisCursor(message) => &message.connections,
            ProgramMessage::Naplps(message) => &message.connections,
            ProgramMessage::NaplpsObjects(message) => &message.connections,
            ProgramMessage::NaplpsLinks(message) => &message.connections,
            ProgramMessage::NaplpsCursor(message) => &message.connections,
            other => panic!("{other:?}"),
        };
        assert!(matches!(audience, PictureAudience::Kept(_)), "{message}");
        refused(&with(&message, "/connections", json!("keep")), "connections", Rule::WrongType);
        refused(&with(&message, "/sub", json!("3F7F")), "sub", Rule::OutOfRange);
    }
    let page = json!({"type": "page", "connections": "kept", "page": subpage()});
    refused(&page, "connections", Rule::WrongType);
    let frame = json!({"type": "frame", "connections": "kept", "page": "100", "sub": "0001", "mark": "begin"});
    refused(&frame, "connections", Rule::WrongType);
}

/// PAR-21: exactly one of `data` and `parts`.
#[test]
fn a_picture_gives_data_or_parts_and_not_both() {
    for kind in ["regis", "naplps"] {
        let message = json!({"type": kind, "connections": [1], "page": "100", "sub": "0001", "data": base64(b"x")});
        refused(&without(&message, "/data"), "data", Rule::MissingField);
        let both = with(&message, "/parts", json!([{"data": base64(b"x")}]));
        refused(&both, "parts", Rule::DuplicateField);
        let parts = with(&without(&message, "/data"), "/parts", json!([{"data": base64(b"x")}, {"text": "Hi", "language": null}]));
        let read = accepted(&parts);
        // The parts are written into `data`, which the server then sends
        // as though the program had given it.
        let (data, parts) = match &read {
            ProgramMessage::Regis(message) => (&message.data, &message.parts),
            ProgramMessage::Naplps(message) => (&message.data, &message.parts),
            other => panic!("{other:?}"),
        };
        assert!(parts.is_none(), "parts are written into data");
        let expected: &[u8] = if kind == "regis" { b"xT(A101)\"Hi\"T(A0)" } else { b"x\x1b\x28\x31\x0fHi\x1b\x28\x42" };
        assert_eq!(data.as_ref().map(|data| data.0.clone()), Some(base64(expected)));
    }
}

/// PAR-21: the bounds of `parts`, and each part's faults with its path.
#[test]
fn parts_are_bounded_and_each_fault_names_its_part() {
    let message = json!({"type": "regis", "connections": [1], "page": "100", "sub": "0001", "parts": [{"text": "a"}]});
    refused(&with(&message, "/parts", json!([])), "parts", Rule::InvalidPictureParts);
    let most = alloc::vec![json!({"text": "a"}); crate::protocol::MOST_PICTURE_PARTS];
    accepted(&with(&message, "/parts", Value::Array(most.clone())));
    let mut over = most;
    over.push(json!({"text": "a"}));
    refused(&with(&message, "/parts", Value::Array(over)), "parts", Rule::InvalidPictureParts);
    text_field(&message, "/parts/0/text", "parts[0].text", crate::protocol::MOST_PART_CHARACTERS);
    refused(&with(&message, "/parts/0/language", json!("xx")), "parts[0].language", Rule::UnknownLanguage);
    refused(&with(&message, "/parts/0/extra", json!(1)), "parts[0].extra", Rule::UnknownField);
    refused(&with(&message, "/parts/0", json!({"data": "!!!!"})), "parts[0].data", Rule::NotBase64);
    // A character no set of Latin English shows, then from outside.
    refused(&with(&message, "/parts/0/text", json!("\u{05d0}")), "parts[0].text", Rule::TextUnshown);
    let outside = accepted(&with(&with(&message, "/parts/0/text", json!("a\u{05d0}")), "/parts/0/outside", json!(true)));
    let ProgramMessage::Regis(outside) = outside else { panic!() };
    assert_eq!(outside.data.map(|data| data.0), Some(base64(b"T(A101)\"a?\"T(A0)")), "the stand-in");
    // Each kind of part reads as its own.
    let read: PicturePart = serde_json::from_value(json!({"text": "a"})).unwrap();
    assert!(matches!(read, PicturePart::Text(_)));
}

/// PAR-20: `naplps_links` and `naplps_cursor`.
#[test]
fn naplps_links_and_cursor_are_read_and_their_faults_refused() {
    let [.., links, cursor] = pictures();
    refused(&with(&links, "/links/0/object", json!(0)), "links[0].object", Rule::InvalidNaplpsLinks);
    let twice = with(&links, "/links", json!([{"object": 7, "target": "200"}, {"object": 7, "target": "300"}]));
    refused(&twice, "links[1].object", Rule::InvalidNaplpsLinks);
    refused(&with(&links, "/links/0/target", json!("a,b")), "links[0].target", Rule::InvalidNaplpsLinks);
    let most: Vec<Value> = (1..=crate::protocol::MOST_DRAWING_LINKS).map(|object| json!({"object": object, "target": "200"})).collect();
    accepted(&with(&links, "/links", Value::Array(most.clone())));
    let mut over = most;
    over.push(json!({"object": 999, "target": "200"}));
    refused(&with(&links, "/links", Value::Array(over)), "links", Rule::InvalidNaplpsLinks);
    accepted(&with(&cursor, "/object", json!(0)));
    refused(&with(&cursor, "/object", json!(65_536)), "object", Rule::WrongType);
}

/// PAR-19 and PAR-22: `forget_picture` and `measure_picture_text`.
#[test]
fn forget_picture_and_measure_picture_text_are_read() {
    let forget = json!({"type": "forget_picture", "page": "200", "sub": "0001"});
    accepted(&forget);
    refused(&with(&forget, "/sub", json!("3F7F")), "sub", Rule::OutOfRange);
    refused(&with(&forget, "/connections", json!("all")), "connections", Rule::UnknownField);
    let measure = json!({"type": "measure_picture_text", "request": 17, "text": "Hi", "language": null, "outside": false});
    accepted(&measure);
    accepted(&without(&without(&measure, "/language"), "/outside"));
    text_field(&measure, "/text", "text", crate::protocol::MOST_PART_CHARACTERS);
}

/// PAR-26: `description.picture` in its two forms.
#[test]
fn a_description_takes_a_kept_picture_or_bytes() {
    let described = json!({"type": "description", "page": "100", "public": true, "title": "T", "text": "t", "content": subpage()});
    let kept = accepted(&with(&described, "/picture", json!({"kept": "0001"})));
    let ProgramMessage::Description(kept) = kept else { panic!() };
    assert!(matches!(kept.picture, Some(DescriptionPicture::Kept(_))));
    refused(&with(&described, "/picture", json!({"kept": "3F7F"})), "picture.kept", Rule::OutOfRange);
    let bytes = json!({"language": "naplps", "data": base64(b"\x0e"), "objects": []});
    let read = accepted(&with(&described, "/picture", bytes.clone()));
    let ProgramMessage::Description(read) = read else { panic!() };
    assert!(matches!(read.picture, Some(DescriptionPicture::Bytes(_))));
    refused(&with(&described, "/picture", with(&bytes, "/language", json!("sixel"))), "picture.language", Rule::NotListed);
    refused(&with(&described, "/picture", with(&bytes, "/language", json!("regis"))), "picture.objects", Rule::UnknownField);
    let over = base64(&alloc::vec![0_u8; crate::protocol::MOST_DESCRIPTION_PICTURE_BYTES + 1]);
    refused(&with(&described, "/picture", with(&bytes, "/data", json!(over))), "picture.data", Rule::TooLong);
    let private = json!({"type": "description", "page": "100", "public": false, "picture": {"kept": "0001"}});
    refused(&private, "picture", Rule::UnknownField);
}
