//! The protocol's reader (REQ-1475): any bytes as a line from a program.
//! No panic, nothing beyond the line's bound allocated (the full gate runs
//! it with an RSS limit), and every line either refused whole or accepted
//! whole: what the reader accepts passes every check again, and a channel
//! given it either refuses it or acts on it, never both.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::protocol::{self, check};
use teletext_runtime::{Channel, ChannelConfig, Effect, HandleSecret, InitialPage, RandomSource, RandomUnavailable};

/// Random bytes that count, so that a run repeats.
struct Counting(u8);

impl RandomSource for Counting {
    fn fill(&mut self, bytes: &mut [u8]) -> Result<(), RandomUnavailable> {
        for byte in bytes {
            self.0 = self.0.wrapping_add(1);
            *byte = self.0;
        }
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    let read = protocol::read(data);
    if let Ok(message) = &read {
        assert!(check::message(message).is_ok(), "accepted, then refused: {message:?}");
    }
    let config = ChannelConfig {
        name: "Fuzz".into(),
        address: "http://127.0.0.1/fuzz/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
    };
    let mut channel = Channel::new(config, &HandleSecret::new([0; 32]), Box::new(Counting(0)));
    channel.program_started(0);
    channel.opened(1, None, 0);
    let _ = channel.take_effects();
    channel.from_program(data, 0);
    let effects = channel.take_effects();
    let refused = effects.iter().any(|effect| matches!(effect, Effect::ToProgram { line, .. } if line.starts_with(br#"{"type":"refused""#)));
    if read.is_err() {
        assert!(refused, "the reader refused it, and the channel did not: {effects:?}");
        assert!(!effects.iter().any(|effect| matches!(effect, Effect::Send { .. })), "refused, and still sent: {effects:?}");
    }
});
