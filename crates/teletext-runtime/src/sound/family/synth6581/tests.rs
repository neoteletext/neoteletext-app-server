use alloc::vec::Vec;

use serde_json::{Value, json};

use super::*;
use crate::protocol::tests::base64;

/// Every named register of a full update (IDs 0 to 24, S6581-50).
const FULL: [&str; 25] = [
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

fn update(registers: &Value) -> Synth6581Event {
    serde_json::from_value(json!({"event": "register_update", "time_us": 0, "registers": registers})).unwrap()
}

/// SRV-12, S6581-50: a full update by named fields writes the engine's
/// register data message, which the same bytes given as `asid` match, and
/// both open a sound; one register alone does not.
#[test]
fn a_full_update_by_fields_and_by_bytes_is_one_message() {
    let registers: serde_json::Map<String, Value> = FULL.iter().zip(0_u8..).map(|(name, value)| ((*name).into(), json!(value.wrapping_mul(37)))).collect();
    let full = update(&Value::Object(registers));
    let Synth6581Event::RegisterUpdate(fields) = &full else { panic!("an update") };
    let by_fields = register_update(fields).unwrap();
    let [Wire::Sysex(after_start)] = by_fields.as_slice() else { panic!("one message") };
    assert!(takes(&by_fields[0]).is_ok());
    let mut bytes = alloc::vec![0xF0];
    bytes.extend_from_slice(after_start);
    assert_eq!(asid(&bytes).unwrap(), by_fields);
    let as_asid: Synth6581Event = serde_json::from_value(json!({"event": "asid", "time_us": 0, "bytes": base64(&bytes)})).unwrap();
    assert!(is_full_update(&full));
    assert!(is_full_update(&as_asid));
    assert!(!is_full_update(&update(&json!({"voice1_control": 0x41}))));
}

/// S6581-9, S6581-54: a command synth6581 does not take, and a message
/// without its `F7`, are refused.
#[test]
fn other_asid_messages_are_refused() {
    let refused: Vec<Rule> = [&[0xF0, 0x2D, 0x4F, 0xF7][..], &[0xF0, 0x2D, 0x4C][..]].iter().map(|bytes| asid(bytes).unwrap_err().rule).collect();
    assert_eq!(refused, [Rule::NotTaken, Rule::MalformedMidi]);
}
