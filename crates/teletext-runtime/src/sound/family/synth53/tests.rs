//! synth53's call site: named fields and bytes give one program, each
//! field and parameter at its number (SRV-37 to SRV-39).

use neoteletext_audio_instruments::synth53::interface::{self as engine, parameter};
use serde_json::{Value, json};

use super::{bank_dump, check_bank, number_of_parameter, program_of};
use crate::protocol::{SoundKind, Synth53Parameter, Synth53Patch};
use crate::sound::family::Synth;

/// Every field of a program at 0 but three, each in its parameter's range.
fn fields() -> Value {
    let mut fields = serde_json::Map::new();
    for name in [
        "osc_a_frequency",
        "osc_b_frequency",
        "osc_b_fine_tune",
        "osc_a_saw",
        "osc_a_square",
        "osc_b_saw",
        "osc_b_tri",
        "osc_b_square",
        "osc_a_pulse_width",
        "osc_b_pulse_width",
        "osc_sync",
        "osc_b_low_freq",
        "osc_b_keyboard",
        "glide_rate",
        "osc_a_level",
        "osc_b_level",
        "noise_level",
        "cutoff",
        "resonance",
        "filter_keyboard_track",
        "lfo_frequency",
        "lfo_initial_amount",
        "lfo_saw",
        "lfo_tri",
        "lfo_square",
        "lfo_source_mix",
        "lfo_freq_a",
        "lfo_freq_b",
        "lfo_freq_pw_a",
        "lfo_freq_pw_b",
        "lfo_filter",
        "poly_mod_filt_env_amount",
        "poly_mod_osc_b_amount",
        "poly_mod_freq_a",
        "poly_mod_pw",
        "poly_mod_filter",
        "vintage",
        "aftertouch_to_filter",
        "aftertouch_to_lfo",
        "env_filter_amount",
        "velocity_to_filter",
        "velocity_to_amp",
        "attack_filter",
        "attack_vca",
        "decay_filter",
        "decay_vca",
        "sustain_filter",
        "sustain_vca",
        "release_filter",
        "release_vca",
        "release_switch",
        "unison",
        "unison_voice_count",
        "unison_detune",
        "pitch_wheel_range",
        "retrigger_and_unison",
    ] {
        fields.insert(name.into(), json!(0));
    }
    fields.insert("unison_notes".into(), json!([0_u8; 10].to_vec()));
    fields.insert("cutoff".into(), json!(100));
    fields.insert("pitch_wheel_range".into(), json!(11));
    fields.insert("retrigger_and_unison".into(), json!(3));
    fields.insert("form".into(), json!("fields"));
    Value::Object(fields)
}

fn patch(value: Value) -> Synth53Patch {
    serde_json::from_value(value).unwrap()
}

fn synth(kind: SoundKind) -> Synth {
    Synth::of(kind).unwrap()
}

/// SRV-37, SRV-38, S53-38: a program given as fields and the same program
/// given as its edit buffer dump are one program, FILTER REV SELECT
/// written from the kind.
#[test]
fn fields_and_bytes_are_one_program() {
    for (kind, revision) in [(SoundKind::Synth53, 1), (SoundKind::Synth52, 0), (SoundKind::Synth51, 0)] {
        let from_fields = program_of(synth(kind), &patch(fields())).unwrap();
        assert_eq!(from_fields.get(parameter::CUTOFF), 100);
        assert_eq!(from_fields.get(parameter::PITCH_WHEEL_RANGE), 11);
        assert_eq!(from_fields.get(parameter::RETRIGGER_AND_UNISON), 3);
        assert_eq!(from_fields.get(parameter::FILTER_REV_SELECT), revision);
        let dump = engine::edit_buffer_dump(&from_fields);
        let from_bytes = program_of(synth(kind), &patch(json!({"form": "bytes", "bytes": crate::protocol::tests::base64(&dump)}))).unwrap();
        assert_eq!(from_bytes, from_fields);
    }
}

/// S53-33: a reserved byte of a dump is carried, never refused; a
/// parameter above its maximum is refused.
#[test]
fn reserved_bytes_are_carried() {
    let mut bytes = *engine::Program::default_for(engine::Variant::Synth53).bytes();
    bytes[100] = 0x55;
    let dump = engine::edit_buffer_dump(&engine::Program::from_bytes(bytes));
    let program = program_of(synth(SoundKind::Synth53), &patch(json!({"form": "bytes", "bytes": crate::protocol::tests::base64(&dump)}))).unwrap();
    assert_eq!(program.bytes()[100], 0x55);
    bytes[usize::from(parameter::CUTOFF)] = 121;
    let dump = engine::edit_buffer_dump(&engine::Program::from_bytes(bytes));
    assert!(program_of(synth(SoundKind::Synth53), &patch(json!({"form": "bytes", "bytes": crate::protocol::tests::base64(&dump)}))).is_err());
}

/// SRV-39: each named parameter's number, parameter 20 left out.
#[test]
fn parameters_have_their_numbers() {
    let numbered = |name: &str, value: u8| number_of_parameter(serde_json::from_value::<Synth53Parameter>(json!({name: value})).unwrap());
    assert_eq!(numbered("osc_a_frequency", 5), (0, 5));
    assert_eq!(numbered("filter_keyboard_track", 2), (19, 2));
    assert_eq!(numbered("lfo_frequency", 7), (21, 7));
    assert_eq!(numbered("unison_note_1", 1), (55, 1));
    assert_eq!(numbered("unison_note_10", 10), (64, 10));
    assert_eq!(numbered("pitch_wheel_range", 11), (86, 11));
    assert_eq!(numbered("retrigger_and_unison", 3), (87, 3));
}

/// SRV-63, S53-28: a program dump is lifted into its group's bank; an edit
/// buffer dump is not.
#[test]
fn program_dumps_are_bank_dumps() {
    let program = engine::Program::default_for(engine::Variant::Synth53);
    let dump = engine::program_dump(3, 7, &program).unwrap();
    assert_eq!(bank_dump(&dump[1..]), Ok(Some(3)));
    assert_eq!(bank_dump(&engine::edit_buffer_dump(&program)[1..]), Ok(None));
    assert!(check_bank(&dump).is_ok());
    let mut two = dump.to_vec();
    two.extend(engine::program_dump(4, 8, &program).unwrap());
    assert!(check_bank(&two).is_err());
}
