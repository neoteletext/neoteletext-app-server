//! server.md SRV-92 item 1: arbitrary bytes as one `sound` of the program protocol
//! line of a synthesizer kind, handled to the end: read, checked, built
//! and sent, or refused whole. The server's encoder reads every block it
//! writes with the receiver's reader before sending, so an accepted line
//! is one whose blocks the receiver wholly acts on; here, no panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "common_synth.rs"]
mod common;

fuzz_target!(|data: &[u8]| {
    let mut channel = common::channel();
    // The bytes as a line of their own.
    channel.from_program(data, 1);
    // And as raw MIDI of a stored sound of each family, at a time the first
    // bytes choose, after a bank of the same bytes.
    let [kind, time, rest @ ..] = data else { return };
    let (name, more) = common::NOTE_KINDS[usize::from(*kind) % common::NOTE_KINDS.len()];
    let bytes = common::base64(rest);
    let bank = format!(r#"{{"type":"sound","kind":"{name}_bank","connections":"all","handle":1,"generation":0,"bank":{{"form":"bytes","bytes":"{bytes}"}}}}"#);
    channel.from_program(bank.as_bytes(), 2);
    let time = u64::from(*time) * 1_000;
    let line = format!(
        r#"{{"type":"sound","kind":"{name}","connections":"all","handle":2,"generation":0{more},"banks":[1],"lift_banks":[],"length":{{"tail_us":0}},"music":{{"form":"events","events":[{{"event":"raw","time_us":{time},"bytes":"{bytes}"}},{{"event":"system_exclusive","time_us":{time},"bytes":"{bytes}"}}]}}}}"#
    );
    channel.from_program(line.as_bytes(), 3);
    let _ = channel.take_effects();
});
