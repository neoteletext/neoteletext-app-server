//! A channel whose application is a program (`Remote`), as every channel
//! is, fed any input streams from two sessions and any lines from the
//! program, with time passing: no panic, whatever the bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::remote::declared_pages;
use teletext_runtime::{Channel, ChannelConfig, Remote};

/// The time between messages, in milliseconds.
const STEP: u64 = 50;
/// The program's declaration: a page of each kind a viewer opens, input
/// on them reaching the program as lines.
const DECLARATION: &[u8] = br#"{"type":"pages","pages":[{"page":"100","title":"Index"},{"page":"110","title":"Long read","section":"Services"},{"page":"300","title":"Game"}]}"#;
/// Who sends each message, by its first byte: the two sessions and the program.
const SENDERS: u8 = 3;
/// The sender byte that is the program's.
const PROGRAM: u8 = 2;

fuzz_target!(|data: &[u8]| {
    let config = ChannelConfig { name: "Fuzz".into(), push_address: "ws://127.0.0.1/fuzz/push".into(), initial_page: 0x100, epoch_at_start_ms: 0 };
    let Ok(pages) = declared_pages(DECLARATION) else { return };
    let mut channel = Channel::new(config, vec![Box::new(Remote::new(pages))], 1);
    channel.start(0);
    channel.opened(1, 0);
    channel.opened(2, 0);
    let mut now = 0_u64;
    // Each message: a sender byte, a length byte, then that many bytes.
    let mut rest = data;
    while let [sender, length, tail @ ..] = rest {
        let take = usize::from(*length).min(tail.len());
        let (message, after) = tail.split_at(take);
        match sender % SENDERS {
            PROGRAM => channel.from_application(0, message, now),
            session => channel.received(u64::from(session) + 1, message, now),
        }
        now += STEP;
        channel.deadline(now);
        let _ = channel.take_effects();
        rest = after;
    }
    channel.closed(1, now);
    channel.closed(2, now);
});
