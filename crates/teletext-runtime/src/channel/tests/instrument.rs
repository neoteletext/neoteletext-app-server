//! A program's instrument sounds as a viewer receives them (REQ-6413,
//! DEC-6410, DEC-6461): a sound of synth6581 or synth8580, a synth7 sound
//! and a bank of synth7 patches as OBJECT and DATA records of the audio profile's codecs 3, 4 and 5, each read
//! back here byte by byte against the engine's REQ-6411, and each fault a
//! receiver would refuse refused whole, with nothing sent. The figures are
//! written out here, not taken from the code they test.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use serde_json::{Value, json};

use super::{CONNECTION, Channel, Effect, answered, line, sent_to, told};
use crate::protocol::tests::base64;

/// The handle and generation of the tests' sounds.
const HANDLE: u8 = 9;
const GENERATION: u16 = 0x0102;
/// The record types of the audio profile.
const TYPE_OBJECT: u8 = 1;
const TYPE_DATA: u8 = 2;
const TYPE_CONTROL: u8 = 3;
/// The profile's codecs of a synth6581 sound, a synth7 sound and a bank.
const CODEC_SYNTH6581: u8 = 3;
const CODEC_SYNTH7: u8 = 4;
const CODEC_BANK: u8 = 5;
/// The sample-rate code of 48,000 Hz.
const RATE_48K: u8 = 7;
/// The samples of a 20 ms block at 48,000 Hz.
const BLOCK_SAMPLES: u32 = 960;
/// One image of the chip's registers.
const IMAGE: usize = 25;
/// One synth7 slot, and one patch.
const SLOT: usize = 4;
const PATCH: usize = 128;
/// The most blocks of a stored sound, and of one message of a live one.
const MOST_STORED: usize = 6_000;
const MOST_LIVE: usize = 100;
/// A DATA record's body holds at most 1,016 bytes, 5 of them its handle,
/// generation and first block.
const DATA_ROOM: usize = 1_016 - 5;

/// A sound message of `kind` for the test's connection.
fn sound(kind: &str, mut fields: Value) -> Vec<u8> {
    fields["type"] = json!("sound");
    fields["kind"] = json!(kind);
    fields["connections"] = json!([CONNECTION]);
    fields["handle"] = json!(HANDLE);
    fields["generation"] = json!(GENERATION);
    line(&fields)
}

/// The records in `bytes`, each as its type and body.
fn records(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut found = Vec::new();
    let mut record: Vec<u8> = Vec::new();
    for bytes in bytes.as_chunks::<PACKET_BYTES>().0 {
        let fragment = neoteletext_audio::idl::fragment(&Packet { bytes: *bytes }).unwrap().unwrap();
        if fragment.index == 0 {
            record.clear();
        }
        record.extend_from_slice(&fragment.data);
        if fragment.last {
            let (kind, body) = neoteletext_audio::record::split(&record[..neoteletext_audio::record::framed_length(&record).unwrap()]).unwrap();
            found.push((kind, body.to_vec()));
        }
    }
    found
}

/// What an OBJECT record's body says, field by field.
#[derive(PartialEq, Eq, Debug)]
struct Object {
    handle: u8,
    generation: u16,
    id: u32,
    codec: u8,
    rate: u8,
    block_bytes: u16,
    samples: u32,
    blocks: u16,
    extensions: Vec<u8>,
}

/// An OBJECT record's body, read by hand.
fn object(body: &[u8]) -> Object {
    Object {
        handle: body[0],
        generation: u16::from_be_bytes([body[1], body[2]]),
        id: u32::from_be_bytes([body[3], body[4], body[5], body[6]]),
        codec: body[7],
        rate: body[8],
        block_bytes: u16::from_be_bytes([body[9], body[10]]),
        samples: u32::from_be_bytes([body[11], body[12], body[13], body[14]]),
        blocks: u16::from_be_bytes([body[15], body[16]]),
        extensions: body[17..].to_vec(),
    }
}

/// The object the tests' sound has, of `codec`.
fn expected(codec: u8, block_bytes: usize, samples: u32, blocks: u16, extensions: &[u8]) -> Object {
    Object {
        handle: HANDLE,
        generation: GENERATION,
        id: u32::from(HANDLE),
        codec,
        rate: RATE_48K,
        block_bytes: u16::try_from(block_bytes).unwrap(),
        samples,
        blocks,
        extensions: extensions.to_vec(),
    }
}

/// A DATA record's body, read by hand: its first block, and its blocks'
/// bytes.
fn data(body: &[u8]) -> (u16, Vec<u8>) {
    assert_eq!(body[0], HANDLE);
    assert_eq!(u16::from_be_bytes([body[1], body[2]]), GENERATION);
    (u16::from_be_bytes([body[3], body[4]]), body[5..].to_vec())
}

/// The refusals in `effects`, each as its path and rule.
fn refusals(effects: &[Effect]) -> Vec<(String, String)> {
    told(effects)
        .iter()
        .filter(|line| line["type"] == "refused")
        .map(|line| (line["path"].as_str().unwrap().into(), line["rule"].as_str().unwrap().into()))
        .collect()
}

/// What the viewer is sent for one message: its records, none refused.
fn sent(channel: &mut Channel, message: &[u8]) -> Vec<(u8, Vec<u8>)> {
    channel.from_program(message, 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [], "{:?}", told(&effects));
    records(&sent_to(&effects, CONNECTION))
}

/// Bytes that differ from one to the next, so a block out of place shows.
fn pattern(length: usize) -> Vec<u8> {
    (0..length).map(|index| u8::try_from(index % 251).unwrap()).collect()
}

/// Blocks of note-on slots: every slot one the synth7 codec takes.
fn notes(slots: usize, blocks: usize) -> Vec<u8> {
    (0..slots.saturating_mul(blocks)).flat_map(|index| [u8::try_from(index % 200).unwrap(), 0x90, u8::try_from(index % 128).unwrap(), 100]).collect()
}

/// A stored sound of synth6581 or synth8580 reaches the viewer as an
/// OBJECT record of codec 3 that names its chip, which its kind gives
/// (DEC-6461), and its clock, then its images as DATA from block 0; any
/// byte is an image's.
#[test]
fn a_stored_synth6581_sound_is_an_object_of_codec_3_and_its_images() {
    const BLOCKS: usize = 3;
    let images: Vec<u8> = (0..IMAGE * BLOCKS).map(|index| u8::try_from(255 - index).unwrap()).collect();
    for (chip, clock, byte) in [("synth6581", "pal", 0), ("synth8580", "pal", 1), ("synth6581", "ntsc", 2), ("synth8580", "ntsc", 3)] {
        let mut channel = answered();
        let got = sent(&mut channel, &sound(chip, json!({"images_per_block": 1, "clock": clock, "images": base64(&images)})));
        assert_eq!(got.iter().map(|record| record.0).collect::<Vec<u8>>(), [TYPE_OBJECT, TYPE_DATA]);
        assert_eq!(object(&got[0].1), expected(CODEC_SYNTH6581, IMAGE, 3 * BLOCK_SAMPLES, 3, &[3, 1, byte]), "{chip} {clock}");
        assert_eq!(data(&got[1].1), (0, images.clone()));
    }
}

/// A stored sound longer than one record is cut at whole blocks, each
/// record saying which block it begins at: ten blocks of four images, 100
/// bytes each, fit a record's 1,011 bytes.
#[test]
fn a_stored_sound_is_cut_into_records_at_whole_blocks() {
    const BLOCK: usize = IMAGE * 4;
    const BLOCKS: usize = 25;
    const A_RECORD: usize = DATA_ROOM / BLOCK;
    assert_eq!(A_RECORD, 10);
    let images = pattern(BLOCK * BLOCKS);
    let mut channel = answered();
    let got = sent(&mut channel, &sound("synth6581", json!({"images_per_block": 4, "clock": "pal", "images": base64(&images)})));
    assert_eq!(object(&got[0].1), expected(CODEC_SYNTH6581, BLOCK, 25 * BLOCK_SAMPLES, 25, &[3, 1, 0]));
    let cut: Vec<(u16, Vec<u8>)> = got[1..].iter().map(|(kind, body)| (assert_eq!(*kind, TYPE_DATA), data(body)).1).collect();
    assert_eq!(cut, [(0, images[..1_000].to_vec()), (10, images[1_000..2_000].to_vec()), (20, images[2_000..].to_vec())]);

    // A synth7 sound of 16 slots: 64 bytes a block, 15 blocks a record.
    let blocks = notes(16, 31);
    let got = sent(&mut channel, &sound("synth7", json!({"slots": 16, "bank": 0, "blocks": base64(&blocks)})));
    let cut: Vec<(u16, usize)> = got[1..].iter().map(|(_, body)| (data(body).0, data(body).1.len())).collect();
    assert_eq!(cut, [(0, 15 * 64), (15, 15 * 64), (30, 64)]);
    assert_eq!(got[1..].iter().flat_map(|(_, body)| data(body).1).collect::<Vec<u8>>(), blocks);
}

/// A live synth8580 sound is announced by an OBJECT record with no length, and
/// its blocks go as DATA records only, numbered from the block named,
/// counting modulo 65,536.
#[test]
fn a_live_synth8580_sound_is_announced_and_its_blocks_follow() {
    const BLOCK: usize = IMAGE * 2;
    let mut channel = answered();
    let got = sent(&mut channel, &sound("synth8580_live", json!({"images_per_block": 2, "clock": "ntsc"})));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, TYPE_OBJECT);
    assert_eq!(object(&got[0].1), expected(CODEC_SYNTH6581, BLOCK, 0, 0, &[3, 1, 3]));

    // 20 blocks of 50 bytes fit a record; 25 blocks from 65,530 pass 65,535.
    let images = pattern(BLOCK * 25);
    let got = sent(&mut channel, &sound("synth8580_blocks", json!({"images_per_block": 2, "first": 65_530, "images": base64(&images)})));
    assert_eq!(got.iter().map(|record| record.0).collect::<Vec<u8>>(), [TYPE_DATA, TYPE_DATA]);
    assert_eq!(data(&got[0].1), (65_530, images[..1_000].to_vec()));
    assert_eq!(data(&got[1].1), (14, images[1_000..].to_vec()));
}

/// A stored synth7 sound is an OBJECT record of codec 4 that names its bank,
/// then its slots as DATA; a live one is announced with no length and its
/// blocks follow as DATA records only.
#[test]
fn a_synth7_sound_is_an_object_of_codec_4_that_names_its_bank() {
    /// The bank's handle.
    const BANK: u8 = 200;
    // Every message the codec takes, at its bounds, and an empty slot.
    let slots: [[u8; SLOT]; 12] = [
        [0, 0x90, 60, 100],
        [199, 0x8F, 127, 127],
        [0, 0, 0, 0],
        [10, 0xB0, 1, 127],
        [10, 0xB1, 7, 0],
        [10, 0xB2, 64, 127],
        [10, 0xB3, 120, 0],
        [10, 0xB4, 121, 0],
        [10, 0xBF, 123, 0],
        [0, 0xC0, 31, 0],
        [0, 0xE0, 127, 127],
        [199, 0x9F, 0, 0],
    ];
    let blocks: Vec<u8> = slots.as_flattened().to_vec();
    let mut channel = answered();
    let got = sent(&mut channel, &sound("synth7", json!({"slots": 3, "bank": BANK, "blocks": base64(&blocks)})));
    assert_eq!(got.iter().map(|record| record.0).collect::<Vec<u8>>(), [TYPE_OBJECT, TYPE_DATA]);
    assert_eq!(object(&got[0].1), expected(CODEC_SYNTH7, 3 * SLOT, 4 * BLOCK_SAMPLES, 4, &[4, 1, BANK]));
    assert_eq!(data(&got[1].1), (0, blocks.clone()));

    let got = sent(&mut channel, &sound("synth7_live", json!({"slots": 16, "bank": BANK})));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, TYPE_OBJECT);
    assert_eq!(object(&got[0].1), expected(CODEC_SYNTH7, 16 * SLOT, 0, 0, &[4, 1, BANK]));

    let got = sent(&mut channel, &sound("synth7_blocks", json!({"slots": 3, "first": 7, "blocks": base64(&blocks)})));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, TYPE_DATA);
    assert_eq!(data(&got[0].1), (7, blocks));
}

/// A bank is an OBJECT record of codec 5 with no extension, whose length
/// and blocks are its patches, then the patches as DATA, seven to a record.
#[test]
fn a_synth7_bank_is_an_object_of_codec_5_and_its_patches() {
    assert_eq!(DATA_ROOM / PATCH, 7);
    let patches: Vec<u8> = (0..PATCH * 32).map(|index| u8::try_from(index % 128).unwrap()).collect();
    let mut channel = answered();
    let got = sent(&mut channel, &sound("synth7_bank", json!({"patches": base64(&patches)})));
    assert_eq!(object(&got[0].1), expected(CODEC_BANK, PATCH, 32, 32, &[]));
    let cut: Vec<(u16, usize)> = got[1..].iter().map(|(kind, body)| (assert_eq!(*kind, TYPE_DATA), (data(body).0, data(body).1.len())).1).collect();
    assert_eq!(cut, [(0, 7 * PATCH), (7, 7 * PATCH), (14, 7 * PATCH), (21, 7 * PATCH), (28, 4 * PATCH)]);
    assert_eq!(got[1..].iter().flat_map(|(_, body)| data(body).1).collect::<Vec<u8>>(), patches);

    let one = &patches[..PATCH];
    let got = sent(&mut channel, &sound("synth7_bank", json!({"patches": base64(one)})));
    assert_eq!(object(&got[0].1), expected(CODEC_BANK, PATCH, 1, 1, &[]));
    assert_eq!(data(&got[1].1), (0, one.to_vec()));
}

/// `message` is refused at `path` for `rule`, and nothing is sent.
#[track_caller]
fn refused(message: &[u8], path: &str, rule: &str) -> String {
    let mut channel = answered();
    channel.from_program(message, 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), [(path.into(), rule.into())], "{:?}", told(&effects));
    assert_eq!(sent_to(&effects, CONNECTION), [] as [u8; 0]);
    told(&effects).iter().find(|line| line["type"] == "refused").unwrap()["detail"].as_str().unwrap().into()
}

/// A synth7 slot the codec does not take refuses the whole message, naming
/// its block and slot: a time past the block, a status that is not one of
/// the five messages, and each message's data out of its range.
#[test]
fn a_synth7_slot_the_codec_does_not_take_is_refused_with_its_block_and_slot() {
    let bad: [[u8; SLOT]; 16] = [
        [200, 0x90, 60, 100],
        [1, 0, 0, 0],
        [0, 0, 60, 0],
        [0, 0x70, 0, 0],
        [0, 0xA0, 60, 1],
        [0, 0xD0, 1, 0],
        [0, 0xF0, 0, 0],
        [0, 0x90, 128, 0],
        [0, 0x80, 0, 128],
        [0, 0xB0, 2, 0],
        [0, 0xB0, 0, 0],
        [0, 0xB0, 7, 128],
        [0, 0xC0, 32, 0],
        [0, 0xC0, 0, 1],
        [0, 0xE0, 128, 0],
        [0, 0xE0, 0, 128],
    ];
    for slot in bad {
        // The third block's second slot, among slots that are taken.
        let mut blocks = notes(2, 3);
        blocks[5 * SLOT..6 * SLOT].copy_from_slice(&slot);
        for (kind, fields) in [
            ("synth7", json!({"slots": 2, "bank": 0, "blocks": base64(&blocks)})),
            ("synth7_blocks", json!({"slots": 2, "first": 0, "blocks": base64(&blocks)})),
        ] {
            let detail = refused(&sound(kind, fields), "blocks", "outside_profile");
            assert!(detail.starts_with("block 2 slot 1 "), "{slot:?}: {detail}");
        }
    }
}

/// Blocks that are not whole, none, or more than a sound may have are
/// refused by the rules the other sounds' data is; so are a number out of
/// its range, a clock that is not one of the two, and a chip, which is
/// the kind's to name (DEC-6461).
#[test]
fn an_instrument_sound_out_of_its_bounds_is_refused() {
    let synth6581 = |fields: Value| {
        let mut all = json!({"images_per_block": 2, "clock": "pal", "images": base64(&[0; 2 * IMAGE])});
        all.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        sound("synth6581", all)
    };
    refused(&synth6581(json!({"images": base64(&[0; IMAGE])})), "images", "wrong_length");
    refused(&synth6581(json!({"images": base64(&[0; 2 * IMAGE + 1])})), "images", "wrong_length");
    refused(&synth6581(json!({"images": ""})), "images", "wrong_length");
    refused(&synth6581(json!({"images_per_block": 1, "images": base64(&alloc::vec![0; IMAGE * (MOST_STORED + 1)])})), "images", "too_long");
    refused(&synth6581(json!({"images_per_block": 3})), "images_per_block", "not_listed");
    refused(&synth6581(json!({"images_per_block": 0})), "images_per_block", "not_listed");
    refused(&synth6581(json!({"chip": "6581"})), "chip", "unknown_field");
    refused(&synth6581(json!({"clock": "PAL"})), "clock", "not_listed");
    refused(&sound("synth6581_live", json!({"images_per_block": 8, "clock": "pal"})), "images_per_block", "not_listed");
    refused(&sound("synth8580_live", json!({"images_per_block": 1, "chip": "8580", "clock": "pal"})), "chip", "unknown_field");
    refused(&sound("synth8580_live", json!({"images_per_block": 1, "clock": "secam"})), "clock", "not_listed");
    let live = |images: &[u8]| sound("synth6581_blocks", json!({"images_per_block": 1, "first": 0, "images": base64(images)}));
    refused(&live(&[]), "images", "wrong_length");
    refused(&live(&[0; IMAGE - 1]), "images", "wrong_length");
    refused(&live(&alloc::vec![0; IMAGE * (MOST_LIVE + 1)]), "images", "too_long");
    refused(&sound("synth6581_blocks", json!({"images_per_block": 1, "first": 65_536, "images": base64(&[0; IMAGE])})), "first", "wrong_type");

    let synth7 = |slots: usize, blocks: &[u8]| sound("synth7", json!({"slots": slots, "bank": 0, "blocks": base64(blocks)}));
    refused(&synth7(2, &[0; SLOT]), "blocks", "wrong_length");
    refused(&synth7(1, &[]), "blocks", "wrong_length");
    refused(&synth7(1, &alloc::vec![0; SLOT * (MOST_STORED + 1)]), "blocks", "too_long");
    refused(&synth7(0, &[0; SLOT]), "slots", "out_of_range");
    refused(&synth7(17, &[0; 17 * SLOT]), "slots", "out_of_range");
    refused(&sound("synth7", json!({"slots": 1, "bank": 256, "blocks": base64(&[0; SLOT])})), "bank", "wrong_type");
    refused(&sound("synth7_live", json!({"slots": 17, "bank": 0})), "slots", "out_of_range");
    let live = |blocks: &[u8]| sound("synth7_blocks", json!({"slots": 1, "first": 0, "blocks": base64(blocks)}));
    refused(&live(&[]), "blocks", "wrong_length");
    refused(&live(&[0; SLOT + 1]), "blocks", "wrong_length");
    refused(&live(&alloc::vec![0; SLOT * (MOST_LIVE + 1)]), "blocks", "too_long");

    let bank = |patches: &[u8]| sound("synth7_bank", json!({"patches": base64(patches)}));
    refused(&bank(&[]), "patches", "wrong_length");
    refused(&bank(&[0; PATCH - 1]), "patches", "wrong_length");
    refused(&bank(&alloc::vec![0; PATCH * 33]), "patches", "too_long");
    let mut patches = alloc::vec![127_u8; PATCH * 2];
    patches[PATCH + 5] = 128;
    assert_eq!(refused(&bank(&patches), "patches", "outside_profile"), "patch 1 byte 5 is 128: at most 127");
}

/// The longest sounds are taken: 6,000 blocks stored, 100 in one message
/// of a live sound, and a bank of 32 patches.
#[test]
fn an_instrument_sound_at_its_bounds_is_sent() {
    let mut channel = answered();
    let images = alloc::vec![0xFF_u8; IMAGE * 4 * MOST_STORED];
    let got = sent(&mut channel, &sound("synth6581", json!({"images_per_block": 4, "clock": "pal", "images": base64(&images)})));
    assert_eq!(object(&got[0].1), expected(CODEC_SYNTH6581, IMAGE * 4, 5_760_000, 6_000, &[3, 1, 0]));
    assert_eq!(got.len(), 1 + 600);
    let got = sent(&mut channel, &sound("synth7", json!({"slots": 16, "bank": 0, "blocks": base64(&notes(16, MOST_STORED))})));
    assert_eq!(object(&got[0].1), expected(CODEC_SYNTH7, 16 * SLOT, 5_760_000, 6_000, &[4, 1, 0]));
    let got =
        sent(&mut channel, &sound("synth6581_blocks", json!({"images_per_block": 4, "first": 0, "images": base64(&alloc::vec![0; IMAGE * 4 * MOST_LIVE])})));
    assert_eq!(got.len(), 10);
    let got = sent(&mut channel, &sound("synth7_blocks", json!({"slots": 16, "first": 0, "blocks": base64(&notes(16, MOST_LIVE))})));
    assert_eq!(got.len(), 7);
}

/// `play` names an instrument's sound as it names any other: a CONTROL
/// record with the sound's handle and generation.
#[test]
fn play_names_an_instrument_sound_as_any_other() {
    let mut channel = answered();
    sent(&mut channel, &sound("synth6581", json!({"images_per_block": 1, "clock": "pal", "images": base64(&[0; IMAGE])})));
    let play = json!({"channel": 2, "gain": 80, "fade_ms": 0, "loop": true, "after": false, "duck": 0, "page": null});
    let got = sent(&mut channel, &sound("play", play));
    assert_eq!(got.len(), 1);
    let (kind, body) = &got[0];
    assert_eq!(*kind, TYPE_CONTROL);
    // Channel 2, the first command on it, play (operation 1), then the sound.
    assert_eq!(body[..7], [2, 0, 1, 1, HANDLE, 0x01, 0x02]);
}

/// The profile's codecs of the analogue synthesizers' sounds, and of a
/// bank of their patches (DEC-6429).
const CODEC_SYNTH106: u8 = 6;
const CODEC_SYNTHD: u8 = 7;
const CODEC_SYNTH53: u8 = 8;
const CODEC_SYNTH8: u8 = 9;
const CODEC_SYNTH80: u8 = 10;
const CODEC_PATCH_BANK: u8 = 11;
/// The OBJECT extensions: the bank's handle, the model, a bank's family and
/// synth8's keyboard.
const TAG_BANK: u8 = 4;
const TAG_MODEL: u8 = 5;
const TAG_FAMILY: u8 = 6;
const TAG_KEYS: u8 = 7;

/// Each synthesizer's kind (DEC-6461), its family's codec, a sound's
/// fields beyond synth7's, the extensions after the bank's that it writes,
/// the model its name gives among them, and its patch's bytes.
fn synths() -> [(&'static str, u8, Value, Vec<u8>, usize); 8] {
    [
        ("synth106", CODEC_SYNTH106, json!({}), alloc::vec![TAG_MODEL, 1, 0], 18),
        ("synth60", CODEC_SYNTH106, json!({}), alloc::vec![TAG_MODEL, 1, 1], 18),
        ("synthd", CODEC_SYNTHD, json!({}), Vec::new(), 36),
        ("synth53", CODEC_SYNTH53, json!({}), alloc::vec![TAG_MODEL, 1, 0], 24),
        ("synth52", CODEC_SYNTH53, json!({}), alloc::vec![TAG_MODEL, 1, 1], 24),
        ("synth51", CODEC_SYNTH53, json!({}), alloc::vec![TAG_MODEL, 1, 1], 24),
        ("synth8", CODEC_SYNTH8, json!({"key_mode": "split", "split_key": 60}), alloc::vec![TAG_KEYS, 2, 2, 60], 43),
        ("synth80", CODEC_SYNTH80, json!({}), Vec::new(), 85),
    ]
}

/// `fields` with `more` added.
fn joined(mut fields: Value, more: &Value) -> Value {
    fields.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
    fields
}

/// A synthesizer's sound is synth7's in its family's codec: a stored one an
/// OBJECT record that names its bank, then its model or keyboard, then its
/// slots as DATA; a live one announced with no length, its blocks following
/// as DATA records only.
#[test]
fn a_synthesizer_sound_is_an_object_of_its_codec_that_names_its_bank() {
    /// The bank's handle.
    const BANK: u8 = 201;
    let blocks = notes(2, 3);
    for (kind, codec, more, extensions, _) in synths() {
        let mut channel = answered();
        let named: Vec<u8> = [TAG_BANK, 1, BANK].into_iter().chain(extensions).collect();
        let got = sent(&mut channel, &sound(kind, joined(json!({"slots": 2, "bank": BANK, "blocks": base64(&blocks)}), &more)));
        assert_eq!(got.iter().map(|record| record.0).collect::<Vec<u8>>(), [TYPE_OBJECT, TYPE_DATA], "{kind}");
        assert_eq!(object(&got[0].1), expected(codec, 2 * SLOT, 3 * BLOCK_SAMPLES, 3, &named), "{kind}");
        assert_eq!(data(&got[1].1), (0, blocks.clone()), "{kind}");

        let got = sent(&mut channel, &sound(&alloc::format!("{kind}_live"), joined(json!({"slots": 16, "bank": BANK}), &more)));
        assert_eq!(got.len(), 1, "{kind}");
        assert_eq!(object(&got[0].1), expected(codec, 16 * SLOT, 0, 0, &named), "{kind}");

        let got = sent(&mut channel, &sound(&alloc::format!("{kind}_blocks"), json!({"slots": 2, "first": 65_535, "blocks": base64(&blocks)})));
        assert_eq!(got.len(), 1, "{kind}");
        assert_eq!(got[0].0, TYPE_DATA);
        assert_eq!(data(&got[0].1), (65_535, blocks.clone()), "{kind}");
    }
}

/// Each key mode is the byte the engine counts it by: whole, dual and
/// split are 0, 1 and 2. (Each name's model is in `synths`: synth52 and
/// synth51 write the earlier revision's, 1.)
#[test]
fn a_synth8_sound_names_its_keyboard_by_the_engines_numbers() {
    let blocks = notes(1, 1);
    let cases = [
        (json!({"key_mode": "whole", "split_key": 0}), [TAG_KEYS, 2, 0, 0]),
        (json!({"key_mode": "dual", "split_key": 127}), [TAG_KEYS, 2, 1, 127]),
        (json!({"key_mode": "split", "split_key": 48}), [TAG_KEYS, 2, 2, 48]),
    ];
    for (more, tail) in cases {
        let mut channel = answered();
        let got = sent(&mut channel, &sound("synth8", joined(json!({"slots": 1, "bank": 3, "blocks": base64(&blocks)}), &more)));
        let named: Vec<u8> = [TAG_BANK, 1, 3].into_iter().chain(tail).collect();
        assert_eq!(object(&got[0].1).extensions, named, "{more}");
    }
}

/// DEC-6461: a kind the names replaced is no kind, and nothing is sent.
#[test]
fn a_kind_the_names_replaced_is_refused() {
    for old in ["juno", "sid", "prophet5_live"] {
        refused(&sound(old, json!({"slots": 1, "bank": 0, "blocks": base64(&[0; SLOT])})), "kind", "not_listed");
    }
}

/// A patch of `kind`, `bytes` long, that its instrument reads: every byte 0
/// but the range of synth106 and synth60, which must name one, here 8'.
fn plain(kind: &str, bytes: usize) -> Vec<u8> {
    /// The switch byte with the 8' range bit. Source: JUNO-106 owner's
    /// manual, MIDI Implementation 3.4.
    const SYNTH106_EIGHT_FEET: u8 = 0b010;
    let mut patch = alloc::vec![0; bytes];
    if kind == "synth106" || kind == "synth60" {
        patch[16] = SYNTH106_EIGHT_FEET;
    }
    patch
}

/// A synthesizer's bank is an OBJECT record of codec 11 that names its
/// family by its codec, whose block is that family's patch and whose length
/// is its patches, then the patches as DATA, as many as fit a record.
#[test]
fn a_synthesizer_bank_is_an_object_of_codec_11_that_names_its_family() {
    for (kind, codec, _, _, patch) in synths() {
        let patches = plain(kind, patch).repeat(32);
        let mut channel = answered();
        let got = sent(&mut channel, &sound(&alloc::format!("{kind}_bank"), json!({"patches": base64(&patches)})));
        assert_eq!(object(&got[0].1), expected(CODEC_PATCH_BANK, patch, 32, 32, &[TAG_FAMILY, 1, codec]), "{kind}");
        let a_record = DATA_ROOM / patch;
        let cut: Vec<(u16, usize)> = got[1..].iter().map(|(_, body)| (data(body).0, data(body).1.len())).collect();
        let want: Vec<(u16, usize)> = (0..32).step_by(a_record).map(|first| (u16::try_from(first).unwrap(), patch * a_record.min(32 - first))).collect();
        assert_eq!(cut, want, "{kind}");
    }
}

/// A patch the instrument's reader refuses refuses the whole bank, naming
/// the patch, the byte and its control: a receiver would play its default
/// patch in its place and say nothing. Every byte at its largest is taken.
#[test]
fn a_synthesizer_patch_out_of_its_ranges_is_refused_with_its_byte_and_control() {
    // A patch of each at every control's largest, and a byte past one.
    let synth106: Vec<u8> = [[127; 16].as_slice(), &[0x7C, 0x1F]].concat();
    let monophonic: Vec<u8> = [
        &242_i16.to_le_bytes()[..],
        &[100, 100, 5, 5, 5, 5],
        &(-700_i16).to_le_bytes(),
        &[5, 5],
        &700_i16.to_le_bytes(),
        &[100; 5],
        &(-500_i16).to_le_bytes(),
        &[100, 100],
        &10_000_u16.to_le_bytes(),
        &10_000_u16.to_le_bytes(),
        &[100],
        &10_000_u16.to_le_bytes(),
        &10_000_u16.to_le_bytes(),
        &[100, 100, 0x7F, 0x3F],
    ]
    .concat();
    let synth53: Vec<u8> = [[255; 22].as_slice(), &[127, 127]].concat();
    let mut synth8 = alloc::vec![255_u8; 43];
    for (at, most) in [
        (2, 3),
        (5, 1),
        (6, 1),
        (8, 2),
        (10, 3),
        (11, 3),
        (12, 1),
        (13, 1),
        (14, 36),
        (16, 3),
        (22, 1),
        (24, 1),
        (33, 1),
        (38, 3),
        (39, 1),
        (40, 1),
        (41, 1),
        (42, 1),
    ] {
        synth8[at] = most;
    }
    let mut synth80 = alloc::vec![127_u8; 85];
    for at in [3, 4, 29, 30, 76, 78, 80, 81, 82] {
        synth80[at] = 1;
    }
    for at in [52, 53, 62] {
        synth80[at] = 5;
    }
    let cases: [(&str, Vec<u8>, usize, u8, &str); 13] = [
        ("synth106", synth106.clone(), 3, 128, "byte 3 (DCO PWM) is 128: at most 127"),
        (
            "synth60",
            synth106.clone(),
            16,
            0x13,
            "byte 16 (range, waveforms and chorus) is 0x13: at most 0x7F, with exactly one of the range bits 0x01 (16'), 0x02 (8') and 0x04 (4')",
        ),
        ("synth106", synth106, 17, 0x20, "byte 17 (PWM, ENV, VCA and HPF switches) is 0x20: no bit outside 0x1F"),
        ("synthd", monophonic.clone(), 2, 101, "byte 2 (GLIDE) is 101: at most 100"),
        ("synthd", monophonic.clone(), 7, 6, "byte 7 (OSCILLATOR 2 WAVEFORM) is 6: at most 5"),
        ("synthd", monophonic.clone(), 9, 0x03, "byte 8 (OSCILLATOR 2 FREQUENCY) is 836: -700 to 700, the low byte first"),
        ("synthd", monophonic.clone(), 26, 0x28, "byte 25 (filter DECAY TIME) is 10256: at most 10000, the low byte first"),
        ("synthd", monophonic, 35, 0x40, "byte 35 (noise, filter, keyboard, glide and decay switches) is 0x40: no bit outside 0x3F"),
        ("synth52", synth53, 23, 128, "byte 23 (OSC B FINE) is 128: at most 127"),
        ("synth8", synth8.clone(), 14, 37, "byte 14 (VCO-2 RANGE) is 37: at most 36"),
        ("synth8", synth8, 38, 4, "byte 38 (ASSIGN MODE) is 4: at most 3"),
        ("synth80", synth80.clone(), 30, 2, "byte 30 (channel II SAWTOOTH) is 2: at most 1"),
        ("synth80", synth80.clone(), 62, 6, "byte 62 (SUB OSCILLATOR FUNCTION) is 6: at most 5"),
    ];
    for (kind, good, at, bad, why) in cases {
        let bank = |patch: &[u8]| sound(&alloc::format!("{kind}_bank"), json!({"patches": base64(&[good.as_slice(), patch].concat())}));
        sent(&mut answered(), &bank(&good));
        let mut patch = good.clone();
        patch[at] = bad;
        assert_eq!(refused(&bank(&patch), "patches", "outside_profile"), alloc::format!("patch 1 {why}"), "{kind}");
    }
    // The levers of synth80 are seven bits.
    let mut patch = synth80;
    patch[84] = 128;
    let detail = refused(&sound("synth80_bank", json!({"patches": base64(&patch)})), "patches", "outside_profile");
    assert_eq!(detail, "patch 0 byte 84 (TREMOLO/CHORUS DEPTH) is 128: at most 127");
}

/// A bank of the wrong length, none, or more than 32 patches is refused;
/// so is another family's patch length.
#[test]
fn a_synthesizer_bank_out_of_its_bounds_is_refused() {
    for (kind, _, _, _, patch) in synths() {
        let bank = |patches: &[u8]| sound(&alloc::format!("{kind}_bank"), json!({"patches": base64(patches)}));
        refused(&bank(&[]), "patches", "wrong_length");
        refused(&bank(&alloc::vec![0; patch - 1]), "patches", "wrong_length");
        refused(&bank(&plain(kind, patch).repeat(33)), "patches", "too_long");
    }
    refused(&sound("synthd_bank", json!({"patches": base64(&[0; 18])})), "patches", "wrong_length");
}

/// Each synthesizer takes synth7's messages, and its own: synth80 key and
/// channel pressure, synth106 and synth60 controllers 124 to 127, synth8
/// controllers 5, 8 and 65 and program changes on MIDI channels 1 and 2
/// only. What one takes another refuses, naming the block and slot.
#[test]
fn each_synthesizer_takes_its_own_messages() {
    let common: [[u8; SLOT]; 9] = [
        [0, 0x90, 60, 100],
        [199, 0x8F, 127, 127],
        [0, 0, 0, 0],
        [1, 0xB0, 1, 127],
        [1, 0xB0, 64, 0],
        [1, 0xB0, 123, 0],
        [0, 0xC0, 31, 0],
        [0, 0xC1, 0, 0],
        [0, 0xEF, 127, 127],
    ];
    let synth80: [[u8; SLOT]; 3] = [[0, 0xA0, 60, 127], [0, 0xAF, 0, 0], [0, 0xD3, 127, 0]];
    let synth8: [[u8; SLOT]; 3] = [[0, 0xB0, 5, 127], [0, 0xB1, 8, 0], [0, 0xB0, 65, 127]];
    let synth106: [[u8; SLOT]; 4] = [[0, 0xB0, 124, 0], [0, 0xB0, 125, 0], [0, 0xB0, 126, 1], [0, 0xBF, 127, 0]];
    let only_synth8_refuses: [u8; SLOT] = [0, 0xC2, 0, 0];
    let stored = |kind: &str, slots: &[[u8; SLOT]]| {
        let more = synths().into_iter().find(|synth| synth.0 == kind).unwrap().2;
        sound(kind, joined(json!({"slots": slots.len(), "bank": 0, "blocks": base64(slots.as_flattened())}), &more))
    };
    for (kind, ..) in synths() {
        sent(&mut answered(), &stored(kind, &common));
        let live = |slots: &[[u8; SLOT]]| sound(&alloc::format!("{kind}_blocks"), json!({"slots": 1, "first": 0, "blocks": base64(slots.as_flattened())}));
        for (own, extra) in [(&["synth80"][..], &synth80[..]), (&["synth8"], &synth8[..]), (&["synth106", "synth60"], &synth106[..])] {
            for slot in extra {
                if own.contains(&kind) {
                    sent(&mut answered(), &live(&[*slot]));
                } else {
                    let detail = refused(&live(&[[0; SLOT], *slot]), "blocks", "outside_profile");
                    assert!(detail.starts_with("block 1 slot 0 "), "{kind} {slot:?}: {detail}");
                }
            }
        }
        if kind == "synth8" {
            let detail = refused(&stored(kind, &[only_synth8_refuses]), "blocks", "outside_profile");
            assert!(detail.ends_with("a synth8's program change is on MIDI channel 1, the upper patch, or 2, the lower"), "{detail}");
        } else {
            sent(&mut answered(), &stored(kind, &[only_synth8_refuses]));
        }
    }
    // Their data out of range.
    for slot in [[0, 0xA0, 128, 0], [0, 0xA0, 0, 128], [0, 0xD0, 128, 0], [0, 0xD0, 1, 1]] {
        refused(&stored("synth80", &[slot]), "blocks", "outside_profile");
    }
    refused(&stored("synth8", &[[0, 0xB0, 5, 128]]), "blocks", "outside_profile");
    refused(&stored("synth8", &[[0, 0xB0, 9, 0]]), "blocks", "outside_profile");
    refused(&stored("synth106", &[[0, 0xB0, 127, 128]]), "blocks", "outside_profile");
    for kind in ["synth106", "synth60"] {
        let detail = refused(&stored(kind, &[[0, 0xB0, 122, 0]]), "blocks", "outside_profile");
        assert!(detail.ends_with(&alloc::format!("a {kind}'s control change is of controller 1, 7, 64, 120, 121 or 123 to 127")), "{detail}");
    }
}

/// A synthesizer sound's fields out of their bounds are refused, naming the
/// field; the longest sounds are taken.
#[test]
fn a_synthesizer_sound_out_of_its_bounds_is_refused() {
    refused(&sound("synth60", json!({"model": "60", "slots": 1, "bank": 0, "blocks": base64(&[0; SLOT])})), "model", "unknown_field");
    refused(&sound("synth52_live", json!({"model": "revisions_1_and_2", "slots": 1, "bank": 0})), "model", "unknown_field");
    refused(&sound("synth8", json!({"key_mode": "split", "split_key": 128, "slots": 1, "bank": 0, "blocks": base64(&[0; SLOT])})), "split_key", "out_of_range");
    refused(&sound("synth8_live", json!({"key_mode": "layer", "split_key": 60, "slots": 1, "bank": 0})), "key_mode", "not_listed");
    refused(&sound("synthd", json!({"model": "106", "slots": 1, "bank": 0, "blocks": base64(&[0; SLOT])})), "model", "unknown_field");
    refused(&sound("synth80", json!({"slots": 0, "bank": 0, "blocks": base64(&[0; SLOT])})), "slots", "out_of_range");
    refused(&sound("synth80_live", json!({"slots": 17, "bank": 0})), "slots", "out_of_range");
    refused(&sound("synthd", json!({"slots": 1, "bank": 0, "blocks": base64(&alloc::vec![0; SLOT * (MOST_STORED + 1)])})), "blocks", "too_long");
    refused(&sound("synthd_blocks", json!({"slots": 1, "first": 0, "blocks": base64(&alloc::vec![0; SLOT * (MOST_LIVE + 1)])})), "blocks", "too_long");
    refused(&sound("synth53_blocks", json!({"slots": 2, "first": 0, "blocks": base64(&[0; SLOT])})), "blocks", "wrong_length");

    let mut channel = answered();
    let got = sent(&mut channel, &sound("synth80", json!({"slots": 16, "bank": 0, "blocks": base64(&notes(16, MOST_STORED))})));
    assert_eq!(object(&got[0].1), expected(CODEC_SYNTH80, 16 * SLOT, 5_760_000, 6_000, &[TAG_BANK, 1, 0]));
    let got = sent(&mut channel, &sound("synth106_blocks", json!({"slots": 16, "first": 0, "blocks": base64(&notes(16, MOST_LIVE))})));
    assert_eq!(got.len(), 7);
}
