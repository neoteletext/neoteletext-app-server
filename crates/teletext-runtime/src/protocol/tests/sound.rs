//! `sound`, each `kind` (the engine's audio profile).

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

fn sid() -> Value {
    sound("sid", json!({"handle": 1, "generation": 2, "images_per_block": 1, "chip": "6581", "clock": "pal", "images": base64(&[0xFF; 25])}))
}

fn dx7() -> Value {
    sound("dx7", json!({"handle": 1, "generation": 2, "slots": 2, "bank": 0, "blocks": base64(&[0, 0x90, 60, 100, 0, 0, 0, 0])}))
}

fn bank() -> Value {
    sound("dx7_bank", json!({"handle": 1, "generation": 2, "patches": base64(&[0; 128])}))
}

fn play() -> Value {
    sound("play", json!({"channel": 3, "handle": 1, "generation": 2, "gain": 100, "fade_ms": 10, "loop": false, "after": true, "duck": 6, "page": "100"}))
}

fn every() -> [Value; 16] {
    [
        opus(),
        sound("opus_live", json!({"handle": 1, "generation": 2, "packet_bytes": 40})),
        sound("opus_packets", json!({"handle": 1, "generation": 2, "packet_bytes": 2, "first": 0, "packets": base64(&[1, 2])})),
        object(),
        sound("live", json!({"handle": 1, "generation": 2, "rate": 48_000})),
        sound("blocks", json!({"handle": 1, "generation": 2, "rate": 48_000, "first": 3, "pcm": base64(&[0, 0])})),
        sid(),
        sound("sid_live", json!({"handle": 1, "generation": 2, "images_per_block": 4, "chip": "8580", "clock": "ntsc"})),
        sound("sid_blocks", json!({"handle": 1, "generation": 2, "images_per_block": 2, "first": 65_535, "images": base64(&[0xFF; 50])})),
        dx7(),
        sound("dx7_live", json!({"handle": 1, "generation": 2, "slots": 16, "bank": 255})),
        sound("dx7_blocks", json!({"handle": 1, "generation": 2, "slots": 1, "first": 0, "blocks": base64(&[0, 0x90, 60, 100])})),
        bank(),
        play(),
        sound("stop", json!({"channel": 0, "fade_ms": 0})),
        sound("gain", json!({"channel": 1, "gain": 50, "fade_ms": 0})),
    ]
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
        accepted(&with(&with(&sid(), "/images_per_block", json!(images_per_block)), "/images", json!(base64(&alloc::vec![0; bytes]))));
    }
    refused(&with(&sid(), "/images_per_block", json!(3)), "images_per_block", Rule::NotListed);
    refused(&with(&sid(), "/chip", json!("6580")), "chip", Rule::NotListed);
    refused(&with(&sid(), "/clock", json!("Pal")), "clock", Rule::NotListed);
    accepted(&with(&sid(), "/chip", json!("8580")));
    accepted(&with(&sid(), "/clock", json!("ntsc")));
    bounds(&sid(), "/handle", "handle", 255, Rule::WrongType);
    bounds(&sid(), "/generation", "generation", 65_535, Rule::WrongType);
    bounds(&with(&dx7(), "/blocks", json!(base64(&[0; 64]))), "/slots", "slots", 16, Rule::OutOfRange);
    refused(&with(&dx7(), "/slots", json!(0)), "slots", Rule::OutOfRange);
    bounds(&dx7(), "/bank", "bank", 255, Rule::WrongType);
    base64_field(&sid(), "/images", "images", &[1; 50], &[1; 26]);
    base64_field(&dx7(), "/blocks", "blocks", &[0; 16], &[0; 4]);
    base64_field(&bank(), "/patches", "patches", &[127; 256], &[0; 127]);
    refused(&with(&dx7(), "/blocks", json!(base64(&[0, 0x90, 60, 100, 0, 0xA0, 0, 0]))), "blocks", Rule::OutsideProfile);
}
