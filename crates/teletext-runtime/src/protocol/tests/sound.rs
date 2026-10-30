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

fn play() -> Value {
    sound("play", json!({"channel": 3, "handle": 1, "generation": 2, "gain": 100, "fade_ms": 10, "loop": false, "after": true, "duck": 6, "page": "100"}))
}

fn every() -> [Value; 9] {
    [
        opus(),
        sound("opus_live", json!({"handle": 1, "generation": 2, "packet_bytes": 40})),
        sound("opus_packets", json!({"handle": 1, "generation": 2, "packet_bytes": 2, "first": 0, "packets": base64(&[1, 2])})),
        object(),
        sound("live", json!({"handle": 1, "generation": 2, "rate": 48_000})),
        sound("blocks", json!({"handle": 1, "generation": 2, "rate": 48_000, "first": 3, "pcm": base64(&[0, 0])})),
        play(),
        sound("stop", json!({"channel": 0, "fade_ms": 0})),
        sound("gain", json!({"channel": 1, "gain": 50, "fade_ms": 0})),
    ]
}

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
