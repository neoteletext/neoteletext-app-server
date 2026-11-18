//! The synthesizers' messages of program protocol 13 (neoteletext-common's
//! `docs/audio/server.md`): every kind read whole, every fault of their
//! shape refused with its path and rule (SRV-3 to SRV-24, SRV-49, SRV-51),
//! and the shared fixture `docs/audio/examples.jsonl` read.

use alloc::string::String;
use alloc::vec::Vec;

use serde_json::{Value, json};

use super::sound::sound;
use super::{accepted, base64, refused, with, without};
use crate::protocol::{Rule, SoundKind};

fn note(time_us: u64, channel: u8) -> Value {
    json!({"event": "note_on", "time_us": time_us, "channel": channel, "key": 60, "velocity": 100})
}

fn events(list: &[Value]) -> Value {
    json!({"form": "events", "events": list})
}

fn stored(kind: &str) -> Value {
    sound(kind, json!({"handle": 8, "generation": 0, "banks": [1], "lift_banks": [], "length": {"tail_us": 0}, "music": events(&[note(0, 1)])}))
}

fn synth6581(kind: &str) -> Value {
    let registers: serde_json::Map<String, Value> = (0..25).map(|id| (REGISTERS[id].into(), json!(0))).collect();
    let update = json!({"event": "register_update", "time_us": 0, "registers": registers});
    sound(kind, json!({"handle": 8, "generation": 0, "clock": "pal", "length": {"length_us": 20_000}, "music": events(&[update])}))
}

/// The first 25 ASID register IDs' names (SRV-12).
const REGISTERS: [&str; 25] = [
    "voice1_frequency_low",
    "voice1_frequency_high",
    "voice1_pulse_width_low",
    "voice1_pulse_width_high",
    "voice1_attack_decay",
    "voice1_sustain_release",
    "voice2_frequency_low",
    "voice2_frequency_high",
    "voice2_pulse_width_low",
    "voice2_pulse_width_high",
    "voice2_attack_decay",
    "voice2_sustain_release",
    "voice3_frequency_low",
    "voice3_frequency_high",
    "voice3_pulse_width_low",
    "voice3_pulse_width_high",
    "voice3_attack_decay",
    "voice3_sustain_release",
    "filter_cutoff_low",
    "filter_cutoff_high",
    "filter_resonance_routing",
    "filter_mode_volume",
    "voice1_control",
    "voice2_control",
    "voice3_control",
];

/// The names whose family has banks.
const BANKED: [&str; 9] = ["synth7", "synth106", "synth60", "synthd", "synth53", "synth52", "synth51", "synth8", "synth80"];

fn keyboard(kind: &str, value: Value) -> Value {
    if kind.starts_with("synth8_") || kind == "synth8" { with(&with(&value, "/key_mode", json!("split")), "/split_key", json!(60)) } else { value }
}

/// One message of each of the 42 synthesizer kinds (SRV-3).
fn every() -> Vec<Value> {
    let mut every = Vec::new();
    for name in ["synth6581", "synth8580"] {
        every.push(synth6581(name));
        every.push(sound(&alloc::format!("{name}_live"), json!({"handle": 8, "generation": 0, "clock": "ntsc"})));
        every.push(sound(&alloc::format!("{name}_events"), json!({"handle": 8, "generation": 0, "until_us": 20_000, "events": []})));
    }
    for name in BANKED {
        every.push(keyboard(name, stored(name)));
        every.push(keyboard(name, sound(&alloc::format!("{name}_live"), json!({"handle": 8, "generation": 0, "banks": [1]}))));
        every.push(sound(&alloc::format!("{name}_events"), json!({"handle": 8, "generation": 0, "until_us": 20_001, "events": [note(20_000, 16)]})));
        let bank = json!({"form": "bytes", "bytes": base64(&[0; 42])});
        every.push(sound(&alloc::format!("{name}_bank"), json!({"handle": 1, "generation": 0, "bank": bank})));
    }
    every
}

/// SRV-3: 42 synthesizer kinds, each read whole; an unknown field and a
/// missing one refused with their paths.
#[test]
fn every_synthesizer_kind_is_read_whole() {
    let every = every();
    assert_eq!(every.len(), 42);
    for message in &every {
        accepted(message);
        refused(&with(message, "/extra", json!(1)), "extra", Rule::UnknownField);
        refused(&without(message, "/handle"), "handle", Rule::MissingField);
    }
}

/// SRV-3, SRV-4: protocol 12's fields and `_blocks` kinds are refused.
#[test]
fn protocol_twelve_is_refused() {
    for field in ["slots", "images_per_block", "first", "model", "chip"] {
        refused(&with(&stored("synth7"), &alloc::format!("/{field}"), json!(1)), field, Rule::UnknownField);
    }
    refused(&sound("synth7_blocks", json!({"handle": 1, "generation": 0})), "kind", Rule::NotListed);
}

/// SRV-7, SRV-49: a channel is 1 to 16, refused with its path inside the
/// tagged music and event.
#[test]
fn a_channel_is_one_to_sixteen() {
    for channel in [1, 16] {
        accepted(&with(&stored("synth7"), "/music/events/0/channel", json!(channel)));
    }
    for channel in [0, 17] {
        refused(&with(&stored("synth7"), "/music/events/0/channel", json!(channel)), "music.events[0].channel", Rule::OutOfRange);
    }
    refused(&with(&stored("synth7"), "/music/events/0/event", json!("note")), "music.events[0].event", Rule::NotListed);
    refused(&with(&stored("synth7"), "/music/form", json!("tune")), "music.form", Rule::NotListed);
    refused(&without(&stored("synth7"), "/music/events/0/key"), "music.events[0].key", Rule::MissingField);
}

/// SRV-8, SRV-14, SRV-18, SRV-74: times in order, a stored sound's below
/// two minutes, a live message's before its `until_us`.
#[test]
fn times_are_in_order_and_bounded() {
    let backwards = with(&stored("synth106"), "/music/events", json!([note(20_000, 1), note(19_999, 1)]));
    refused(&backwards, "music.events[1]", Rule::NotAscending);
    accepted(&with(&stored("synth106"), "/music/events", json!([note(20_000, 1), note(20_000, 1)])));
    refused(&with(&stored("synth106"), "/music/events/0/time_us", json!(120_000_000)), "music.events[0].time_us", Rule::OutOfRange);
    let live = sound("synth106_events", json!({"handle": 8, "generation": 0, "until_us": 20_000, "events": [note(20_000, 1)]}));
    refused(&live, "events[0].time_us", Rule::NotBeforeUntil);
    refused(&with(&live, "/until_us", json!(0)), "until_us", Rule::OutOfRange);
}

/// SRV-9: a stored sound's length is one of its two forms, each bounded.
#[test]
fn a_length_is_bounded() {
    refused(&with(&stored("synth8"), "/length", json!({"length_us": 0})), "length.length_us", Rule::OutOfRange);
    refused(&with(&stored("synth8"), "/length", json!({"length_us": 120_000_001})), "length.length_us", Rule::OutOfRange);
    accepted(&keyboard("synth8", with(&stored("synth8"), "/length", json!({"length_us": 120_000_000}))));
}

/// SRV-16, SRV-17: a sound names at least one bank.
#[test]
fn a_sound_names_a_bank() {
    refused(&with(&stored("synth80"), "/banks", json!([])), "banks", Rule::TooShort);
    refused(&sound("synth80_live", json!({"handle": 8, "generation": 0, "banks": []})), "banks", Rule::TooShort);
    accepted(&with(&with(&stored("synth80"), "/banks", json!([])), "/lift_banks", json!([{"handle": 2, "generation": 0}])));
}

/// SRV-12: a register update writes at least one register.
#[test]
fn a_register_update_writes_a_register() {
    let empty = json!({"event": "register_update", "time_us": 0, "registers": {}});
    refused(&with(&synth6581("synth6581"), "/music/events/0", empty), "music.events[0].registers", Rule::TooShort);
    let wide = with(&synth6581("synth6581"), "/music/events/0/registers/voice1_control", json!(256));
    refused(&wide, "music.events[0].registers.voice1_control", Rule::WrongType);
}

/// SRV-26, SRV-28: a voice's fields are ranged, its name printable, and a
/// parameter's value within its own range.
#[test]
fn a_synth7_voice_is_ranged() {
    let operator = json!({"eg_rates": [99, 99, 99, 99], "eg_levels": [99, 99, 99, 0], "break_point": 39, "left_depth": 0, "right_depth": 0,
        "left_curve": 0, "right_curve": 0, "rate_scaling": 0, "amplitude_sensitivity": 0, "velocity_sensitivity": 0, "output_level": 99,
        "oscillator_mode": 0, "frequency_coarse": 1, "frequency_fine": 0, "detune": 7});
    let voice = json!({"form": "fields", "operators": [operator, operator, operator, operator, operator, operator],
        "pitch_eg_rates": [99, 99, 99, 99], "pitch_eg_levels": [50, 50, 50, 50], "algorithm": 0, "feedback": 0, "oscillator_sync": 1,
        "lfo_speed": 35, "lfo_delay": 0, "lfo_pitch_depth": 0, "lfo_amplitude_depth": 0, "lfo_sync": 1, "lfo_wave": 0,
        "pitch_sensitivity": 3, "transpose": 24, "name": "INIT VOICE"});
    let bank = sound("synth7_bank", json!({"handle": 1, "generation": 0, "bank": {"form": "patches", "patches": [voice]}}));
    accepted(&bank);
    refused(&with(&bank, "/bank/patches/0/operators/2/detune", json!(15)), "bank.patches[0].operators[2].detune", Rule::OutOfRange);
    refused(&with(&bank, "/bank/patches/0/name", json!("TOO LONG NAME")), "bank.patches[0].name", Rule::TooLong);
    refused(&with(&bank, "/bank/patches/0/name", json!("\u{7F}")), "bank.patches[0].name", Rule::ForbiddenCharacter);
    let parameter = |set: Value| {
        sound("synth7_events", json!({"handle": 8, "generation": 0, "until_us": 1, "events": [{"event": "parameter", "time_us": 0, "channel": 1, "set": set}]}))
    };
    accepted(&parameter(json!({"op3_output_level": 99})));
    refused(&parameter(json!({"op3_output_level": 100})), "events[0].set.op3_output_level", Rule::OutOfRange);
    refused(&parameter(json!({"op7_output_level": 1})), "events[0].set", Rule::NotListed);
}

/// SRV-48: a refused sound names its kind, handle and generation where the
/// line holds them, and nothing when its kind is not listed.
#[test]
fn a_refused_sound_is_named() {
    use crate::protocol::sound_of;
    let named = sound_of(&super::line(&with(&stored("synth7"), "/music/events/0/key", json!(128)))).unwrap();
    assert_eq!((named.kind, named.handle, named.generation), (SoundKind::Synth7, Some(8), Some(0)));
    let stop = sound_of(&super::line(&sound("stop", json!({"channel": 9})))).unwrap();
    assert_eq!((stop.kind, stop.handle, stop.generation), (SoundKind::Stop, None, None));
    assert_eq!(sound_of(&super::line(&sound("synth7_blocks", json!({})))), None);
    assert_eq!(sound_of(b"[1]"), None);
}

/// SRV-94's shared fixture: every message of neoteletext-common's
/// `docs/audio/examples.jsonl` is read whole by the protocol's reader. The
/// file is read from the neoteletext-common checkout beside this one, or
/// from `NEOTELETEXT_COMMON`.
#[test]
#[ignore = "needs a neoteletext-common checkout at ~/devel/neoteletext-common or NEOTELETEXT_COMMON, holding docs/audio/examples.jsonl"]
fn the_shared_examples_are_read() {
    for line in examples() {
        let value: Value = serde_json::from_str(&line).unwrap();
        accepted(&value["message"]);
    }
}

/// The lines of the shared fixture.
pub(crate) fn examples() -> Vec<String> {
    extern crate std;
    let common = std::env::var("NEOTELETEXT_COMMON").unwrap_or_else(|_| alloc::format!("{}/devel/neoteletext-common", std::env::var("HOME").unwrap()));
    let text = std::fs::read_to_string(alloc::format!("{common}/docs/audio/examples.jsonl")).unwrap();
    text.lines().filter(|line| !line.is_empty()).map(String::from).collect()
}
