//! `sound`, each `kind` (the engine's audio profile).

use alloc::vec::Vec;

use serde_json::{Value, json};

use super::{accepted, base64, base64_field, bounds, refused, with, without};
use crate::protocol::Rule;

fn sound(kind: &str, fields: Value) -> Value {
    let mut sound = json!({"type": "sound", "kind": kind, "connections": [1]});
    let Value::Object(fields) = fields else { panic!("fields are an object") };
    for (name, value) in fields {
        sound[name] = value;
    }
    sound
}

fn opus() -> Value {
    sound("opus", json!({"handle": 1, "generation": 2, "packet_bytes": 4, "samples": 960, "packets": base64(&[1, 2, 3, 4])}))
}

fn object() -> Value {
    sound("object", json!({"handle": 1, "generation": 2, "rate": 8_000, "pcm": base64(&[0, 0, 1, 0])}))
}

fn synth6581() -> Value {
    sound("synth6581", json!({"handle": 1, "generation": 2, "images_per_block": 1, "clock": "pal", "images": base64(&[0xFF; 25])}))
}

fn synth7() -> Value {
    sound("synth7", json!({"handle": 1, "generation": 2, "slots": 2, "bank": 0, "blocks": base64(&[0, 0x90, 60, 100, 0, 0, 0, 0])}))
}

fn bank() -> Value {
    sound("synth7_bank", json!({"handle": 1, "generation": 2, "patches": base64(&[0; 128])}))
}

fn synth106() -> Value {
    sound("synth106", json!({"handle": 1, "generation": 2, "slots": 1, "bank": 0, "blocks": base64(&[0, 0x90, 60, 100])}))
}

fn synth8() -> Value {
    sound("synth8", json!({"handle": 1, "generation": 2, "key_mode": "split", "split_key": 60, "slots": 1, "bank": 0, "blocks": base64(&[0; 4])}))
}

fn synth106_bank() -> Value {
    sound("synth106_bank", json!({"handle": 1, "generation": 2, "patches": base64(&[[0; 16].as_slice(), &[2, 0]].concat())}))
}

fn play() -> Value {
    sound("play", json!({"channel": 3, "handle": 1, "generation": 2, "gain": 100, "fade_ms": 10, "loop": false, "after": true, "duck": 6, "page": "100"}))
}

/// A synthesizer's four kinds, of one that has no field of its own, with
/// a bank of one patch of `patch`.
fn four(name: &str, patch: &[u8]) -> [Value; 4] {
    [
        sound(name, json!({"handle": 1, "generation": 2, "slots": 1, "bank": 0, "blocks": base64(&[0; 4])})),
        sound(&alloc::format!("{name}_live"), json!({"handle": 1, "generation": 2, "slots": 16, "bank": 255})),
        sound(&alloc::format!("{name}_blocks"), json!({"handle": 1, "generation": 2, "slots": 1, "first": 0, "blocks": base64(&[0; 4])})),
        sound(&alloc::format!("{name}_bank"), json!({"handle": 1, "generation": 2, "patches": base64(patch)})),
    ]
}

/// Every kind of sound: the six of Opus and samples, three for each of
/// synth6581 and synth8580, four for synth7 and for each of the eight
/// analogue synthesizers (DEC-6461), and the three commands.
fn every() -> Vec<Value> {
    let synth106_patch = [[0; 16].as_slice(), &[2, 0]].concat();
    let synth53_patch = [[0xFF; 22].as_slice(), &[0x7F, 0x7F]].concat();
    let mut every = alloc::vec![
        opus(),
        sound("opus_live", json!({"handle": 1, "generation": 2, "packet_bytes": 40})),
        sound("opus_packets", json!({"handle": 1, "generation": 2, "packet_bytes": 2, "first": 0, "packets": base64(&[1, 2])})),
        object(),
        sound("live", json!({"handle": 1, "generation": 2, "rate": 48_000})),
        sound("blocks", json!({"handle": 1, "generation": 2, "rate": 48_000, "first": 3, "pcm": base64(&[0, 0])})),
        synth6581(),
        sound("synth6581_live", json!({"handle": 1, "generation": 2, "images_per_block": 4, "clock": "ntsc"})),
        sound("synth6581_blocks", json!({"handle": 1, "generation": 2, "images_per_block": 2, "first": 65_535, "images": base64(&[0xFF; 50])})),
        with(&synth6581(), "/kind", json!("synth8580")),
        sound("synth8580_live", json!({"handle": 1, "generation": 2, "images_per_block": 4, "clock": "ntsc"})),
        sound("synth8580_blocks", json!({"handle": 1, "generation": 2, "images_per_block": 2, "first": 65_535, "images": base64(&[0xFF; 50])})),
        synth7(),
        sound("synth7_live", json!({"handle": 1, "generation": 2, "slots": 16, "bank": 255})),
        sound("synth7_blocks", json!({"handle": 1, "generation": 2, "slots": 1, "first": 0, "blocks": base64(&[0, 0x90, 60, 100])})),
        bank(),
        synth106(),
        synth106_bank(),
        synth8(),
        sound("synth8_live", json!({"handle": 1, "generation": 2, "key_mode": "whole", "split_key": 0, "slots": 1, "bank": 0})),
        sound("synth8_blocks", json!({"handle": 1, "generation": 2, "slots": 1, "first": 0, "blocks": base64(&[0; 4])})),
        sound("synth8_bank", json!({"handle": 1, "generation": 2, "patches": base64(&[0; 43])})),
        sound("synth80", json!({"handle": 1, "generation": 2, "slots": 1, "bank": 0, "blocks": base64(&[0, 0xA0, 60, 100])})),
        sound("synth80_blocks", json!({"handle": 1, "generation": 2, "slots": 1, "first": 0, "blocks": base64(&[0, 0xD0, 60, 0])})),
        play(),
        sound("stop", json!({"channel": 0, "fade_ms": 0})),
        sound("gain", json!({"channel": 1, "gain": 50, "fade_ms": 0})),
    ];
    for (name, patch) in [
        ("synth106", synth106_patch.as_slice()),
        ("synth60", &synth106_patch),
        ("synthd", &[0; 36]),
        ("synth53", &synth53_patch),
        ("synth52", &synth53_patch),
        ("synth51", &synth53_patch),
        ("synth80", &[0; 85]),
    ] {
        every.extend(four(name, patch));
    }
    every
}

/// DEC-6461: a maker's name is no kind of sound, and neither is the kind
/// of a family; the chip and the model are no fields, the kind naming
/// them.
#[test]
fn the_kinds_and_fields_the_names_replaced_are_refused() {
    for old in ["sid", "sid_live", "dx7", "dx7_bank", "juno", "juno_live", "minimoog", "prophet5", "jupiter8", "cs80", "cs80_bank", "synth_d", "synthD"] {
        refused(&with(&synth106(), "/kind", json!(old)), "kind", Rule::NotListed);
    }
    refused(&with(&synth6581(), "/chip", json!("6581")), "chip", Rule::UnknownField);
    refused(&with(&with(&synth6581(), "/kind", json!("synth8580")), "/chip", json!("8580")), "chip", Rule::UnknownField);
    refused(&with(&synth106(), "/model", json!("106")), "model", Rule::UnknownField);
    refused(&with(&with(&synth106(), "/kind", json!("synth53")), "/model", json!("revision_3")), "model", Rule::UnknownField);
}

/// REQ-1476: every kind of `sound`.
#[test]
fn every_kind_of_sound_is_read_whole() {
    for sound in every() {
        accepted(&sound);
        refused(&with(&sound, "/extra", json!(1)), "extra", Rule::UnknownField);
        refused(&without(&sound, "/connections"), "connections", Rule::MissingField);
    }
}

#[test]
fn a_sound_names_the_field_at_fault() {
    refused(&with(&opus(), "/kind", json!("mp3")), "kind", Rule::NotListed);
    refused(&without(&opus(), "/kind"), "", Rule::MissingField);
    refused(&with(&opus(), "/kind", json!(1)), "kind", Rule::WrongType);
    refused(&with(&opus(), "/connections/0", json!(0)), "connections[0]", Rule::OutOfRange);
    refused(&with(&object(), "/rate", json!(44_000)), "rate", Rule::NotListed);
    bounds(&opus(), "/handle", "handle", 255, Rule::WrongType);
    bounds(&opus(), "/generation", "generation", 65_535, Rule::WrongType);
    bounds(&play(), "/channel", "channel", 3, Rule::OutOfRange);
    bounds(&play(), "/gain", "gain", 100, Rule::OutOfRange);
    bounds(&play(), "/duck", "duck", 6, Rule::OutOfRange);
    bounds(&play(), "/fade_ms", "fade_ms", 65_535, Rule::WrongType);
    bounds(&with(&opus(), "/packets", json!(base64(&[0; 40]))), "/packet_bytes", "packet_bytes", 40, Rule::OutOfRange);
    refused(&with(&opus(), "/packet_bytes", json!(0)), "packet_bytes", Rule::OutOfRange);
    refused(&with(&play(), "/page", json!("900")), "page", Rule::WrongType);
    refused(&with(&play(), "/loop", json!(1)), "loop", Rule::WrongType);
}

#[test]
fn a_sounds_data_is_exact_base64() {
    base64_field(&opus(), "/packets", "packets", &[1, 2, 3, 4, 5, 6, 7, 8], &[1, 2, 3]);
    base64_field(&object(), "/pcm", "pcm", &[0, 1, 2, 3], &[0, 1, 2]);
}

/// REQ-6413: the instrument sounds' fields, each at its bounds and past
/// them, and their data exact base64 of whole blocks.
#[test]
fn an_instrument_sound_names_the_field_at_fault() {
    for (images_per_block, bytes) in [(1, 25), (2, 50), (4, 100)] {
        accepted(&with(&with(&synth6581(), "/images_per_block", json!(images_per_block)), "/images", json!(base64(&alloc::vec![0; bytes]))));
    }
    refused(&with(&synth6581(), "/images_per_block", json!(3)), "images_per_block", Rule::NotListed);
    refused(&with(&synth6581(), "/clock", json!("Pal")), "clock", Rule::NotListed);
    accepted(&with(&synth6581(), "/clock", json!("ntsc")));
    bounds(&synth6581(), "/handle", "handle", 255, Rule::WrongType);
    bounds(&synth6581(), "/generation", "generation", 65_535, Rule::WrongType);
    bounds(&with(&synth7(), "/blocks", json!(base64(&[0; 64]))), "/slots", "slots", 16, Rule::OutOfRange);
    refused(&with(&synth7(), "/slots", json!(0)), "slots", Rule::OutOfRange);
    bounds(&synth7(), "/bank", "bank", 255, Rule::WrongType);
    base64_field(&synth6581(), "/images", "images", &[1; 50], &[1; 26]);
    base64_field(&synth7(), "/blocks", "blocks", &[0; 16], &[0; 4]);
    base64_field(&bank(), "/patches", "patches", &[127; 256], &[0; 127]);
    refused(&with(&synth7(), "/blocks", json!(base64(&[0, 0x90, 60, 100, 0, 0xA0, 0, 0]))), "blocks", Rule::OutsideProfile);
}

/// TODO-6430, DEC-6429: the analogue synthesizers' fields, each at its
/// bounds and past them, and their data exact base64 of whole blocks and
/// patches.
#[test]
fn a_synthesizer_sound_names_the_field_at_fault() {
    bounds(&synth8(), "/split_key", "split_key", 127, Rule::OutOfRange);
    refused(&with(&synth8(), "/key_mode", json!("layer")), "key_mode", Rule::NotListed);
    for mode in ["whole", "dual", "split"] {
        accepted(&with(&synth8(), "/key_mode", json!(mode)));
    }
    bounds(&with(&synth106(), "/blocks", json!(base64(&[0; 64]))), "/slots", "slots", 16, Rule::OutOfRange);
    refused(&with(&synth106(), "/slots", json!(0)), "slots", Rule::OutOfRange);
    bounds(&synth106(), "/bank", "bank", 255, Rule::WrongType);
    base64_field(&synth106(), "/blocks", "blocks", &[0; 8], &[0; 6]);
    base64_field(&synth106_bank(), "/patches", "patches", &[[0; 16].as_slice(), &[1, 0]].concat(), &[0; 17]);
    refused(&with(&synth106_bank(), "/patches", json!(base64(&[0; 18]))), "patches", Rule::OutsideProfile);
    refused(&with(&synth106(), "/blocks", json!(base64(&[0, 0xA0, 60, 100]))), "blocks", Rule::OutsideProfile);
}
