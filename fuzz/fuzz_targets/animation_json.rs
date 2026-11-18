//! animation.md ANIM-23, ANIM-21 (REQ-6533): any bytes as the rest of one
//! of the protocol's animation messages, chosen by the first byte. No
//! panic; the message is refused, or it encodes to records the engine's
//! reader accepts; and the server's clock moved on, with the timelines and
//! the frame it began, sends nothing the reader refuses either.
#![no_main]

mod common_rules;

use common_rules::{channel, records, refused};
use libfuzzer_sys::fuzz_target;
use teletext_runtime::Effect;
use teletext_runtime::protocol::{self, check};

/// The openings of the animation messages, each for the connection and
/// the page it shows; the fuzzer's bytes are the fields that follow.
const OPENINGS: [&str; 7] = [
    r#"{"type":"frame","connections":[1],"page":"101","sub":"0001","#,
    r#"{"type":"animate","connections":[1],"page":"101","sub":"0001","#,
    r#"{"type":"sprites","connections":[1],"page":"101","sub":"0001","#,
    r#"{"type":"naplps_objects","connections":[1],"page":"101","sub":"0001","#,
    r#"{"type":"sprite_sheet","connections":[1],"page":"101","#,
    r#"{"type":"timeline","connections":[1],"page":"101","sub":"0001","#,
    r#"{"type":"timeline_stop","connections":[1],"page":"101","#,
];
/// How far the clock is moved on: past a frame's two seconds.
const LATER_MS: u64 = 5_000;

fuzz_target!(|data: &[u8]| {
    let Some((choice, rest)) = data.split_first() else { return };
    let mut line = OPENINGS[usize::from(*choice) % OPENINGS.len()].as_bytes().to_vec();
    line.extend_from_slice(rest);
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
    let _ = records(&effects);
    channel.deadline(LATER_MS);
    let _ = records(&channel.take_effects());
    channel.program_ended();
    let _ = records(&channel.take_effects());
});
