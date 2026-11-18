use alloc::vec::Vec;

use serde_json::{Value, json};

use super::*;
use crate::protocol::tests::base64;

/// A voice by named fields whose operator n has output level 10 n, so
/// that each operator's place among the voice parameters shows.
fn voice() -> Value {
    let operator = |level: u8| {
        json!({"eg_rates": [99, 99, 99, 99], "eg_levels": [99, 99, 99, 0], "break_point": 39, "left_depth": 0, "right_depth": 0,
        "left_curve": 0, "right_curve": 0, "rate_scaling": 0, "amplitude_sensitivity": 0, "velocity_sensitivity": 0, "output_level": level,
        "oscillator_mode": 0, "frequency_coarse": 1, "frequency_fine": 0, "detune": 7})
    };
    json!({"form": "fields", "operators": [operator(10), operator(20), operator(30), operator(40), operator(50), operator(60)],
        "pitch_eg_rates": [99, 99, 99, 99], "pitch_eg_levels": [50, 50, 50, 50], "algorithm": 4, "feedback": 6, "oscillator_sync": 1,
        "lfo_speed": 35, "lfo_delay": 0, "lfo_pitch_depth": 0, "lfo_amplitude_depth": 0, "lfo_sync": 1, "lfo_wave": 0,
        "pitch_sensitivity": 3, "transpose": 24, "name": "BRASS"})
}

fn patch_event(channel: u8, patch: &Value) -> Synth7PatchEvent {
    serde_json::from_value(json!({"time_us": 0, "channel": channel, "patch": patch})).unwrap()
}

fn synth() -> Synth {
    Synth::of(crate::protocol::SoundKind::Synth7).unwrap()
}

/// SRV-26, S7-28: each field lands on its voice parameter: operator n's
/// output level (offset 16) at `(6 - n) * 21 + 16`, the algorithm at 134
/// and the name, padded with spaces, at 145 to 154.
#[test]
fn the_fields_are_the_voice_parameters() {
    let Synth7Patch::Fields(fields) = patch_event(1, &voice()).patch else { panic!("fields") };
    let patch = voice_of_fields(&fields).unwrap();
    for operator in 1_u16..=6 {
        assert_eq!(patch.parameter((6 - operator) * 21 + 16), Some(u8::try_from(operator * 10).unwrap()), "operator {operator}");
    }
    assert_eq!(patch.parameter(134), Some(4));
    assert_eq!(patch.parameter(135), Some(6));
    let name: Vec<u8> = (145..=154).map(|number| patch.parameter(number).unwrap()).collect();
    assert_eq!(name, b"BRASS     ");
}

/// SRV-27, SRV-29: the same voice as fields and as a single-voice dump of
/// another channel writes the same single-voice dump on the event's
/// channel, with sub-status `0x00 | (c - 1)`.
#[test]
fn fields_and_bytes_write_the_same_dump() {
    let (by_fields, _) = Synth7Family::inline_patch(synth(), &patch_event(3, &voice())).unwrap();
    let [Wire::Sysex(dump)] = by_fields.as_slice() else { panic!("one dump: {by_fields:?}") };
    assert_eq!(dump.get(..3), Some(&[0x43, 0x02, 0x00][..]));
    // The same dump on channel 16, given as bytes on channel 3.
    let mut other = alloc::vec![0xF0];
    other.extend_from_slice(dump);
    if let Some(sub_status) = other.get_mut(2) {
        *sub_status = 0x0F;
    }
    let (by_bytes, _) = Synth7Family::inline_patch(synth(), &patch_event(3, &json!({"form": "bytes", "bytes": base64(&other)}))).unwrap();
    assert_eq!(by_bytes, by_fields);
    // A wrong checksum is refused with invalid_patch (SRV-27).
    let at = other.len() - 2;
    other[at] ^= 1;
    let refused = Synth7Family::inline_patch(synth(), &patch_event(3, &json!({"form": "bytes", "bytes": base64(&other)}))).unwrap_err();
    assert_eq!(refused.rule, Rule::InvalidPatch);
}

/// SRV-29: a voice parameter and a function parameter, every byte.
#[test]
fn a_named_parameter_writes_the_parameter_change() {
    let event = |channel: u8, set: Value| -> Synth7ParameterEvent { serde_json::from_value(json!({"time_us": 0, "channel": channel, "set": set})).unwrap() };
    // Voice parameter 105 (operator 1's EG RATE 1), group 0, on channel 3.
    let wires = Synth7Family::parameter(synth(), &event(3, json!({"op1_eg_rate_1": 50}))).unwrap();
    assert_eq!(wires, [Wire::Sysex(alloc::vec![0x43, 0x12, 0x00, 105, 50, 0xF7])]);
    // Voice parameter 155, the operator switch: its high bits in byte 3.
    let wires = Synth7Family::parameter(synth(), &event(1, json!({"operator_switch": 0x15}))).unwrap();
    assert_eq!(wires, [Wire::Sysex(alloc::vec![0x43, 0x10, 0x01, 155 - 128, 0x15, 0xF7])]);
    // Function parameter 77, group 2, on channel 1.
    let wires = Synth7Family::parameter(synth(), &event(1, json!({"after_touch_assign": 7}))).unwrap();
    assert_eq!(wires, [Wire::Sysex(alloc::vec![0x43, 0x10, 0x08, 77, 7, 0xF7])]);
    assert!(takes(&wires[0]).is_ok());
}

/// SRV-28: every one of the 170 names is the parameter its place in the
/// table gives: operators, then voice parameters 126 to 155, then function
/// parameters 64 to 77.
#[test]
fn every_named_parameter_has_its_number() {
    for (index, name) in (0_u16..).zip(Synth7Parameter::NAMES) {
        let parameter: Synth7Parameter = serde_json::from_value(json!({name: 0})).unwrap();
        let expected = match index {
            0..126 => Parameter::Voice((5 - index / 21) * 21 + index % 21),
            126..156 => Parameter::Voice(index),
            _ => Parameter::Function(index - 156 + 64),
        };
        assert_eq!(parameter_of(parameter).0, expected, "{name}");
    }
}

/// SRV-56, SRV-63: a bank of one voice is one 32-voice dump that the
/// engine's check takes, is the bank dump the music lifts, and addresses
/// no part.
#[test]
fn a_bank_of_one_voice_is_a_whole_dump() {
    let bank: Synth7Bank = serde_json::from_value(json!({"form": "patches", "patches": [voice()]})).unwrap();
    let image = Synth7Family::bank_image(synth(), &bank).unwrap();
    assert_eq!(image.len(), 4_104);
    let after = &image[1..];
    assert_eq!(bank_dump(after).unwrap(), Some(0));
    assert_eq!(part_of(after), None);
    assert!(check_bank(&image[..4_103]).is_err());
}
