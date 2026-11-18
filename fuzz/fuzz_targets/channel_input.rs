//! A channel fed any input streams from two connections and any lines from
//! its program, with time passing: no panic, whatever the bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::{Channel, ChannelConfig, HandleSecret, InitialPage, RandomSource, RandomUnavailable};

/// The time between messages, in milliseconds.
const STEP: u64 = 50;
/// Who sends each message, by its first byte: the two connections and the program.
const SENDERS: u8 = 3;
/// The sender byte that is the program's.
const PROGRAM: u8 = 2;
/// Each connection's first message: HELLO with version 8 and the CURSOR and
/// SELECT capabilities, then SELECT of page 100, sub-code 0000 (DOC-1753 and
/// DOC-1921), which also answers the opening challenge with no key (VS-202).
/// Stamped input can then match the selection, so
/// the fuzzer reaches the stamp check, what is held and its releases, and
/// not only the drop of input with nothing selected.
const OPENING: [u8; 18] = *b"\xFF\x09\x08\x80\x80\x88\x02\xFF\x40\x08100/0000";

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
    channel.opened(2, None, 0);
    channel.received(1, &OPENING, 0);
    channel.received(2, &OPENING, 0);
    let mut now = 0_u64;
    // Each message: a sender byte, a length byte, then that many bytes.
    let mut rest = data;
    while let [sender, length, tail @ ..] = rest {
        let take = usize::from(*length).min(tail.len());
        let (message, after) = tail.split_at(take);
        match sender % SENDERS {
            PROGRAM => channel.from_program(message, now),
            connection => channel.received(u64::from(connection) + 1, message, now),
        }
        now += STEP;
        channel.deadline(now);
        let _ = channel.take_effects();
        rest = after;
    }
    channel.closed(1, now);
    channel.closed(2, now);
});
