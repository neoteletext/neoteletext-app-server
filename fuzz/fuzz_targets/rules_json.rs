//! page-rules.md RULE-83, RULE-39, RULE-59 (REQ-6532): any bytes as the
//! table of a `rules` message, or, with the first byte odd, as a whole line
//! of a command message (RULE-114). No panic; the message is refused whole
//! and nothing of it sent, or it is accepted and every record the server
//! then sends is one the receivers' own reader accepts, every table image
//! one their decoder accepts.
#![no_main]

mod common_rules;

use common_rules::{CONNECTION, channel, records, refused};
use libfuzzer_sys::fuzz_target;
use teletext_runtime::Effect;
use teletext_runtime::protocol::{self, check};

fuzz_target!(|data: &[u8]| {
    let Some((&choice, data)) = data.split_first() else { return };
    let line = if choice % 2 == 1 {
        data.to_vec()
    } else {
        let mut line = br#"{"type":"rules","connections":[1],"page":"101","sub":"0001","table":"#.to_vec();
        line.extend_from_slice(data);
        line.push(b'}');
        line
    };
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
    for (page, sub, image) in records(&effects) {
        assert!(image.len() <= neoteletext_rules::limits::MAX_TABLE_BYTES);
        assert!(
            neoteletext_rules::check(&image, page, sub, neoteletext_web::records::body_check).is_ok(),
            "the receivers' decoder refuses a table the server sent"
        );
    }
    let _ = CONNECTION;
});
