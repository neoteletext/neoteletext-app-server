//! Viewer state's requests (DEC-6471, VS-352, VS-353): each read whole, and
//! `cookie` no longer a message at all.

use serde_json::{Value, json};

use super::{accepted, bounds, refused, with, without};
use crate::protocol::{Handle, ProgramMessage, Rule};

/// A handle as the server writes one.
const HANDLE: &str = "0123456789abcdef0123456789abcdef";

fn request_state() -> Value {
    json!({"type": "request_state", "connection": 2})
}

fn end_state() -> Value {
    json!({"type": "end_state", "viewer": HANDLE})
}

fn offer_remember() -> Value {
    json!({"type": "offer_remember", "connection": 2})
}

/// VS-352: each request is read whole.
#[test]
fn each_request_is_read_whole() {
    accepted(&request_state());
    accepted(&offer_remember());
    let ProgramMessage::EndState(end) = accepted(&end_state()) else { panic!("end_state is read as itself") };
    assert_eq!(Some(end.viewer), Handle::read(HANDLE));
}

/// VS-352, VS-353: each names the field at fault; a handle is exactly 32
/// lower-case hexadecimal digits, and a refusal of one does not repeat it,
/// since the refusal reaches the operator's log (VS-209).
#[test]
fn each_request_names_the_field_at_fault() {
    for message in [request_state(), offer_remember()] {
        bounds(&message, "/connection", "connection", crate::protocol::SAFE_INTEGER_LIMIT, Rule::OutOfRange);
        refused(&without(&message, "/connection"), "connection", Rule::MissingField);
        refused(&with(&message, "/viewer", json!(HANDLE)), "viewer", Rule::UnknownField);
    }
    for wrong in
        ["0123456789ABCDEF0123456789ABCDEF", "0123456789abcdef0123456789abcde", "0123456789abcdef0123456789abcdef0", "g123456789abcdef0123456789abcdef"]
    {
        let refusal = refused(&with(&end_state(), "/viewer", json!(wrong)), "viewer", Rule::WrongType);
        assert!(!refusal.detail.contains(wrong), "{}", refusal.detail);
    }
    refused(&with(&end_state(), "/connection", json!(2)), "connection", Rule::UnknownField);
}

/// VS-352: `cookie` is no message of protocol 13.
#[test]
fn a_cookie_message_is_refused_as_an_unknown_type() {
    let cookie = json!({"type": "cookie", "connection": 2, "name": "pref", "value": "dark", "path": null, "max_age": null, "same_site": null, "secure": false, "http_only": false});
    refused(&cookie, "type", Rule::NotListed);
}
