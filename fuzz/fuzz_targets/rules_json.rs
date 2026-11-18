//! page-rules.md RULE-83, RULE-39, RULE-59 (REQ-6532): any bytes as the
//! table of a `rules` message. No panic; the message is refused whole and
//! nothing of it sent, or it is accepted and every RULES record the server
//! then sends is one the receivers' own decoder accepts, within one
//! record's body.
#![no_main]

mod common_rules;

use common_rules::{CONNECTION, channel, records, refused};
use libfuzzer_sys::fuzz_target;
use teletext_runtime::Effect;
use teletext_runtime::protocol::{self, check};

fuzz_target!(|data: &[u8]| {
    let mut line = br#"{"type":"rules","connections":[1],"page":"101","table":"#.to_vec();
    line.extend_from_slice(data);
    line.push(b'}');
    let read = protocol::read(&line);
    if let Ok(message) = &read {
        assert!(check::message(message).is_ok(), "accepted, then refused: {message:?}");
    }
    let mut channel = channel();
    channel.from_program(&line, 1);
    let effects = channel.take_effects();
    if read.is_err() {
        assert!(refused(&effects), "the reader refused it, and the channel did not");
        assert!(!effects.iter().any(|effect| matches!(effect, Effect::Send { .. })), "refused, and still sent");
        return;
    }
    assert!(!refused(&effects), "the reader accepted it, and the channel refused it");
    for body in records(&effects) {
        assert!(body.len() <= neoteletext_rules::limits::MAX_TABLE_BYTES);
        assert!(neoteletext_rules::check(&body).is_ok(), "the receivers' decoder refuses a table the server sent");
    }
    let _ = CONNECTION;
});
