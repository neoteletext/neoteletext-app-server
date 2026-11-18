//! server.md SRV-92 item 3: arbitrary bytes as each family's patch bytes
//! form, inline and in a bank's list, and as its bank bytes form: checked
//! and carried, or refused whole, no panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "common_synth.rs"]
mod common;

fuzz_target!(|data: &[u8]| {
    let [kind, rest @ ..] = data else { return };
    let (name, more) = common::NOTE_KINDS[usize::from(*kind) % common::NOTE_KINDS.len()];
    let bytes = common::base64(rest);
    let mut channel = common::channel();
    let image = format!(r#"{{"type":"sound","kind":"{name}_bank","connections":"all","handle":1,"generation":0,"bank":{{"form":"bytes","bytes":"{bytes}"}}}}"#);
    channel.from_program(image.as_bytes(), 1);
    let list = format!(
        r#"{{"type":"sound","kind":"{name}_bank","connections":"all","handle":3,"generation":0,"bank":{{"form":"patches","patches":[{{"form":"bytes","bytes":"{bytes}"}}]}}}}"#
    );
    channel.from_program(list.as_bytes(), 1);
    let inline = format!(
        r#"{{"type":"sound","kind":"{name}","connections":"all","handle":2,"generation":0{more},"banks":[1],"lift_banks":[],"length":{{"tail_us":0}},"music":{{"form":"events","events":[{{"event":"patch","time_us":0,"channel":1,"patch":{{"form":"bytes","bytes":"{bytes}"}}}}]}}}}"#
    );
    channel.from_program(inline.as_bytes(), 1);
    let _ = channel.take_effects();
});
