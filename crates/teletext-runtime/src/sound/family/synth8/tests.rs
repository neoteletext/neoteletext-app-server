use alloc::vec::Vec;

use neoteletext_audio_instruments::synth8::interface::{PATCH_BYTES, Param, Patch};
use serde_json::{Value, json};

use super::*;
use crate::protocol::SoundKind;
use crate::protocol::tests::base64;
use crate::protocol::{Synth8Bank, Synth8ParameterEvent, Synth8PatchEvent};

fn synth() -> Synth {
    Synth::of(SoundKind::Synth8).unwrap()
}

/// The default patch's fields (S8-22), named as the engine names its
/// controls, which SRV-40's names are.
fn default_fields() -> Value {
    let mut map = serde_json::Map::new();
    map.insert("form".into(), json!("fields"));
    for param in Param::ALL {
        map.insert(param.name().into(), json!(Patch::DEFAULT.get(param)));
    }
    Value::Object(map)
}

fn patch_event(channel: u8, patch: &Value) -> Synth8PatchEvent {
    serde_json::from_value(json!({"time_us": 0, "channel": channel, "patch": patch})).unwrap()
}

/// SRV-40, SRV-42, S8-27: fields and bytes write the same 42 control
/// changes, in byte order, each the engine's controller of its control.
#[test]
fn fields_and_bytes_write_the_same_inline_patch() {
    let from_fields = Synth8Family::inline_patch(synth(), &patch_event(2, &default_fields())).unwrap().0;
    let from_bytes = Synth8Family::inline_patch(synth(), &patch_event(2, &json!({"form": "bytes", "bytes": base64(&Patch::DEFAULT.to_bytes())}))).unwrap().0;
    assert_eq!(from_fields, from_bytes);
    let expected: Vec<Wire> = Param::ALL
        .into_iter()
        .zip(Patch::DEFAULT.to_bytes())
        .map(|(param, value)| Wire::Channel(ChannelMessage::new(status::CONTROL_CHANGE, 1, param.controller(), value)))
        .collect();
    assert_eq!(from_fields, expected);
    assert_eq!(from_fields.len(), PATCH_BYTES);
}

/// S8-2, SRV-42: a panel control on channel 3 is not taken.
#[test]
fn a_panel_control_off_channels_1_and_2_is_not_taken() {
    let wires = Synth8Family::inline_patch(synth(), &patch_event(3, &default_fields())).unwrap().0;
    let refusal = takes(&wires[0]).unwrap_err();
    assert!(refusal.starts_with("S8-2"), "{refusal}");
    let upper = Synth8Family::inline_patch(synth(), &patch_event(1, &default_fields())).unwrap().0;
    assert!(upper.iter().all(|wire| takes(wire).is_ok()));
}

/// SRV-41: 43 bytes are refused.
#[test]
fn a_patch_of_43_bytes_is_refused() {
    let fault = Synth8Family::inline_patch(synth(), &patch_event(1, &json!({"form": "bytes", "bytes": base64(&[0; 43])}))).unwrap_err();
    assert_eq!(fault.rule, Rule::InvalidPatch);
}

/// SRV-42: a named parameter is its controller's control change on the
/// event's channel.
#[test]
fn a_named_parameter_writes_its_controller() {
    let event: Synth8ParameterEvent = serde_json::from_value(json!({"time_us": 0, "channel": 2, "set": {"vcf_cutoff": 90}})).unwrap();
    assert_eq!(
        Synth8Family::parameter(synth(), &event).unwrap(),
        [Wire::Channel(ChannelMessage::new(status::CONTROL_CHANGE, 1, Param::VcfCutoff.controller(), 90))]
    );
}

/// SRV-41, S8-29 (a): a bank of patches and of bytes is the same image; a
/// length that is not a multiple of 42 is refused.
#[test]
fn a_bank_is_built_and_checked_by_the_engine() {
    let bank = |value: Value| -> Synth8Bank { serde_json::from_value(value).unwrap() };
    let image = Synth8Family::bank_image(synth(), &bank(json!({"form": "patches", "patches": [default_fields()]}))).unwrap();
    assert_eq!(image, Patch::DEFAULT.to_bytes());
    assert_eq!(Synth8Family::bank_image(synth(), &bank(json!({"form": "bytes", "bytes": base64(&image)}))).unwrap(), image);
    let fault = Synth8Family::bank_image(synth(), &bank(json!({"form": "bytes", "bytes": base64(&image[..41])}))).unwrap_err();
    assert_eq!(fault.rule, Rule::InvalidBank);
}
