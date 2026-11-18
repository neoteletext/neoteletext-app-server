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

/// The bank of `kind` among the shared examples, as a bank of `as_kind` at
/// `handle` for the test's connection.
fn example_bank(kind: &str, as_kind: &str, handle: u8) -> Vec<u8> {
    let found = crate::protocol::tests::synth::examples()
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["message"].clone())
        .find(|message| message["kind"] == kind)
        .unwrap_or_else(|| panic!("no example of {kind}"));
    let mut bank = found;
    bank["kind"] = json!(as_kind);
    bank["handle"] = json!(handle);
    bank["connections"] = json!([CONNECTION]);
    // synth60's panel has one switch synth106's has not (S106-22).
    if as_kind == "synth60_bank" {
        bank["bank"]["patches"][0]["sub_switch"] = json!(true);
    }
    line(&bank)
}

/// The local sound's bank's handle, and its own, as RULE-61's example has
/// them.
const LOCAL_BANK: u8 = 2;
const LOCAL: u8 = 1;

/// RULE-61's example: a local synth106 sound whose setup is program
/// change 0.
fn local_synth106(setup: &Value) -> Vec<u8> {
    message("synth106_local", json!({"handle": LOCAL, "generation": 1, "banks": [LOCAL_BANK], "setup": setup}))
}

fn program_change(time_us: u64) -> Value {
    json!({"event": "program_change", "channel": 1, "program": 0, "time_us": time_us})
}

fn play(page: &Value, looped: bool, after: bool) -> Vec<u8> {
    message("play", json!({"channel": 0, "handle": LOCAL, "generation": 1, "gain": 100, "fade_ms": 5, "duck": 0, "loop": looped, "after": after, "page": page}))
}

/// REQ-6532, RULE-60, RULE-61, RULE-85: a local sound is announced as an
/// OBJECT that says it is local (tag 9), byte for byte the engine's wire
/// vector of RULE-61's example, and its one block, the setup; nothing
/// more of it is ever sent.
#[test]
fn a_local_sound_is_its_object_and_its_setup() {
    let mut channel = answered();
    channel.from_program(&example_bank("synth106_bank", "synth106_bank", LOCAL_BANK), 1);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0]);
    let first = first_record(&channel);
    channel.from_program(&local_synth106(&json!([program_change(0)])), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    // The setup block: program change 0 on channel 1 at tick 0 (MIDI-23).
    let setup = [0x00, 0xC0, 0x00];
    let mut sound = Sound::local(LOCAL, 1, Codec::Synth106, &setup).unwrap();
    sound.banks = alloc::vec![LOCAL_BANK];
    let row = neoteletext_web::wire_vectors::vectors().find(|row| row.kind == "object" && row.name == "local_sound").unwrap();
    let (vector, length) = neoteletext_web::wire_vectors::bytes_of(row.value).unwrap();
    assert_eq!(sound.object_record().unwrap(), vector[..length], "the engine's vector of RULE-61's example, which names no variant");
    // The server names the variant of every sound of the synth106 family
    // (MIDI-48 tag 5): 0, synth106, which is also what no tag means.
    sound.variant = Some(0);
    let object = sound.object_record().unwrap();
    assert_eq!(object.len(), length + 3, "the vector and the variant's tag, length and value");
    // RULE-85: and with the variant named it is the shared row of what the
    // server writes, byte for byte.
    let row = neoteletext_web::wire_vectors::vectors().find(|row| row.kind == "object" && row.name == "local_sound_as_the_server_writes_it").unwrap();
    let (vector, length) = neoteletext_web::wire_vectors::bytes_of(row.value).unwrap();
    assert_eq!(object, vector[..length], "the shared vector of the OBJECT as the server writes it");
    assert!(object.windows(3).any(|tag| tag == [neoteletext_audio::record::TAG_LOCAL, 1, 1]), "tag 9, one byte, 1");
    let mut records = alloc::vec![object];
    records.extend(sound.data_records());
    assert_eq!(records.len(), 2, "the OBJECT and the one DATA record of its setup");
    assert_eq!(sent_to(&effects, CONNECTION), audio(&records, first));
    assert_eq!(built(&effects), [] as [Value; 0], "it is no stored music: nothing was built from events");
    channel.deadline(60_000);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), [] as [u8; 0], "no blocks follow");
}

/// REQ-6532, RULE-61: each family that plays notes has its local sound,
/// announced with tag 9 and its family's codec; the synth6581 family,
/// which takes register writes, has none.
#[test]
fn every_note_family_has_a_local_sound() {
    let families = [
        ("synth7_local", "synth7_bank", "synth7_bank", Codec::Synth7, json!({})),
        ("synth106_local", "synth106_bank", "synth106_bank", Codec::Synth106, json!({})),
        ("synth60_local", "synth106_bank", "synth60_bank", Codec::Synth106, json!({})),
        ("synthd_local", "synthd_bank", "synthd_bank", Codec::SynthD, json!({})),
        ("synth53_local", "synth53_bank", "synth53_bank", Codec::Synth53, json!({})),
        ("synth52_local", "synth53_bank", "synth52_bank", Codec::Synth53, json!({})),
        ("synth51_local", "synth53_bank", "synth51_bank", Codec::Synth53, json!({})),
        ("synth80_local", "synth80_bank", "synth80_bank", Codec::Synth80, json!({})),
    ];
    for (kind, bank_example, bank_kind, codec, settings) in families {
        let mut channel = answered();
        channel.from_program(&example_bank(bank_example, bank_kind, LOCAL_BANK), 1);
        assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0], "{bank_kind}");
        let mut fields = settings;
        fields["handle"] = json!(LOCAL);
        fields["generation"] = json!(1);
        fields["banks"] = json!([LOCAL_BANK]);
        fields["setup"] = json!([program_change(0)]);
        channel.from_program(&message(kind, fields), 2);
        let effects = channel.take_effects();
        assert_eq!(refusals(&effects), [] as [Value; 0], "{kind}");
        let sent = sent_to(&effects, CONNECTION);
        assert_eq!(record_types(&sent), [TYPE_OBJECT, TYPE_DATA], "{kind}");
        let object = neoteletext_audio::idl::fragment(&neoteletext_core::packet::Packet { bytes: sent[..42].try_into().unwrap() }).unwrap().unwrap();
        // Record version, type, a two-byte length, then the OBJECT's body:
        // its handle, its generation and its codec (MIDI-48).
        assert_eq!(object.data[4], LOCAL, "{kind}");
        assert!(object.data.contains(&codec.code()), "{kind}: codec {}", codec.code());
    }
    // synth8, whose bank is the tests' own, with its keyboard's settings.
    let mut channel = answered();
    channel.from_program(&bank(), 1);
    let _ = channel.take_effects();
    let first = first_record(&channel);
    let local = json!({"handle": HANDLE, "generation": 1, "key_mode": "split", "split_key": 60, "banks": [BANK], "setup": []});
    channel.from_program(&message("synth8_local", local), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0]);
    let mut sound = Sound::local(HANDLE, 1, Codec::Synth8, &[]).unwrap();
    sound.banks = alloc::vec![BANK];
    sound.keys = Some([SYNTH8_KEYS_SPLIT, 60]);
    let mut records = alloc::vec![sound.object_record().unwrap()];
    records.extend(sound.data_records());
    assert_eq!(sent_to(&effects, CONNECTION), audio(&records, first), "an empty setup is an empty block");
    // No kind of the synth6581 family is local.
    for kind in ["synth6581_local", "synth8580_local"] {
        channel.from_program(&message(kind, json!({"handle": 3, "generation": 1, "banks": [BANK], "setup": []})), 3);
        let refused = refusals(&channel.take_effects());
        assert_eq!((&refused[0]["rule"], &refused[0]["path"]), (&json!("not_listed"), &json!("kind")), "{kind}");
    }
}

/// REQ-6532, RULE-61: a local sound's setup is at most 64 events, each at
/// time 0, that fit one block; and `play` of a local sound names a page,
/// with `loop` and `after` false.
#[test]
fn a_local_sound_is_refused_as_it_must_be() {
    let mut channel = answered();
    channel.from_program(&example_bank("synth106_bank", "synth106_bank", LOCAL_BANK), 1);
    let _ = channel.take_effects();
    let refused_as = |channel: &mut Channel, line: &[u8]| {
        channel.from_program(line, 2);
        let effects = channel.take_effects();
        assert_eq!(sent_to(&effects, CONNECTION), [] as [u8; 0], "nothing of a refused message is sent");
        let refused = refusals(&effects);
        (refused[0]["rule"].as_str().unwrap().to_string(), refused[0]["path"].as_str().unwrap().to_string())
    };
    let named = |rule: &str, path: &str| (rule.to_string(), path.to_string());
    // An event after time 0.
    assert_eq!(refused_as(&mut channel, &local_synth106(&json!([program_change(0), program_change(20_000)]))), named("out_of_range", "setup[1].time_us"));
    // A 65th event.
    let many: Vec<Value> = (0..65).map(|_| program_change(0)).collect();
    assert_eq!(refused_as(&mut channel, &local_synth106(&json!(many))), named("too_long", "setup"));
    // 64 events that do not fit one block: eleven control changes each.
    let long: Vec<Value> = (0..64).map(|_| json!({"event": "raw", "time_us": 0, "bytes": base64(&[0xB0, 7, 100].repeat(11))})).collect();
    assert_eq!(refused_as(&mut channel, &local_synth106(&json!(long))).0, "block_full");
    // No bank named, and a bank that is not one.
    let no_bank = message("synth106_local", json!({"handle": LOCAL, "generation": 1, "banks": [], "setup": []}));
    assert_eq!(refused_as(&mut channel, &no_bank), named("too_short", "banks"));
    let unknown = message("synth106_local", json!({"handle": LOCAL, "generation": 1, "banks": [77], "setup": []}));
    assert_eq!(refused_as(&mut channel, &unknown).0, "unknown_bank");
    // Played without a page, looping, or after the sound playing.
    channel.from_program(&local_synth106(&json!([program_change(0)])), 3);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0]);
    assert_eq!(refused_as(&mut channel, &play(&Value::Null, false, false)), named("out_of_range", "page"));
    assert_eq!(refused_as(&mut channel, &play(&json!("101"), true, false)), named("out_of_range", "loop"));
    assert_eq!(refused_as(&mut channel, &play(&json!("101"), false, true)), named("out_of_range", "after"));
    channel.from_program(&play(&json!("101"), false, false), 4);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [] as [Value; 0], "RULE-61's example of play");
    assert_eq!(record_types(&sent_to(&effects, CONNECTION)), [neoteletext_audio::record::TYPE_CONTROL]);
    // Another sound on the handle is no longer local: it plays as any does.
    channel.from_program(&message("synth106_live", json!({"handle": LOCAL, "generation": 1, "banks": [LOCAL_BANK]})), 5);
    let _ = channel.take_effects();
    channel.from_program(&play(&Value::Null, true, false), 6);
    assert_eq!(refusals(&channel.take_effects()), [] as [Value; 0]);
}
