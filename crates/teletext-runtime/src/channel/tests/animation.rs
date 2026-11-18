//! Animation on a channel, second form (animation.md ANIM-17, ANIM-21;
//! page-rules.md RULE-40, RULE-111, RULE-112; REQ-6533, REQ-6615).

use alloc::vec::Vec;

use neoteletext_audio::record::TYPE_SHEET;
use serde_json::{Value, json};

use super::{CONNECTION, Effect, message, record_types, select, selected, sent_to, told};
use crate::protocol::tests::base64;

/// A `sprite_sheet` message of a ReGIS sheet for page 100 and `sub`.
fn sheet(sub: &str, data: &[u8], restart: bool) -> Vec<u8> {
    serde_json::to_vec(&json!({"type":"sprite_sheet","connections":"all","page":"100","sub":sub,"kind":"regis","data":base64(data),"restart":restart})).unwrap()
}

/// The refusals among `effects`, by rule.
fn refusals(effects: &[Effect]) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == "refused").map(|line| line["rule"].clone()).collect()
}

/// A ReGIS sheet as it should be: a hidden master, then a commit.
const MASTER: &[u8] = b"D(B41)P[10,10]V[20,20]D(E)D(S41(V0),C)";

/// ANIM-17, DEC-6599 (REQ-6533): a sheet belongs to one page and
/// sub-code: it reaches the connection that has that page selected and no
/// other sub-code of the page number, and a sheet that leaves a drawing
/// visible is refused `invalid_sheet` with the ReGIS renderer's own
/// validator (TODO-6570).
#[test]
fn a_sheet_is_one_pages_and_draws_nothing() {
    let mut channel = selected();
    channel.from_program(&sheet("0001", MASTER, true), 1);
    let effects = channel.take_effects();
    assert!(!record_types(&sent_to(&effects, CONNECTION)).contains(&TYPE_SHEET), "another sub-code is another page");
    channel.from_program(&sheet("0000", MASTER, true), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    assert!(record_types(&sent_to(&effects, CONNECTION)).contains(&TYPE_SHEET));
    channel.from_program(&sheet("0000", b"D(B42)P[10,10]V[20,20]D(E)D(C)", true), 3);
    assert_eq!(refusals(&channel.take_effects()), [json!("invalid_sheet")]);
}

/// ANIM-21, RULE-111 (REQ-6533): a timeline ends for a connection that
/// leaves its page: a later step of it is never sent there.
#[test]
fn a_timeline_ends_when_its_page_is_left() {
    let mut channel = selected();
    let timeline = json!({"type":"timeline","connections":"all","page":"100","sub":"0000","id":1,
        "steps":[{"tick":60,"ops":[{"action":"show","target":{"object":1}}]}]});
    channel.from_program(&serde_json::to_vec(&timeline).unwrap(), 0);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x100, 1)]), 10);
    let _ = channel.take_effects();
    channel.deadline(2_000);
    let effects = channel.take_effects();
    assert!(!record_types(&sent_to(&effects, CONNECTION)).contains(&neoteletext_audio::record::TYPE_ANIMATE), "the step went to a page left");
}

/// ANIM-17, TODO-6569 (REQ-6533): an object page reaches the connections
/// that have its page selected, as packets, and no other.
#[test]
fn an_object_page_reaches_its_pages_viewers() {
    let mut channel = selected();
    let object = crate::protocol::tests::with(&crate::protocol::tests::subpage(), "/page", json!("1C0"));
    let to = |page: &str| serde_json::to_vec(&json!({"type":"object_page","connections":"all","page":page,"sub":"0000","object":object})).unwrap();
    channel.from_program(&to("200"), 1);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), Vec::<u8>::new());
    channel.from_program(&to("100"), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    assert_ne!(sent_to(&effects, CONNECTION), [] as [u8; 0]);
}
