//! A program's instrument sounds as a viewer receives them (REQ-6413,
//! DEC-6410): a SID sound, a DX7 sound and a bank of DX7 patches as OBJECT
//! and DATA records of the audio profile's codecs 3, 4 and 5, each read
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
/// The profile's codecs of a SID sound, a DX7 sound and a bank.
const CODEC_SID: u8 = 3;
const CODEC_DX7: u8 = 4;
const CODEC_BANK: u8 = 5;
/// The sample-rate code of 48,000 Hz.
const RATE_48K: u8 = 7;
/// The samples of a 20 ms block at 48,000 Hz.
const BLOCK_SAMPLES: u32 = 960;
/// One image of the SID chip's registers.
const IMAGE: usize = 25;
/// One DX7 slot, and one patch.
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

/// Blocks of note-on slots: every slot one the DX7 codec takes.
fn notes(slots: usize, blocks: usize) -> Vec<u8> {
    (0..slots.saturating_mul(blocks)).flat_map(|index| [u8::try_from(index % 200).unwrap(), 0x90, u8::try_from(index % 128).unwrap(), 100]).collect()
}

/// A stored SID sound reaches the viewer as an OBJECT record of codec 3
/// that names its chip and clock, then its images as DATA from block 0;
/// any byte is an image's.
#[test]
fn a_stored_sid_sound_is_an_object_of_codec_3_and_its_images() {
    const BLOCKS: usize = 3;
    let images: Vec<u8> = (0..IMAGE * BLOCKS).map(|index| u8::try_from(255 - index).unwrap()).collect();
    for (chip, clock, byte) in [("6581", "pal", 0), ("8580", "pal", 1), ("6581", "ntsc", 2), ("8580", "ntsc", 3)] {
        let mut channel = answered();
        let got = sent(&mut channel, &sound("sid", json!({"images_per_block": 1, "chip": chip, "clock": clock, "images": base64(&images)})));
        assert_eq!(got.iter().map(|record| record.0).collect::<Vec<u8>>(), [TYPE_OBJECT, TYPE_DATA]);
        assert_eq!(object(&got[0].1), expected(CODEC_SID, IMAGE, 3 * BLOCK_SAMPLES, 3, &[3, 1, byte]), "{chip} {clock}");
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
    let got = sent(&mut channel, &sound("sid", json!({"images_per_block": 4, "chip": "6581", "clock": "pal", "images": base64(&images)})));
    assert_eq!(object(&got[0].1), expected(CODEC_SID, BLOCK, 25 * BLOCK_SAMPLES, 25, &[3, 1, 0]));
    let cut: Vec<(u16, Vec<u8>)> = got[1..].iter().map(|(kind, body)| (assert_eq!(*kind, TYPE_DATA), data(body)).1).collect();
    assert_eq!(cut, [(0, images[..1_000].to_vec()), (10, images[1_000..2_000].to_vec()), (20, images[2_000..].to_vec())]);

    // A DX7 sound of 16 slots: 64 bytes a block, 15 blocks a record.
    let blocks = notes(16, 31);
    let got = sent(&mut channel, &sound("dx7", json!({"slots": 16, "bank": 0, "blocks": base64(&blocks)})));
    let cut: Vec<(u16, usize)> = got[1..].iter().map(|(_, body)| (data(body).0, data(body).1.len())).collect();
    assert_eq!(cut, [(0, 15 * 64), (15, 15 * 64), (30, 64)]);
    assert_eq!(got[1..].iter().flat_map(|(_, body)| data(body).1).collect::<Vec<u8>>(), blocks);
}

/// A live SID sound is announced by an OBJECT record with no length, and
/// its blocks go as DATA records only, numbered from the block named,
/// counting modulo 65,536.
#[test]
fn a_live_sid_sound_is_announced_and_its_blocks_follow() {
    const BLOCK: usize = IMAGE * 2;
    let mut channel = answered();
    let got = sent(&mut channel, &sound("sid_live", json!({"images_per_block": 2, "chip": "8580", "clock": "ntsc"})));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, TYPE_OBJECT);
    assert_eq!(object(&got[0].1), expected(CODEC_SID, BLOCK, 0, 0, &[3, 1, 3]));

    // 20 blocks of 50 bytes fit a record; 25 blocks from 65,530 pass 65,535.
    let images = pattern(BLOCK * 25);
    let got = sent(&mut channel, &sound("sid_blocks", json!({"images_per_block": 2, "first": 65_530, "images": base64(&images)})));
    assert_eq!(got.iter().map(|record| record.0).collect::<Vec<u8>>(), [TYPE_DATA, TYPE_DATA]);
    assert_eq!(data(&got[0].1), (65_530, images[..1_000].to_vec()));
    assert_eq!(data(&got[1].1), (14, images[1_000..].to_vec()));
}

/// A stored DX7 sound is an OBJECT record of codec 4 that names its bank,
/// then its slots as DATA; a live one is announced with no length and its
/// blocks follow as DATA records only.
#[test]
fn a_dx7_sound_is_an_object_of_codec_4_that_names_its_bank() {
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
    let got = sent(&mut channel, &sound("dx7", json!({"slots": 3, "bank": BANK, "blocks": base64(&blocks)})));
    assert_eq!(got.iter().map(|record| record.0).collect::<Vec<u8>>(), [TYPE_OBJECT, TYPE_DATA]);
    assert_eq!(object(&got[0].1), expected(CODEC_DX7, 3 * SLOT, 4 * BLOCK_SAMPLES, 4, &[4, 1, BANK]));
    assert_eq!(data(&got[1].1), (0, blocks.clone()));

    let got = sent(&mut channel, &sound("dx7_live", json!({"slots": 16, "bank": BANK})));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, TYPE_OBJECT);
    assert_eq!(object(&got[0].1), expected(CODEC_DX7, 16 * SLOT, 0, 0, &[4, 1, BANK]));

    let got = sent(&mut channel, &sound("dx7_blocks", json!({"slots": 3, "first": 7, "blocks": base64(&blocks)})));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, TYPE_DATA);
    assert_eq!(data(&got[0].1), (7, blocks));
}

/// A bank is an OBJECT record of codec 5 with no extension, whose length
/// and blocks are its patches, then the patches as DATA, seven to a record.
#[test]
fn a_dx7_bank_is_an_object_of_codec_5_and_its_patches() {
    assert_eq!(DATA_ROOM / PATCH, 7);
    let patches: Vec<u8> = (0..PATCH * 32).map(|index| u8::try_from(index % 128).unwrap()).collect();
    let mut channel = answered();
    let got = sent(&mut channel, &sound("dx7_bank", json!({"patches": base64(&patches)})));
    assert_eq!(object(&got[0].1), expected(CODEC_BANK, PATCH, 32, 32, &[]));
    let cut: Vec<(u16, usize)> = got[1..].iter().map(|(kind, body)| (assert_eq!(*kind, TYPE_DATA), (data(body).0, data(body).1.len())).1).collect();
    assert_eq!(cut, [(0, 7 * PATCH), (7, 7 * PATCH), (14, 7 * PATCH), (21, 7 * PATCH), (28, 4 * PATCH)]);
    assert_eq!(got[1..].iter().flat_map(|(_, body)| data(body).1).collect::<Vec<u8>>(), patches);

    let one = &patches[..PATCH];
    let got = sent(&mut channel, &sound("dx7_bank", json!({"patches": base64(one)})));
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

/// A DX7 slot the codec does not take refuses the whole message, naming
/// its block and slot: a time past the block, a status that is not one of
/// the five messages, and each message's data out of its range.
#[test]
fn a_dx7_slot_the_codec_does_not_take_is_refused_with_its_block_and_slot() {
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
        for (kind, fields) in
            [("dx7", json!({"slots": 2, "bank": 0, "blocks": base64(&blocks)})), ("dx7_blocks", json!({"slots": 2, "first": 0, "blocks": base64(&blocks)}))]
        {
            let detail = refused(&sound(kind, fields), "blocks", "outside_profile");
            assert!(detail.starts_with("block 2 slot 1 "), "{slot:?}: {detail}");
        }
    }
}

/// Blocks that are not whole, none, or more than a sound may have are
/// refused by the rules the other sounds' data is; so are a number out of
/// its range and a chip or clock that is not one of the two.
#[test]
fn an_instrument_sound_out_of_its_bounds_is_refused() {
    let sid = |fields: Value| {
        let mut all = json!({"images_per_block": 2, "chip": "6581", "clock": "pal", "images": base64(&[0; 2 * IMAGE])});
        all.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        sound("sid", all)
    };
    refused(&sid(json!({"images": base64(&[0; IMAGE])})), "images", "wrong_length");
    refused(&sid(json!({"images": base64(&[0; 2 * IMAGE + 1])})), "images", "wrong_length");
    refused(&sid(json!({"images": ""})), "images", "wrong_length");
    refused(&sid(json!({"images_per_block": 1, "images": base64(&alloc::vec![0; IMAGE * (MOST_STORED + 1)])})), "images", "too_long");
    refused(&sid(json!({"images_per_block": 3})), "images_per_block", "not_listed");
    refused(&sid(json!({"images_per_block": 0})), "images_per_block", "not_listed");
    refused(&sid(json!({"chip": "6582"})), "chip", "not_listed");
    refused(&sid(json!({"chip": 6581})), "chip", "wrong_type");
    refused(&sid(json!({"clock": "PAL"})), "clock", "not_listed");
    refused(&sound("sid_live", json!({"images_per_block": 8, "chip": "6581", "clock": "pal"})), "images_per_block", "not_listed");
    refused(&sound("sid_live", json!({"images_per_block": 1, "chip": "sid", "clock": "pal"})), "chip", "not_listed");
    refused(&sound("sid_live", json!({"images_per_block": 1, "chip": "8580", "clock": "secam"})), "clock", "not_listed");
    let live = |images: &[u8]| sound("sid_blocks", json!({"images_per_block": 1, "first": 0, "images": base64(images)}));
    refused(&live(&[]), "images", "wrong_length");
    refused(&live(&[0; IMAGE - 1]), "images", "wrong_length");
    refused(&live(&alloc::vec![0; IMAGE * (MOST_LIVE + 1)]), "images", "too_long");
    refused(&sound("sid_blocks", json!({"images_per_block": 1, "first": 65_536, "images": base64(&[0; IMAGE])})), "first", "wrong_type");

    let dx7 = |slots: usize, blocks: &[u8]| sound("dx7", json!({"slots": slots, "bank": 0, "blocks": base64(blocks)}));
    refused(&dx7(2, &[0; SLOT]), "blocks", "wrong_length");
    refused(&dx7(1, &[]), "blocks", "wrong_length");
    refused(&dx7(1, &alloc::vec![0; SLOT * (MOST_STORED + 1)]), "blocks", "too_long");
    refused(&dx7(0, &[0; SLOT]), "slots", "out_of_range");
    refused(&dx7(17, &[0; 17 * SLOT]), "slots", "out_of_range");
    refused(&sound("dx7", json!({"slots": 1, "bank": 256, "blocks": base64(&[0; SLOT])})), "bank", "wrong_type");
    refused(&sound("dx7_live", json!({"slots": 17, "bank": 0})), "slots", "out_of_range");
    let live = |blocks: &[u8]| sound("dx7_blocks", json!({"slots": 1, "first": 0, "blocks": base64(blocks)}));
    refused(&live(&[]), "blocks", "wrong_length");
    refused(&live(&[0; SLOT + 1]), "blocks", "wrong_length");
    refused(&live(&alloc::vec![0; SLOT * (MOST_LIVE + 1)]), "blocks", "too_long");

    let bank = |patches: &[u8]| sound("dx7_bank", json!({"patches": base64(patches)}));
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
    let got = sent(&mut channel, &sound("sid", json!({"images_per_block": 4, "chip": "6581", "clock": "pal", "images": base64(&images)})));
    assert_eq!(object(&got[0].1), expected(CODEC_SID, IMAGE * 4, 5_760_000, 6_000, &[3, 1, 0]));
    assert_eq!(got.len(), 1 + 600);
    let got = sent(&mut channel, &sound("dx7", json!({"slots": 16, "bank": 0, "blocks": base64(&notes(16, MOST_STORED))})));
    assert_eq!(object(&got[0].1), expected(CODEC_DX7, 16 * SLOT, 5_760_000, 6_000, &[4, 1, 0]));
    let got = sent(&mut channel, &sound("sid_blocks", json!({"images_per_block": 4, "first": 0, "images": base64(&alloc::vec![0; IMAGE * 4 * MOST_LIVE])})));
    assert_eq!(got.len(), 10);
    let got = sent(&mut channel, &sound("dx7_blocks", json!({"slots": 16, "first": 0, "blocks": base64(&notes(16, MOST_LIVE))})));
    assert_eq!(got.len(), 7);
}

/// `play` names an instrument's sound as it names any other: a CONTROL
/// record with the sound's handle and generation.
#[test]
fn play_names_an_instrument_sound_as_any_other() {
    let mut channel = answered();
    sent(&mut channel, &sound("sid", json!({"images_per_block": 1, "chip": "6581", "clock": "pal", "images": base64(&[0; IMAGE])})));
    let play = json!({"channel": 2, "gain": 80, "fade_ms": 0, "loop": true, "after": false, "duck": 0, "page": null});
    let got = sent(&mut channel, &sound("play", play));
    assert_eq!(got.len(), 1);
    let (kind, body) = &got[0];
    assert_eq!(*kind, TYPE_CONTROL);
    // Channel 2, the first command on it, play (operation 1), then the sound.
    assert_eq!(body[..7], [2, 0, 1, 1, HANDLE, 0x01, 0x02]);
}
