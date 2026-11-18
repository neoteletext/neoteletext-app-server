//! server.md SRV-92 item 2: a live synthesizer sound and a sequence of
//! events messages for its handle, each advancing `until_us` by an amount
//! and carrying raw MIDI the bytes choose, with the channel's clock moving
//! on: every message built, paced and sent or refused whole, no panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "common_synth.rs"]
mod common;

/// The bytes of one events message's choice: how far `until_us` moves, in
/// milliseconds, and how many bytes of raw MIDI it carries.
const STEP: usize = 2;

fuzz_target!(|data: &[u8]| {
    let [kind, rest @ ..] = data else { return };
    let (name, more) = common::NOTE_KINDS[usize::from(*kind) % common::NOTE_KINDS.len()];
    let mut channel = common::channel();
    let bank = format!(r#"{{"type":"sound","kind":"{name}_bank","connections":"all","handle":1,"generation":0,"bank":{{"form":"bytes","bytes":"AA=="}}}}"#);
    channel.from_program(bank.as_bytes(), 1);
    let live = format!(r#"{{"type":"sound","kind":"{name}_live","connections":"all","handle":2,"generation":0{more},"banks":[1]}}"#);
    channel.from_program(live.as_bytes(), 1);
    let mut until_us: u64 = 0;
    let mut now: u64 = 1;
    let mut rest = rest;
    while let [advance, length, after @ ..] = rest {
        let (raw, after) = after.split_at(usize::from(*length).min(after.len()));
        rest = after;
        let time = until_us;
        until_us += u64::from(*advance) * 10_000 + 1;
        let bytes = common::base64(raw);
        let events = if raw.is_empty() { String::new() } else { format!(r#"{{"event":"raw","time_us":{time},"bytes":"{bytes}"}}"#) };
        let line =
            format!(r#"{{"type":"sound","kind":"{name}_events","connections":"all","handle":2,"generation":0,"until_us":{until_us},"events":[{events}]}}"#);
        channel.from_program(line.as_bytes(), now);
        now += u64::from(*advance) * STEP as u64;
        channel.deadline(now);
        let _ = channel.take_effects();
    }
});
