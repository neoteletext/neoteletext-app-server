//! A channel of every showcase application, fed any input streams from
//! two sessions, with time passing: no panic, whatever the bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::{Channel, ChannelConfig};

/// The time between messages, in milliseconds.
const STEP: u64 = 50;

fuzz_target!(|data: &[u8]| {
    let config = ChannelConfig { name: "Fuzz".into(), push_address: "ws://127.0.0.1/fuzz/push".into(), initial_page: 0x100, epoch_at_start_ms: 0 };
    let applications = teletext_showcase::channels(&[]).into_iter().flat_map(|spec| spec.applications).collect();
    let mut channel = Channel::new(config, applications, 1);
    channel.start(0);
    channel.opened(1, 0);
    channel.opened(2, 0);
    let mut now = 0_u64;
    // Each message: a session byte, a length byte, then that many bytes.
    let mut rest = data;
    while let [session, length, tail @ ..] = rest {
        let take = usize::from(*length).min(tail.len());
        let (message, after) = tail.split_at(take);
        channel.received(u64::from(session % 2 + 1), message, now);
        now += STEP;
        channel.deadline(now);
        let _ = channel.take_effects();
        rest = after;
    }
    channel.closed(1, now);
    channel.closed(2, now);
});
