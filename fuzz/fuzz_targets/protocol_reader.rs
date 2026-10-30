//! The protocol's reader (REQ-1475): any bytes as a line from a program.
//! No panic, nothing beyond the line's bound allocated (the full gate runs
//! it with an RSS limit), and every line either refused whole or accepted
//! whole: what the reader accepts passes every check again, and a channel
//! given it either refuses it or acts on it, never both.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::protocol::{self, check};
use teletext_runtime::{Channel, ChannelConfig, Effect};

fuzz_target!(|data: &[u8]| {
    let read = protocol::read(data);
    if let Ok(message) = &read {
        assert!(check::message(message).is_ok(), "accepted, then refused: {message:?}");
    }
    let config = ChannelConfig { name: "Fuzz".into(), push_address: "ws://127.0.0.1/fuzz/push".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut channel = Channel::new(config);
    channel.program_started(0);
    channel.opened(1, 1, 0);
    let _ = channel.take_effects();
    channel.from_program(data, 0);
    let effects = channel.take_effects();
    let refused = effects.iter().any(|effect| matches!(effect, Effect::ToProgram { line } if line.starts_with(br#"{"type":"refused""#)));
    if read.is_err() {
        assert!(refused, "the reader refused it, and the channel did not: {effects:?}");
        assert!(!effects.iter().any(|effect| matches!(effect, Effect::Send { .. })), "refused, and still sent: {effects:?}");
    }
});
