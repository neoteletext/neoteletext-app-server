//! The snapshot messages as the reader takes them and the server writes
//! them (REQ-6900; neoteletext-common `docs/tab-thumbnails.md` section 6).
//! Each test fails on the code before them, which has none of these
//! messages.

use serde_json::{Value, json};

use super::{accepted, base64, refused, subpage, with, without};
use crate::protocol::{PageNumber, ProgramMessage, Rule, SAFE_INTEGER_LIMIT, ServerMessage, Snapshot, SnapshotPicture, SubCode, line as server_line, schema};

/// A `snapshot_page` of page 100's sub-page 0000, valid.
fn answer() -> Value {
    json!({"type": "snapshot_page", "snapshot": 17, "page": subpage()})
}

/// REQ-6900 item 1: `snapshot` is written with its type and its four
/// fields and nothing else, and is about no connection.
#[test]
fn snapshot_is_written_with_exactly_its_four_fields() {
    let message = ServerMessage::Snapshot(Snapshot { snapshot: 17, page: PageNumber(0x100), sub: SubCode(0x3F7F), path: "100".into() });
    assert_eq!(message.connection(), None);
    let written: Value = serde_json::from_slice(&server_line(&message)).unwrap();
    assert_eq!(written, json!({"type": "snapshot", "snapshot": 17, "page": "100", "sub": "3F7F", "path": "100"}));
    let one = ServerMessage::Snapshot(Snapshot { snapshot: 18, page: PageNumber(0x302), sub: SubCode(2), path: "302/0002".into() });
    let written: Value = serde_json::from_slice(&server_line(&one)).unwrap();
    assert_eq!(written, json!({"type": "snapshot", "snapshot": 18, "page": "302", "sub": "0002", "path": "302/0002"}));
}

/// REQ-6900 items 2 and 3: both answers read strictly. An unknown field, a
/// connection, a missing page, a number out of range, a picture with both
/// `data` and `parts`, a ReGIS picture with objects and a picture past its
/// bound are refused; a kept picture names the answered sub-page's own
/// sub-code.
#[test]
fn snapshot_page_and_snapshot_declined_are_read_strictly() {
    let ProgramMessage::SnapshotPage(read) = accepted(&answer()) else { panic!() };
    assert_eq!((read.snapshot, read.page.page, read.page.sub, read.picture), (17, PageNumber(0x100), SubCode(0), None));
    assert!(accepted(&answer()).is_answer(), "an answer only a program reading its lines gives");
    let declined = json!({"type": "snapshot_declined", "snapshot": 17});
    let ProgramMessage::SnapshotDeclined(read) = accepted(&declined) else { panic!() };
    assert_eq!(read.snapshot, 17);
    for message in [answer(), declined.clone()] {
        refused(&with(&message, "/unknown", json!(1)), "unknown", Rule::UnknownField);
        refused(&with(&message, "/connection", json!(1)), "connection", Rule::UnknownField);
        refused(&with(&message, "/connections", json!([1])), "connections", Rule::UnknownField);
        refused(&with(&message, "/snapshot", json!(0)), "snapshot", Rule::OutOfRange);
        accepted(&with(&message, "/snapshot", json!(SAFE_INTEGER_LIMIT)));
        refused(&with(&message, "/snapshot", json!(SAFE_INTEGER_LIMIT + 1)), "snapshot", Rule::OutOfRange);
        refused(&without(&message, "/snapshot"), "snapshot", Rule::MissingField);
    }
    refused(&without(&answer(), "/page"), "page", Rule::MissingField);
    refused(&with(&answer(), "/page/sub", json!("3F7F")), "page.sub", Rule::OutOfRange);
    refused(&with(&answer(), "/rules", json!(null)), "rules", Rule::UnknownField);

    let kept = accepted(&with(&answer(), "/picture", json!({"kept": "0000"})));
    let ProgramMessage::SnapshotPage(kept) = kept else { panic!() };
    assert!(matches!(kept.picture, Some(SnapshotPicture::Kept(_))));
    refused(&with(&answer(), "/picture", json!({"kept": "0001"})), "picture.kept", Rule::WrongPage);

    let bytes = json!({"language": "naplps", "data": base64(b"\x0e"), "objects": []});
    accepted(&with(&answer(), "/picture", bytes.clone()));
    accepted(&with(&answer(), "/picture", json!({"language": "regis", "data": base64(b"P[1,1]")})));
    let both = with(&bytes, "/parts", json!([{"data": base64(b"\x0e")}]));
    refused(&with(&answer(), "/picture", both), "picture.parts", Rule::DuplicateField);
    refused(&with(&answer(), "/picture", without(&bytes, "/data")), "picture.data", Rule::MissingField);
    refused(&with(&answer(), "/picture", with(&bytes, "/language", json!("regis"))), "picture.objects", Rule::UnknownField);
    refused(&with(&answer(), "/picture", with(&bytes, "/connections", json!("kept"))), "picture.connections", Rule::UnknownField);
    let most = crate::protocol::MOST_DESCRIPTION_PICTURE_BYTES;
    accepted(&with(&answer(), "/picture", with(&bytes, "/data", json!(base64(&alloc::vec![0x20_u8; most])))));
    refused(&with(&answer(), "/picture", with(&bytes, "/data", json!(base64(&alloc::vec![0x20_u8; most + 1])))), "picture.data", Rule::TooLong);
}

/// REQ-6900 item 2: a picture's parts are written into its `data` as a
/// `naplps` message's are (PAR-21), within the same bound.
#[test]
fn a_snapshot_pictures_parts_are_written_into_its_data() {
    let parts = json!({"language": "naplps", "parts": [{"data": base64(b"\x0e")}, {"text": "Hello"}]});
    let ProgramMessage::SnapshotPage(read) = accepted(&with(&answer(), "/picture", parts)) else { panic!() };
    let Some(SnapshotPicture::Bytes(bytes)) = read.picture else { panic!() };
    assert!(bytes.parts.is_none());
    let data = crate::protocol::check::bytes_of(&bytes.data.unwrap().0);
    assert!(data.starts_with(b"\x0e") && data.len() > 1, "{data:?}");
    let half = crate::protocol::MOST_DESCRIPTION_PICTURE_BYTES / 2 + 1;
    let over = json!({"language": "regis", "parts": [{"data": base64(&alloc::vec![0x20_u8; half])}, {"data": base64(&alloc::vec![0x20_u8; half])}]});
    refused(&with(&answer(), "/picture", over), "picture.parts", Rule::TooLong);
}

/// REQ-6900: the schema lists `snapshot` among the server's messages with
/// exactly its fields, and both answers among a program's, closed.
#[test]
fn the_schema_carries_the_snapshot_messages() {
    let schema = schema();
    let server = schema["$defs"]["ServerMessage"]["oneOf"].as_array().unwrap();
    let snapshot = server.iter().find(|message| message["properties"]["type"]["const"] == "snapshot").unwrap();
    let mut fields: alloc::vec::Vec<&str> = snapshot["properties"].as_object().unwrap().keys().map(alloc::string::String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(fields, ["page", "path", "snapshot", "sub", "type"]);
    let program = schema["$defs"]["ProgramMessage"]["oneOf"].as_array().unwrap();
    for kind in ["snapshot_page", "snapshot_declined"] {
        let message = program.iter().find(|message| message["properties"]["type"]["const"] == kind).unwrap_or_else(|| panic!("{kind}"));
        assert_eq!(message["additionalProperties"], false, "{kind} is open");
    }
    let budgets = &schema["$defs"]["Budgets"]["properties"];
    assert!(budgets.get("snapshot_limit_ms").is_some());
}
