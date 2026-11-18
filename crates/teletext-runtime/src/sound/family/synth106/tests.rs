extern crate std;

use alloc::vec::Vec;

use neoteletext_audio::midi::Banks;
use neoteletext_audio_instruments::synth106::interface::{self as engine, Exclusive, Model, Range, Tracker};
use serde_json::{Value, json};

use super::{FamilyCalls, switch_message};
use crate::protocol::tests::base64;
use crate::protocol::{SoundKind, Synth60Family, Synth60ParameterEvent, Synth106Family, Synth106ParameterEvent, Synth106PatchEvent};
use crate::sound::family::Synth;
use crate::sound::wire::Wire;

/// A synth106 panel with every field set away from the default.
fn panel() -> Value {
    json!({"form": "fields", "lfo_rate": 40, "lfo_delay": 1, "dco_lfo": 2, "dco_pwm": 3, "noise_level": 4, "vcf_frequency": 52,
        "vcf_resonance": 18, "vcf_envelope": 70, "vcf_lfo": 5, "vcf_keyboard": 64, "vca_level": 90, "attack": 22, "decay": 60,
        "sustain": 80, "release": 30, "sub_level": 40, "range": "feet_4", "pulse": true, "sawtooth": false, "chorus": "one",
        "pwm": "lfo", "envelope_polarity": "negative", "vca": "gate", "hpf": 2})
}

fn synth(kind: SoundKind) -> Synth {
    Synth::of(kind).unwrap()
}

fn patch_event(patch: &Value) -> Synth106PatchEvent {
    serde_json::from_value(json!({"time_us": 0, "channel": 3, "patch": patch})).unwrap()
}

/// The whole message, `F0` first, of a system exclusive wire.
fn whole(wire: &Wire) -> Vec<u8> {
    let Wire::Sysex(bytes) = wire else { panic!("not system exclusive: {wire:?}") };
    core::iter::once(0xF0).chain(bytes.iter().copied()).collect()
}

/// SRV-30, SRV-32: a patch given as fields is the engine's packed message
/// on its channel, and the same bytes given as the bytes form are
/// carried unchanged.
#[test]
fn fields_and_bytes_forms_write_the_same_message() {
    let synth106 = synth(SoundKind::Synth106);
    let (fields, hint) = Synth106Family::inline_patch(synth106, &patch_event(&panel())).unwrap();
    assert_eq!(hint, None);
    assert_eq!(fields.len(), 1);
    let message = whole(&fields[0]);
    assert_eq!(message.len(), engine::PATCH_MESSAGE_BYTES);
    // Byte d the channel less one (SRV-33), and the patch reads back.
    assert_eq!(message[3], 2);
    let patch = engine::check_patch(Model::Synth106, &message).unwrap();
    assert_eq!((patch.range, patch.pulse, patch.sawtooth, patch.hpf), (Range::Four, true, false, 2));
    let (bytes, _) = Synth106Family::inline_patch(synth106, &patch_event(&json!({"form": "bytes", "bytes": base64(&message)}))).unwrap();
    assert_eq!(bytes, fields);
}

/// SRV-32: a bytes patch the instrument would ignore is refused.
#[test]
fn a_faulty_bytes_patch_is_refused() {
    let mut message = whole(&Synth106Family::inline_patch(synth(SoundKind::Synth106), &patch_event(&panel())).unwrap().0[0]);
    message[1] = 0x43;
    let refused = Synth106Family::inline_patch(synth(SoundKind::Synth106), &patch_event(&json!({"form": "bytes", "bytes": base64(&message)}))).unwrap_err();
    assert_eq!(refused.rule, crate::protocol::Rule::InvalidPatch);
    assert!(refused.detail.starts_with("SRV-32"), "{}", refused.detail);
}

/// SRV-33 item 1, SRV-68: a switch parameter is written from the panel
/// the tracker holds at its tick, the one field changed.
#[test]
fn a_switch_parameter_follows_the_tracked_panel() {
    let synth106 = synth(SoundKind::Synth106);
    let mut tracker = Tracker::new(Model::Synth106);
    let (patch, _) = Synth106Family::inline_patch(synth106, &patch_event(&panel())).unwrap();
    let _taken = tracker.take(&patch[0].message(), &Banks::none());
    let event: Synth106ParameterEvent = serde_json::from_value(json!({"time_us": 0, "channel": 1, "set": {"sawtooth": true}})).unwrap();
    let written = Synth106Family::parameter(synth106, &event).unwrap();
    let [Wire::Deferred(super::Deferred::Synth106Switch { channel, parameter })] = written.as_slice() else { panic!("{written:?}") };
    let resolved = switch_message(Some(&tracker), *channel, *parameter).unwrap();
    let message = whole(&resolved[0]);
    let Ok(Exclusive::Parameter { number, value, .. }) = engine::read_message(&message) else { panic!("{message:?}") };
    assert_eq!(number, 16);
    let mut after = *tracker.panel();
    after.set_parameter(number, value).unwrap();
    // The sawtooth changed; the range and pulse of the patch are kept.
    assert_eq!((after.range, after.pulse, after.sawtooth), (Range::Four, true, true));
    assert!(switch_message(None, 0, *parameter).is_err());
}

/// SRV-33 item 1: a slider is one function type `32` message.
#[test]
fn a_slider_parameter_is_one_parameter_message() {
    let event: Synth106ParameterEvent = serde_json::from_value(json!({"time_us": 0, "channel": 16, "set": {"release": 99}})).unwrap();
    let written = Synth106Family::parameter(synth(SoundKind::Synth106), &event).unwrap();
    assert_eq!(written.iter().map(whole).collect::<Vec<_>>(), [engine::parameter_message(15, 14, 99).to_vec()]);
}

/// SRV-31, SRV-33 item 2: synth60's switches are the JU-06A controllers at
/// canonical values, and its patch is 24 bytes and three control changes.
#[test]
fn synth60_switches_are_control_changes() {
    let synth60 = synth(SoundKind::Synth60);
    for (set, controller, value) in
        [(json!({"chorus": "both"}), 93, 96), (json!({"pwm": "envelope"}), 15, 86), (json!({"pwm": "manual"}), 15, 43), (json!({"sub_switch": true}), 28, 64)]
    {
        let event: Synth60ParameterEvent = serde_json::from_value(json!({"time_us": 0, "channel": 2, "set": set})).unwrap();
        let written = Synth60Family::parameter(synth60, &event).unwrap();
        let [Wire::Channel(message)] = written.as_slice() else { panic!("{written:?}") };
        assert_eq!((message.status, message.data1, message.data2), (0xB1, controller, value), "{set}");
    }
    let mut panel = panel();
    panel["chorus"] = json!("both");
    panel["pwm"] = json!("envelope");
    panel["sub_switch"] = json!(true);
    let event = serde_json::from_value(json!({"time_us": 0, "channel": 1, "patch": panel})).unwrap();
    let (wires, _) = Synth60Family::inline_patch(synth60, &event).unwrap();
    assert_eq!(wires.len(), 4);
}

/// SRV-32, SRV-60: a bank built from patches is checked by the engine's
/// bank reader, and its bytes given back are accepted as they are.
#[test]
fn a_bank_of_fields_is_built_and_read_back() {
    let synth106 = synth(SoundKind::Synth106);
    let bank = serde_json::from_value(json!({"form": "patches", "patches": [panel(), panel()]})).unwrap();
    let image = Synth106Family::bank_image(synth106, &bank).unwrap();
    assert_eq!(image.len(), 2 * engine::PATCH_MESSAGE_BYTES);
    assert_eq!(engine::check_bank(Model::Synth106, &image), Ok(2));
    let bytes = serde_json::from_value(json!({"form": "bytes", "bytes": base64(&image)})).unwrap();
    assert_eq!(Synth106Family::bank_image(synth106, &bytes).unwrap(), image);
}
