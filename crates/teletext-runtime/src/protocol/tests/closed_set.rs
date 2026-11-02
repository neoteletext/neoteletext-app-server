//! The closed set of messages (REQ-1476), one sub-page a message
//! (neoteletext-common's REQ-1501), and a list in the page's order
//! (neoteletext-common's REQ-1503).

use serde_json::json;

use super::{refused, subpage, with};
use crate::protocol::Rule;

/// REQ-1476: a message of any other type is refused, whatever the server
/// itself sends a program under that name: the set is closed in each direction.
#[test]
fn a_message_of_any_other_type_is_refused() {
    for name in ["selected", "input", "left", "start", "connected", "goto", "publish", "release", ""] {
        let path = refused(&json!({"type": name, "connection": 1}), "type", Rule::NotListed).path;
        assert_eq!(path, "type", "{name}");
    }
}

/// REQ-1476: there are no views: a `view` field is an unknown field, in every
/// message that once carried one.
#[test]
fn a_view_field_is_refused() {
    let reply = json!({"type": "reply", "connection": 1, "request": 1, "magazines": super::magazines(), "selected": null});
    let page = json!({"type": "page", "connections": [1], "page": subpage()});
    refused(&with(&reply, "/view", json!(0)), "view", Rule::UnknownField);
    refused(&with(&page, "/view", json!(0)), "view", Rule::UnknownField);
    refused(&with(&page, "/page/view", json!(0)), "page.view", Rule::UnknownField);
}

/// REQ-1476 and neoteletext-common's REQ-1501 verification 2: a `page`
/// carrying two sub-pages, and a `reply` whose `selected` does, are refused
/// whole with their paths.
#[test]
fn a_message_carries_one_sub_page() {
    let two = json!([subpage(), with(&subpage(), "/sub", json!("0001"))]);
    let page = json!({"type": "page", "connections": [1], "page": two});
    refused(&page, "page[0]", Rule::WrongType);
    let reply = json!({"type": "reply", "connection": 1, "request": 1, "magazines": super::magazines(), "selected": two});
    refused(&reply, "selected[0]", Rule::WrongType);
}

/// REQ-1476 and neoteletext-common's REQ-1503 item 3: a list out of ascending
/// order is refused, and 0000 stands beside other sub-codes, first.
#[test]
fn a_sub_page_list_is_in_ascending_order() {
    let list = |subs: [&str; 3]| {
        let subpages: alloc::vec::Vec<serde_json::Value> = subs.iter().map(|sub| json!({"sub": sub, "title": null})).collect();
        json!({"type": "subpages", "connections": [1], "page": "101", "version": 3, "subpages": subpages})
    };
    super::accepted(&list(["0000", "0001", "0002"]));
    refused(&list(["0001", "0000", "0002"]), "subpages[1].sub", Rule::NotAscending);
}
