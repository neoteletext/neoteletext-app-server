//! synth80's call site: named fields, presets and bytes give one patch,
//! and an inline patch is restated as the patch (SRV-43 to SRV-45, S80-40,
//! S80-46).

use neoteletext_audio::midi::{Banks, Message};
use neoteletext_audio_instruments::synth80::interface::{self as engine, PATCH_BYTES, controller_of};
use serde_json::{Value, json};

use super::{checked, panel_patch, place_of_parameter};
use crate::protocol::{SoundKind, Synth80Panel, Synth80Parameter};
use crate::sound::family::Synth;
use crate::sound::tracker::{TrackHint, Tracker};
use crate::sound::wire::Wire;

/// A panel whose every byte is its own place: channel I's tone 0 to 25,
/// channel II a preset, the master controls 60 to 92.
fn panel() -> Value {
    let mut value: Value = serde_json::from_str(r#"{"channel_1": {"form": "fields", "pwm_speed": 0, "pwm": 1, "pw": 2, "square": 3, "sawtooth": 4, "noise": 5, "hpf": 6, "hpf_resonance": 7, "lpf": 8, "lpf_resonance": 9, "filter_initial_level": 10, "filter_attack_level": 11, "filter_attack": 12, "filter_decay": 13, "filter_release": 14, "vcf_level": 15, "sine": 16, "amplifier_attack": 17, "amplifier_decay": 18, "amplifier_sustain": 19, "amplifier_release": 20, "level": 21, "initial_brilliance": 22, "initial_level": 23, "after_brilliance": 24, "after_level": 25}, "channel_2": {"form": "preset", "name": "brass_3"}, "feet_1": 60, "feet_2": 61, "detune_channel_2": 62, "mix": 63, "brilliance": 64, "resonance": 65, "keyboard_brilliance_low": 66, "keyboard_brilliance_high": 67, "keyboard_level_low": 68, "keyboard_level_high": 69, "sub_oscillator_function": 70, "sub_oscillator_speed": 71, "sub_oscillator_vco": 72, "sub_oscillator_vcf": 73, "sub_oscillator_vca": 74, "touch_pitch_bend": 75, "touch_speed": 76, "touch_vco": 77, "touch_vcf": 78, "ring_modulator_attack_time": 79, "ring_modulator_decay_time": 80, "ring_modulator_depth": 81, "ring_modulator_speed": 82, "ring_modulator_modulation": 83, "sustain_mode": 84, "sustain": 85, "glide_mode": 86, "glide_speed": 87, "tremolo_chorus": 88, "chorus": 89, "tremolo": 90, "tremolo_chorus_speed": 91, "tremolo_chorus_depth": 92}"#).unwrap();
    value["channel_1"]["form"] = json!("fields");
    value
}

/// SRV-43, SRV-44: a patch's fields, with a preset tone taken from the
/// engine's table, are the 85 bytes the engine reads.
#[test]
fn fields_and_bytes_are_one_patch() {
    let panel: Synth80Panel = serde_json::from_value(panel()).unwrap();
    let patch = panel_patch(&panel).unwrap();
    let bytes = engine::pack_patch(&patch);
    assert_eq!(&bytes[..26], &core::array::from_fn::<u8, 26, _>(|place| u8::try_from(place).unwrap()));
    assert_eq!(&bytes[26..52], &engine::preset("BRASS 3").unwrap().to_bytes());
    assert_eq!(bytes[52], 60);
    assert_eq!(bytes[PATCH_BYTES - 1], 92);
    assert_eq!(checked(&bytes).unwrap(), patch);
}

/// SRV-45: the master controls first, then channel I's and channel II's
/// tones, each written as the controller of its byte.
#[test]
fn parameters_move_their_bytes() {
    let place = |name: &str| place_of_parameter(serde_json::from_value::<Synth80Parameter>(json!({name: 9})).unwrap());
    assert_eq!(place("feet_1"), (52, 9));
    assert_eq!(controller_of(52), Some(102));
    assert_eq!(place("tremolo_chorus_depth"), (84, 9));
    assert_eq!(controller_of(84), Some(31));
    assert_eq!(place("channel_1_pwm_speed"), (0, 9));
    assert_eq!(controller_of(0), Some(40));
    assert_eq!(place("channel_2_pwm_speed"), (26, 9));
    assert_eq!(controller_of(26), Some(67));
    assert_eq!(place("channel_2_after_level"), (51, 9));
    assert_eq!(controller_of(51), Some(92));
}

/// S80-40, S80-46, SRV-68: an inline patch, told to the tracker, is
/// restated as the patch's 85 control changes, not as controllers moved.
#[test]
fn an_inline_patch_is_restated_whole() {
    let synth = Synth::of(SoundKind::Synth80).unwrap();
    let panel: Synth80Panel = serde_json::from_value(panel()).unwrap();
    let patch = panel_patch(&panel).unwrap();
    let wires: alloc::vec::Vec<Wire> = engine::patch_messages(&patch, 0)
        .map(|message| match message {
            Message::Channel(message) => Wire::Channel(message),
            Message::SystemExclusive(bytes) => Wire::Sysex(bytes.to_vec()),
        })
        .collect();
    let banks = Banks::none();
    let mut tracker = Tracker::new(synth, &banks);
    let hint = TrackHint::Synth80Patch(alloc::boxed::Box::new(patch));
    let last = wires.len() - 1;
    for (index, wire) in wires.iter().enumerate() {
        tracker.take(wire, (index == last).then_some(&hint), &banks);
    }
    assert_eq!(tracker.restate(0), wires);
}
