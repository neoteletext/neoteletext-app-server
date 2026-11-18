use alloc::vec::Vec;

use neoteletext_audio_instruments::synthd::interface::{self as engine, DEFAULT_PATCH, PATCH_BYTES};
use serde_json::{Value, json};

use super::*;
use crate::protocol::SoundKind;
use crate::protocol::tests::base64;
use crate::protocol::{SynthDBank, SynthDParameterEvent, SynthDPatchEvent};

fn synth() -> Synth {
    Synth::of(SoundKind::SynthD).unwrap()
}

/// The fields of the engine's default patch (SD-21), named as SRV-34 names them.
fn default_fields() -> Value {
    let patch = engine::Patch::from_values(&DEFAULT_PATCH);
    let mut map = serde_json::Map::new();
    map.insert("glide".into(), json!(patch.glide));
    map.insert("oscillator_2_frequency".into(), json!(patch.oscillator_2_frequency));
    map.insert("oscillator_3_frequency".into(), json!(patch.oscillator_3_frequency));
    map.insert("external_input_volume".into(), json!(patch.external_input_volume));
    map.insert("oscillator_1_volume".into(), json!(patch.oscillator_1_volume));
    map.insert("oscillator_2_volume".into(), json!(patch.oscillator_2_volume));
    map.insert("oscillator_3_volume".into(), json!(patch.oscillator_3_volume));
    map.insert("noise_volume".into(), json!(patch.noise_volume));
    map.insert("cutoff_frequency".into(), json!(patch.cutoff_frequency));
    map.insert("emphasis".into(), json!(patch.emphasis));
    map.insert("filter_attack_time".into(), json!(patch.filter_attack_time));
    map.insert("filter_decay_time".into(), json!(patch.filter_decay_time));
    map.insert("filter_sustain_level".into(), json!(patch.filter_sustain_level));
    map.insert("amount_of_contour".into(), json!(patch.amount_of_contour));
    map.insert("loudness_attack_time".into(), json!(patch.loudness_attack_time));
    map.insert("loudness_decay_time".into(), json!(patch.loudness_decay_time));
    map.insert("loudness_sustain_level".into(), json!(patch.loudness_sustain_level));
    map.insert("glide_switch".into(), json!(patch.glide_switch));
    map.insert("oscillator_1_range_octave".into(), json!(patch.oscillator_1_range_octave));
    map.insert("oscillator_2_range_octave".into(), json!(patch.oscillator_2_range_octave));
    map.insert("oscillator_3_range_octave".into(), json!(patch.oscillator_3_range_octave));
    map.insert("oscillator_3_control".into(), json!(patch.oscillator_3_control));
    map.insert("oscillator_3_range_lo_hi".into(), json!(patch.oscillator_3_range_lo_hi));
    map.insert("external_input_switch".into(), json!(patch.external_input_switch));
    map.insert("oscillator_1_switch".into(), json!(patch.oscillator_1_switch));
    map.insert("oscillator_2_switch".into(), json!(patch.oscillator_2_switch));
    map.insert("oscillator_3_switch".into(), json!(patch.oscillator_3_switch));
    map.insert("noise_switch".into(), json!(patch.noise_switch));
    map.insert("decay_switch".into(), json!(patch.decay_switch));
    map.insert("tune".into(), json!(patch.tune));
    map.insert("modulation_mix".into(), json!(patch.modulation_mix));
    map.insert("oscillator_1_range_lo_hi".into(), json!(patch.oscillator_1_range_lo_hi));
    map.insert("oscillator_1_waveform".into(), json!(patch.oscillator_1_waveform));
    map.insert("oscillator_2_range_lo_hi".into(), json!(patch.oscillator_2_range_lo_hi));
    map.insert("oscillator_2_waveform".into(), json!(patch.oscillator_2_waveform));
    map.insert("oscillator_3_waveform".into(), json!(patch.oscillator_3_waveform));
    map.insert("main_output_volume".into(), json!(patch.main_output_volume));
    map.insert("oscillator_modulation".into(), json!(patch.oscillator_modulation));
    map.insert("noise_colour".into(), json!(patch.noise_colour));
    map.insert("filter_modulation".into(), json!(patch.filter_modulation));
    map.insert("keyboard_control_1".into(), json!(patch.keyboard_control_1));
    map.insert("keyboard_control_2".into(), json!(patch.keyboard_control_2));
    Value::Object(map)
}

fn patch_event(patch: &Value) -> SynthDPatchEvent {
    serde_json::from_value(json!({"time_us": 0, "channel": 3, "patch": patch})).unwrap()
}

/// SRV-34, SRV-55: the default patch's fields pack to SD-21's 60 bytes, and
/// the fields and the bytes form write the same 60 control changes (SD-26).
#[test]
fn fields_and_bytes_write_the_same_inline_patch() {
    let fields = json!({"form": "fields"})
        .as_object()
        .unwrap()
        .clone()
        .into_iter()
        .chain(default_fields().as_object().unwrap().clone())
        .collect::<serde_json::Map<_, _>>();
    let from_fields = SynthDFamily::inline_patch(synth(), &patch_event(&Value::Object(fields))).unwrap().0;
    let from_bytes = SynthDFamily::inline_patch(synth(), &patch_event(&json!({"form": "bytes", "bytes": base64(&DEFAULT_PATCH)}))).unwrap().0;
    assert_eq!(from_fields, from_bytes);
    let mut expected = Vec::new();
    engine::control_changes(&DEFAULT_PATCH, 2, &mut |message| {
        if let Message::Channel(message) = message {
            expected.push(Wire::Channel(message));
        }
    });
    assert_eq!(from_fields, expected);
    assert_eq!(from_fields.len(), PATCH_BYTES);
}

/// SRV-35: 61 bytes and a byte of 128 are refused as invalid patches.
#[test]
fn faulty_bytes_are_refused() {
    for bytes in [alloc::vec![0_u8; 61], {
        let mut bytes = DEFAULT_PATCH.to_vec();
        bytes[5] = 128;
        bytes
    }] {
        let fault = SynthDFamily::inline_patch(synth(), &patch_event(&json!({"form": "bytes", "bytes": base64(&bytes)}))).unwrap_err();
        assert_eq!(fault.rule, Rule::InvalidPatch);
        assert!(fault.detail.starts_with("SRV-35"), "{}", fault.detail);
    }
}

fn parameter(set: &Value) -> Vec<Wire> {
    let event: SynthDParameterEvent = serde_json::from_value(json!({"time_us": 0, "channel": 1, "set": set})).unwrap();
    SynthDFamily::parameter(synth(), &event).unwrap()
}

fn cc(number: u8, value: u8) -> Wire {
    Wire::Channel(ChannelMessage::new(status::CONTROL_CHANGE, 0, number, value))
}

/// SRV-36: a pair is its coarse then fine controller, a single its one.
#[test]
fn a_named_parameter_writes_its_controllers() {
    assert_eq!(parameter(&json!({"glide": 16_383})), [cc(5, 127), cc(37, 127)]);
    assert_eq!(parameter(&json!({"tune": 200})), [cc(89, 1), cc(90, 72)]);
    assert_eq!(parameter(&json!({"keyboard_control_2": 9})), [cc(113, 9)]);
}

/// SRV-35, SRV-60: a bank as patches and as bytes is the same image, and
/// the engine's check refuses a length not a multiple of 60.
#[test]
fn a_bank_is_built_and_checked_by_the_engine() {
    let bank = |value: Value| -> SynthDBank { serde_json::from_value(value).unwrap() };
    let fields = {
        let mut map = default_fields().as_object().unwrap().clone();
        map.insert("form".into(), json!("fields"));
        Value::Object(map)
    };
    let from_patches =
        SynthDFamily::bank_image(synth(), &bank(json!({"form": "patches", "patches": [fields, {"form": "bytes", "bytes": base64(&DEFAULT_PATCH)}]}))).unwrap();
    let doubled: Vec<u8> = DEFAULT_PATCH.iter().chain(&DEFAULT_PATCH).copied().collect();
    assert_eq!(from_patches, doubled);
    assert_eq!(SynthDFamily::bank_image(synth(), &bank(json!({"form": "bytes", "bytes": base64(&doubled)}))).unwrap(), doubled);
    let fault = SynthDFamily::bank_image(synth(), &bank(json!({"form": "bytes", "bytes": base64(&doubled[..61])}))).unwrap_err();
    assert_eq!(fault.rule, Rule::InvalidBank);
}

/// SD-1, SRV-24: synthD takes its panel's controllers and refuses system
/// exclusive and channel pressure.
#[test]
fn takes_follows_the_engine() {
    assert!(takes(&cc(5, 1)).is_ok());
    assert!(takes(&Wire::Sysex(alloc::vec![0x7D, 0xF7])).is_err());
    assert!(takes(&Wire::Channel(ChannelMessage::new(status::CHANNEL_PRESSURE, 0, 1, 0))).is_err());
}
