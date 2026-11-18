//! The synthesizers' sounds built and sent (neoteletext-common's
//! `docs/audio/server.md`, section 4): banks held and sent before the
//! sounds that name them, events placed into blocks, the records the
//! engine's encoder writes, `sound_built`, live sounds paced to the
//! receiver's ring, Standard MIDI Files, and every refusal leaving nothing
//! changed.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_audio::Codec;
use neoteletext_audio::encode::{Sound, packets};
use neoteletext_audio::record::{SYNTH8_KEYS_SPLIT, TYPE_DATA, TYPE_OBJECT};
use serde_json::{Value, json};

use super::{CONNECTION, Channel, Effect, answered, line, record_types, sent_to, told};
use crate::channel::SERVICE;
use crate::protocol::tests::base64;

/// The bank's handle.
const BANK: u8 = 1;
/// The sound's handle.
const HANDLE: u8 = 8;
/// A synth8 patch: 42 control values (S8-23).
const SYNTH8_PATCH: [u8; 42] = [0; 42];

fn message(kind: &str, mut fields: Value) -> Vec<u8> {
    fields["type"] = json!("sound");
    fields["kind"] = json!(kind);
    fields["connections"] = json!([CONNECTION]);
    line(&fields)
}

fn bank() -> Vec<u8> {
    message("synth8_bank", json!({"handle": BANK, "generation": 0, "bank": {"form": "bytes", "bytes": base64(&SYNTH8_PATCH)}}))
}

fn note(time_us: u64, channel: u8) -> Value {
    json!({"event": "note_on", "time_us": time_us, "channel": channel, "key": 60, "velocity": 100})
}

fn stored(events: &[Value], length: &Value) -> Vec<u8> {
    message(
        "synth8",
        json!({"handle": HANDLE, "generation": 0, "key_mode": "split", "split_key": 60, "banks": [BANK], "lift_banks": [], "length": length,
            "music": {"form": "events", "events": events}}),
    )
}

fn refusals(effects: &[Effect]) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == "refused").collect()
}

fn built(effects: &[Effect]) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == "sound_built").collect()
}

fn first_record(channel: &Channel) -> u8 {
    channel.connections[&CONNECTION].record_id
}

fn audio(records: &[Vec<u8>], first: u8) -> Vec<u8> {
    let (packets, _) = packets(SERVICE, first, records);
    packets.iter().flat_map(|packet| packet.bytes).collect()
}

/// The bank's records, as the engine's encoder writes them (SRV-60).
fn bank_records() -> Vec<Vec<u8>> {
    let bank = Sound::bank(BANK, 0, Codec::Synth8, &SYNTH8_PATCH).unwrap();
    let mut records = alloc::vec![bank.object_record().unwrap()];
    records.extend(bank.data_records());
    records
}

/// The stored sound's records for `blocks` (SRV-79).
fn sound_records(blocks: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut sound = Sound::events(HANDLE, 0, Codec::Synth8, blocks).unwrap();
    sound.banks = alloc::vec![BANK];
    sound.keys = Some([SYNTH8_KEYS_SPLIT, 60]);
    let mut records = alloc::vec![sound.object_record().unwrap()];
    records.extend(sound.data_records());
    records
}

/// SRV-61, SRV-79, SRV-47, SRV-7, SRV-70: a bank, then a stored sound
/// naming it: the bank's records go before the sound's OBJECT once, the
/// block opens with the restatement, the events land in the blocks with
/// channels 1 and 16 written 0 and 15, and
/// `sound_built` reports the blocks and bytes.
#[test]
fn a_stored_sound_goes_after_its_bank() {
    let mut channel = answered();
    let first = first_record(&channel);
    channel.from_program(&bank(), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    assert_eq!(sent_to(&effects, CONNECTION), audio(&bank_records(), first));
    let first = first_record(&channel);
    channel.from_program(&stored(&[note(0, 1), note(0, 16)], &json!({"tail_us": 0})), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    // The block opens with synth8's restatement at tick 0 (SRV-70, S8-29
    // (e)): the upper and lower halves' program changes, 0 on channels 1
    // and 2, as the engine's tracker writes them for a fresh sound.
    let block = alloc::vec![0x00, 0xC0, 0, 0x00, 0xC1, 0, 0x00, 0x90, 60, 100, 0x00, 0x9F, 60, 100];
    assert_eq!(sent_to(&effects, CONNECTION), audio(&sound_records(&[block]), first), "the bank is not sent twice");
    let built = built(&effects);
    assert_eq!(
        built,
        [json!({"type": "sound_built", "kind": "synth8", "handle": HANDLE, "generation": 0, "blocks": 1, "bytes": 14, "longest_block": 14,
        "banks_lifted": [], "dropped": {"not_taken": 0, "unmapped_channel": 0, "system_messages": 0, "escapes": 0, "meta_events": 0}})]
    );
}

/// SRV-9: a tail of 0 after an event at tick 199 is one block, at tick
/// 200 two; `length_us` of 20,001 is two blocks; a stored event at or past
/// the length is refused `beyond_length`.
#[test]
fn a_stored_sounds_blocks_follow_its_length() {
    for (events, length, blocks) in [
        (alloc::vec![note(19_900, 1)], json!({"tail_us": 0}), 1),
        (alloc::vec![note(20_000, 1)], json!({"tail_us": 0}), 2),
        (alloc::vec![], json!({"length_us": 20_001}), 2),
        (alloc::vec![], json!({"length_us": 1}), 1),
    ] {
        let mut channel = answered();
        channel.from_program(&bank(), 1);
        channel.from_program(&stored(&events, &length), 2);
        let effects = channel.take_effects();
        assert_eq!(refusals(&effects), [] as [Value; 0]);
        assert_eq!(built(&effects)[0]["blocks"], json!(blocks));
    }
    let mut channel = answered();
    channel.from_program(&bank(), 1);
    channel.from_program(&stored(&[note(20_000, 1)], &json!({"length_us": 20_000})), 2);
    let refused = refusals(&channel.take_effects());
    assert_eq!((refused[0]["rule"].as_str(), refused[0]["path"].as_str()), (Some("beyond_length"), Some("music.events[0]")));
}

/// SRV-61, SRV-46, SRV-48, SRV-49: a sound naming no bank the server holds,
/// or another family's, is refused naming the sound; a bank is never
/// played; a refusal changes nothing.
#[test]
fn banks_are_named_and_held() {
    let mut channel = answered();
    channel.from_program(&stored(&[note(0, 1)], &json!({"tail_us": 0})), 1);
    let refused = refusals(&channel.take_effects());
    assert_eq!(refused[0]["rule"], "unknown_bank");
    assert_eq!(refused[0]["sound"], json!({"kind": "synth8", "handle": HANDLE, "generation": 0}));
    channel.from_program(&bank(), 2);
    let synth7 = message(
        "synth7",
        json!({"handle": HANDLE, "generation": 0, "banks": [BANK], "lift_banks": [], "length": {"tail_us": 0}, "music": {"form": "events", "events": []}}),
    );
    channel.from_program(&synth7, 3);
    assert_eq!(refusals(&channel.take_effects())[0]["rule"], "wrong_family");
    let play = message(
        "play",
        json!({"channel": 0, "handle": BANK, "generation": 0, "gain": 100, "fade_ms": 0, "loop": false, "after": false, "duck": 0, "page": null}),
    );
    channel.from_program(&play, 4);
    let refused = refusals(&channel.take_effects());
    assert_eq!((refused[0]["rule"].as_str(), refused[0]["path"].as_str()), (Some("not_listed"), Some("handle")));
    channel.from_program(&stored(&[note(0, 1)], &json!({"tail_us": 0})), 5);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0], "the bank was held through the refusals");
}

fn live() -> Vec<u8> {
    message("synth8_live", json!({"handle": HANDLE, "generation": 0, "key_mode": "whole", "split_key": 0, "banks": [BANK]}))
}

fn live_events(until_us: u64, events: &[Value]) -> Vec<u8> {
    message("synth8_events", json!({"handle": HANDLE, "generation": 0, "until_us": until_us, "events": events}))
}

/// How many of `kinds` are `kind`.
fn bytecount_of(kinds: &[u8], kind: u8) -> usize {
    kinds.iter().fold(0_usize, |count, &each| count.saturating_add(usize::from(each == kind)))
}

fn data_records(bytes: &[u8]) -> usize {
    bytecount_of(&record_types(bytes), TYPE_DATA)
}

/// SRV-73, SRV-74, SRV-77: a live sound's events message writes the blocks
/// it completes; the server sends at most 25 blocks ahead of the live
/// sound's clock and the rest as their time comes.
#[test]
fn live_blocks_are_paced_to_the_ring() {
    let mut channel = answered();
    channel.from_program(&bank(), 1_000);
    channel.from_program(&live(), 1_000);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    assert_eq!(bytecount_of(&record_types(&sent_to(&effects, CONNECTION)), TYPE_OBJECT), 2, "the bank's OBJECT and the sound's");
    // 100 blocks completed at once: blocks 0 to 25 leave now.
    channel.from_program(&live_events(2_000_000, &[note(0, 1)]), 1_000);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    assert_eq!(data_records(&sent_to(&effects, CONNECTION)), 26);
    assert_eq!(channel.next_wake().map(|wake| wake <= 1_020), Some(true));
    // Half a second on, 25 more.
    channel.deadline(1_500);
    assert_eq!(data_records(&sent_to(&channel.take_effects(), CONNECTION)), 25);
    channel.deadline(10_000);
    assert_eq!(data_records(&sent_to(&channel.take_effects(), CONNECTION)), 49);
}

/// SRV-72, SRV-74, SRV-75: events for no live sound, `until_us` moved back
/// or too far, and an event in a block already written are refused, and a
/// refused message changes nothing.
#[test]
fn live_events_are_refused_as_they_must_be() {
    let mut channel = answered();
    channel.from_program(&live_events(20_000, &[]), 1);
    assert_eq!(refusals(&channel.take_effects())[0]["rule"], "unknown_live_sound");
    channel.from_program(&bank(), 1);
    channel.from_program(&live(), 1);
    channel.from_program(&live_events(2_000_001, &[]), 1);
    assert_eq!(refusals(&channel.take_effects())[0]["rule"], "advance_too_far");
    channel.from_program(&live_events(100_001, &[]), 1);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0]);
    channel.from_program(&live_events(100_000, &[]), 1);
    assert_eq!(refusals(&channel.take_effects())[0]["rule"], "until_moved_back");
    channel.from_program(&live_events(120_001, &[note(99_000, 1)]), 1);
    let refused = refusals(&channel.take_effects());
    assert_eq!((refused[0]["rule"].as_str(), refused[0]["path"].as_str()), (Some("late_event"), Some("events[0]")));
    channel.from_program(&live_events(120_001, &[note(100_000, 1)]), 1);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0], "the refused message changed nothing");
}

/// A format 0 file of `track`'s events, 96 ticks a quarter note.
fn file(track: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::from(*b"MThd");
    bytes.extend_from_slice(&[0, 0, 0, 6, 0, 0, 0, 1, 0, 96]);
    bytes.extend_from_slice(b"MTrk");
    bytes.extend_from_slice(&u32::try_from(track.len()).unwrap().to_be_bytes());
    bytes.extend_from_slice(track);
    bytes
}

fn file_sound(bytes: &[u8], map: &Value) -> Vec<u8> {
    message(
        "synth8",
        json!({"handle": HANDLE, "generation": 0, "key_mode": "whole", "split_key": 0, "banks": [BANK], "lift_banks": [], "length": {"tail_us": 0},
            "music": {"form": "midi_file", "bytes": base64(bytes), "channel_map": map}}),
    )
}

/// SRV-81 to SRV-86: a format 0 file at the default tempo, its quarter
/// note 500 milliseconds, its text meta event and its unmapped channel's
/// note dropped and counted; format 2 refused.
#[test]
fn a_standard_midi_file_is_read() {
    // A note on, a text meta event, a note on of channel 2, and at 96 ticks
    // (500 ms, tick 5,000 of 0.1 ms) a note off by running status, then the
    // end of track.
    let track = [0x00, 0x90, 60, 100, 0x00, 0xFF, 0x01, 0x01, b'x', 0x00, 0x91, 60, 100, 0x60, 0x90, 60, 0, 0x00, 0xFF, 0x2F, 0x00];
    let mut map: Vec<Value> = alloc::vec![Value::Null; 16];
    map[0] = json!(1);
    let mut channel = answered();
    channel.from_program(&bank(), 1);
    channel.from_program(&file_sound(&file(&track), &json!(map)), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    let built = &built(&effects)[0];
    assert_eq!(built["blocks"], 26);
    assert_eq!(built["dropped"], json!({"not_taken": 0, "unmapped_channel": 1, "system_messages": 0, "escapes": 0, "meta_events": 1}));
    let mut format_2 = file(&track);
    format_2[9] = 2;
    channel.from_program(&file_sound(&format_2, &Value::Null), 3);
    assert_eq!(refusals(&channel.take_effects())[0]["rule"], "midi_file_unsupported");
}

/// The live synth8 sound the fixture's `named-parameter` example adds to:
/// the fixture gives its events message alone, so the test announces the
/// sound first, with a bank (SRV-58, SRV-72).
const NAMED_PARAMETER_LIVE: u8 = 50;

/// SRV-94: the shared fixture `docs/audio/examples.jsonl`, every line
/// accepted and built, each example's lines in order on one channel.
#[test]
fn the_shared_examples_are_all_built() {
    let mut channel = answered();
    for (now, line) in (1..).zip(crate::protocol::tests::synth::examples()) {
        let value: Value = serde_json::from_str(&line).unwrap();
        if value["example"] == "named-parameter" {
            channel.from_program(&bank(), now);
            let live = json!({"handle": NAMED_PARAMETER_LIVE, "generation": 0, "key_mode": "whole", "split_key": 0, "banks": [BANK]});
            channel.from_program(&message("synth8_live", live), now);
        }
        channel.from_program(&serde_json::to_vec(&value["message"]).unwrap(), now);
        let refused: Vec<String> = refusals(&channel.take_effects()).iter().map(ToString::to_string).collect();
        assert_eq!(refused, [] as [String; 0], "{}", value["example"]);
    }
}

/// SRV-32, MIDI-57: synth106's and synth60's patches differ in size, so a
/// synth60 sound naming a synth106 bank is refused, and a synth106 sound
/// naming it is not.
#[test]
fn a_bank_is_read_only_by_its_own_model() {
    let mut channel = answered();
    let panel = json!({"form": "fields", "lfo_rate": 40, "lfo_delay": 0, "dco_lfo": 0, "dco_pwm": 0, "noise_level": 0, "vcf_frequency": 52,
        "vcf_resonance": 18, "vcf_envelope": 70, "vcf_lfo": 0, "vcf_keyboard": 64, "vca_level": 90, "attack": 22, "decay": 60, "sustain": 80,
        "release": 30, "sub_level": 40, "range": "feet_8", "pulse": false, "sawtooth": true, "chorus": "one", "pwm": "manual",
        "envelope_polarity": "positive", "vca": "envelope", "hpf": 0});
    channel.from_program(&message("synth106_bank", json!({"handle": BANK, "generation": 0, "bank": {"form": "patches", "patches": [panel]}})), 1);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0]);
    channel.from_program(&message("synth60_live", json!({"handle": HANDLE, "generation": 0, "banks": [BANK]})), 2);
    let refused = refusals(&channel.take_effects());
    assert_eq!(refused.iter().map(|refused| refused["rule"].clone()).collect::<Vec<_>>(), [json!("wrong_family")]);
    channel.from_program(&message("synth106_live", json!({"handle": HANDLE, "generation": 0, "banks": [BANK]})), 3);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0]);
}
