//! server.md SRV-92 item 4: arbitrary bytes as a Standard MIDI File for
//! each family, with lifted banks named, read by the server's reader
//! (SRV-81 to SRV-86) and built, or refused whole, no panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "common_synth.rs"]
mod common;

fuzz_target!(|data: &[u8]| {
    let [kind, rest @ ..] = data else { return };
    let bytes = common::base64(rest);
    let mut channel = common::channel();
    if usize::from(*kind) % (common::NOTE_KINDS.len() + 1) == common::NOTE_KINDS.len() {
        let line = format!(
            r#"{{"type":"sound","kind":"synth6581","connections":"all","handle":2,"generation":0,"clock":"pal","length":{{"tail_us":0}},"music":{{"form":"midi_file","bytes":"{bytes}"}}}}"#
        );
        channel.from_program(line.as_bytes(), 1);
        return;
    }
    let (name, more) = common::NOTE_KINDS[usize::from(*kind) % common::NOTE_KINDS.len()];
    let bank = format!(r#"{{"type":"sound","kind":"{name}_bank","connections":"all","handle":1,"generation":0,"bank":{{"form":"bytes","bytes":"AA=="}}}}"#);
    channel.from_program(bank.as_bytes(), 1);
    let line = format!(
        r#"{{"type":"sound","kind":"{name}","connections":"all","handle":2,"generation":0{more},"banks":[1],"lift_banks":[{{"handle":5,"generation":0}}],"length":{{"tail_us":0}},"music":{{"form":"midi_file","bytes":"{bytes}"}}}}"#
    );
    channel.from_program(line.as_bytes(), 1);
    let _ = channel.take_effects();
});
