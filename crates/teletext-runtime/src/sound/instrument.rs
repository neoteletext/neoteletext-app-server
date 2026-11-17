//! The instrument sounds' records (REQ-6413, DEC-6410, DEC-6429, DEC-6461):
//! a sound of synth6581 or synth8580, a synth7 sound and a bank of synth7
//! patches as the audio profile's OBJECT and DATA records of codecs 3, 4
//! and 5 (the engine's REQ-6411), and an analogue synthesizer's sound and
//! bank as those of codecs 6 to 10 and 11. A sound's kind names its
//! synthesizer, and so the chip or the model its record's extension says
//! plays it. The engine this
//! server is pinned to predates those codecs, so the records' bodies are
//! written here, through the engine's frame, as the REGIS and NAPLPS
//! records are. The message has been checked whole before it gets here.

use alloc::string::ToString;
use alloc::vec::Vec;

use neoteletext_audio::encode::frame;
use neoteletext_audio::record::{DATA_FIXED, TYPE_DATA, TYPE_OBJECT};
use neoteletext_audio::{MAX_BODY, rate_code};

use super::{Refused, bytes_of};
use crate::protocol::{
    INSTRUMENT_BLOCK_SAMPLES, INSTRUMENT_RATE, Rule, SYNTH7_PATCH_BYTES, SYNTH7_SLOT_BYTES, SYNTH6581_IMAGE_BYTES, Sound, Synth, Synth7BankSound,
    Synth7BlocksSound, Synth7LiveSound, Synth7Sound, Synth8KeyMode, Synth8LiveSound, Synth8Sound, Synth6581BlocksSound, Synth6581Clock, Synth6581LiveSound,
    Synth6581Sound, SynthBankSound, SynthBlocksSound, SynthLiveSound, SynthSound,
};

/// The profile's codec of a sound of synth6581 or synth8580.
const CODEC_SYNTH6581: u8 = 3;
/// The profile's codec of a synth7 sound.
const CODEC_SYNTH7: u8 = 4;
/// The profile's codec of a bank of synth7 patches.
const CODEC_SYNTH7_BANK: u8 = 5;
/// The OBJECT record's extension that names the chip and clock of a sound
/// of codec 3: one byte of the two bits below.
const TAG_SYNTH6581_CHIP: u8 = 3;
/// The chip is synth8580's; clear, synth6581's.
const CHIP_SYNTH8580: u8 = 1;
/// The clock is NTSC's; clear, PAL's.
const CLOCK_NTSC: u8 = 2;
/// The OBJECT record's extension that names a synth7 sound's bank: one
/// byte, the bank's handle.
const TAG_SYNTH7_BANK: u8 = 4;
/// The OBJECT record's extension that names the model of an analogue
/// synthesizer a sound plays: one byte, counted from 0.
const TAG_MODEL: u8 = 5;
/// The OBJECT record's extension that names the family a patch bank is
/// for: one byte, the code of that family's codec.
const TAG_BANK_FAMILY: u8 = 6;
/// The OBJECT record's extension of a synth8 sound's keyboard: two bytes,
/// the key mode and the split point.
const TAG_SYNTH8_KEYS: u8 = 7;
/// The profile's codec of a bank of an analogue synthesizer's patches.
const CODEC_PATCH_BANK: u8 = 11;

/// Which of the two chips of codec 3 a sound's kind names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chip {
    /// synth6581's.
    Synth6581,
    /// synth8580's.
    Synth8580,
}

/// What an OBJECT record says of a sound, after its handle and generation.
struct Object {
    /// The profile's codec.
    codec: u8,
    /// Bytes in each block.
    block_bytes: u16,
    /// The samples it plays for, or the patches of a bank; 0 for a live
    /// sound.
    samples: u32,
    /// Its blocks; 0 for a live sound.
    blocks: u16,
    /// Its extensions, each a tag, its length and its bytes.
    extensions: Vec<u8>,
}

/// The OBJECT record that announces a sound.
fn object_record(handle: u8, generation: u16, object: &Object) -> Result<Vec<u8>, Refused> {
    let rate = rate_code(INSTRUMENT_RATE).ok_or_else(|| (Rule::OutsideProfile, "the profile has no code for the instruments' sample rate".to_string()))?;
    let mut body = Vec::new();
    body.push(handle);
    body.extend_from_slice(&generation.to_be_bytes());
    // A sound's id, stable across its generations, is its handle, as the
    // engine's own sounds have it.
    body.extend_from_slice(&u32::from(handle).to_be_bytes());
    body.push(object.codec);
    body.push(rate);
    body.extend_from_slice(&object.block_bytes.to_be_bytes());
    body.extend_from_slice(&object.samples.to_be_bytes());
    body.extend_from_slice(&object.blocks.to_be_bytes());
    body.extend_from_slice(&object.extensions);
    Ok(frame(TYPE_OBJECT, &body))
}

/// DATA records of whole blocks of `block_bytes`, as many to a record as
/// fit its body, numbered from block `first`, counting modulo 65,536.
fn data_records(handle: u8, generation: u16, mut first: u16, block_bytes: u16, blocks: &[u8]) -> Vec<Vec<u8>> {
    let block = usize::from(block_bytes).max(1);
    let per_record = MAX_BODY.saturating_sub(DATA_FIXED).checked_div(block).unwrap_or(1).max(1);
    let mut records = Vec::new();
    for chunk in blocks.chunks(block.saturating_mul(per_record)) {
        let mut body = Vec::with_capacity(DATA_FIXED.saturating_add(chunk.len()));
        body.push(handle);
        body.extend_from_slice(&generation.to_be_bytes());
        body.extend_from_slice(&first.to_be_bytes());
        body.extend_from_slice(chunk);
        records.push(frame(TYPE_DATA, &body));
        first = first.wrapping_add(u16::try_from(per_record).unwrap_or(u16::MAX));
    }
    records
}

/// A stored sound's records: its object, saying how many blocks and
/// samples it has, then its blocks from block 0. `samples_a_block` is what
/// one block counts for: its samples, or one patch of a bank.
fn stored(handle: u8, generation: u16, mut object: Object, samples_a_block: u32, data: &[u8]) -> Result<Vec<Vec<u8>>, Refused> {
    let count = data.len().checked_div(usize::from(object.block_bytes)).unwrap_or(0);
    object.blocks = u16::try_from(count).map_err(|_| (Rule::TooLong, "more blocks than a sound may have".to_string()))?;
    object.samples = u32::from(object.blocks).saturating_mul(samples_a_block);
    let mut records = alloc::vec![object_record(handle, generation, &object)?];
    records.extend(data_records(handle, generation, 0, object.block_bytes, data));
    Ok(records)
}

/// What the OBJECT record of a sound of codec 3 says before its length.
fn synth6581_object(images_per_block: u8, chip: Chip, clock: Synth6581Clock) -> Object {
    let mut byte = 0;
    if chip == Chip::Synth8580 {
        byte |= CHIP_SYNTH8580;
    }
    if clock == Synth6581Clock::Ntsc {
        byte |= CLOCK_NTSC;
    }
    Object {
        codec: CODEC_SYNTH6581,
        block_bytes: synth6581_block_bytes(images_per_block),
        samples: 0,
        blocks: 0,
        extensions: alloc::vec![TAG_SYNTH6581_CHIP, 1, byte],
    }
}

/// The bytes of a block of codec 3.
fn synth6581_block_bytes(images_per_block: u8) -> u16 {
    SYNTH6581_IMAGE_BYTES.saturating_mul(u16::from(images_per_block))
}

/// What a synth7 sound's OBJECT record says before its length.
fn synth7_object(slots: u8, bank: u8) -> Object {
    Object { codec: CODEC_SYNTH7, block_bytes: synth7_block_bytes(slots), samples: 0, blocks: 0, extensions: alloc::vec![TAG_SYNTH7_BANK, 1, bank] }
}

/// The bytes of a synth7 sound's block.
fn synth7_block_bytes(slots: u8) -> u16 {
    SYNTH7_SLOT_BYTES.saturating_mul(u16::from(slots))
}

/// A stored sound's records, of synth6581 or synth8580.
fn synth6581(chip: Chip, sound: &Synth6581Sound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth6581_object(sound.images_per_block, chip, sound.clock);
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.images))
}

/// A live sound's object, of synth6581 or synth8580.
fn synth6581_live(chip: Chip, sound: &Synth6581LiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &synth6581_object(sound.images_per_block, chip, sound.clock))?])
}

/// A live sound's next blocks, of synth6581 or synth8580.
fn synth6581_blocks(sound: &Synth6581BlocksSound) -> Vec<Vec<u8>> {
    data_records(sound.handle, sound.generation, sound.first, synth6581_block_bytes(sound.images_per_block), &bytes_of(&sound.images))
}

/// A stored synth7 sound's records.
fn synth7(sound: &Synth7Sound) -> Result<Vec<Vec<u8>>, Refused> {
    stored(sound.handle, sound.generation, synth7_object(sound.slots, sound.bank), INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live synth7 sound's object.
fn synth7_live(sound: &Synth7LiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &synth7_object(sound.slots, sound.bank))?])
}

/// A live synth7 sound's next blocks.
fn synth7_blocks(sound: &Synth7BlocksSound) -> Vec<Vec<u8>> {
    data_records(sound.handle, sound.generation, sound.first, synth7_block_bytes(sound.slots), &bytes_of(&sound.blocks))
}

/// A bank's records: a sound whose blocks are its patches, and whose
/// length is their number.
fn synth7_bank(sound: &Synth7BankSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = Object { codec: CODEC_SYNTH7_BANK, block_bytes: SYNTH7_PATCH_BYTES, samples: 0, blocks: 0, extensions: Vec::new() };
    stored(sound.handle, sound.generation, object, 1, &bytes_of(&sound.patches))
}

/// The records of an instrument's sound or bank, of whichever kind:
/// synth6581's, synth8580's, synth7's or an analogue synthesizer's. Any
/// other kind of sound is written by [`super::Sounds::records`] and has
/// none here.
pub(super) fn records(sound: &Sound) -> Result<Vec<Vec<u8>>, Refused> {
    match sound {
        Sound::Synth6581(sound) => synth6581(Chip::Synth6581, sound),
        Sound::Synth6581Live(sound) => synth6581_live(Chip::Synth6581, sound),
        Sound::Synth8580(sound) => synth6581(Chip::Synth8580, sound),
        Sound::Synth8580Live(sound) => synth6581_live(Chip::Synth8580, sound),
        Sound::Synth6581Blocks(sound) | Sound::Synth8580Blocks(sound) => Ok(synth6581_blocks(sound)),
        Sound::Synth7(sound) => synth7(sound),
        Sound::Synth7Live(sound) => synth7_live(sound),
        Sound::Synth7Blocks(sound) => Ok(synth7_blocks(sound)),
        Sound::Synth7Bank(sound) => synth7_bank(sound),
        Sound::Synth106(sound) => synth(Synth::Synth106, sound),
        Sound::Synth106Live(sound) => synth_live(Synth::Synth106, sound),
        Sound::Synth60(sound) => synth(Synth::Synth60, sound),
        Sound::Synth60Live(sound) => synth_live(Synth::Synth60, sound),
        Sound::SynthD(sound) => synth(Synth::SynthD, sound),
        Sound::SynthDLive(sound) => synth_live(Synth::SynthD, sound),
        Sound::Synth53(sound) => synth(Synth::Synth53, sound),
        Sound::Synth53Live(sound) => synth_live(Synth::Synth53, sound),
        Sound::Synth52(sound) => synth(Synth::Synth52, sound),
        Sound::Synth52Live(sound) => synth_live(Synth::Synth52, sound),
        Sound::Synth51(sound) => synth(Synth::Synth51, sound),
        Sound::Synth51Live(sound) => synth_live(Synth::Synth51, sound),
        Sound::Synth8(sound) => synth8(sound),
        Sound::Synth8Live(sound) => synth8_live(sound),
        Sound::Synth80(sound) => synth(Synth::Synth80, sound),
        Sound::Synth80Live(sound) => synth_live(Synth::Synth80, sound),
        Sound::Synth106Blocks(sound)
        | Sound::Synth60Blocks(sound)
        | Sound::SynthDBlocks(sound)
        | Sound::Synth53Blocks(sound)
        | Sound::Synth52Blocks(sound)
        | Sound::Synth51Blocks(sound)
        | Sound::Synth8Blocks(sound)
        | Sound::Synth80Blocks(sound) => Ok(synth_blocks(sound)),
        Sound::Synth106Bank(sound) => synth_bank(Synth::Synth106, sound),
        Sound::Synth60Bank(sound) => synth_bank(Synth::Synth60, sound),
        Sound::SynthDBank(sound) => synth_bank(Synth::SynthD, sound),
        Sound::Synth53Bank(sound) => synth_bank(Synth::Synth53, sound),
        Sound::Synth52Bank(sound) => synth_bank(Synth::Synth52, sound),
        Sound::Synth51Bank(sound) => synth_bank(Synth::Synth51, sound),
        Sound::Synth8Bank(sound) => synth_bank(Synth::Synth8, sound),
        Sound::Synth80Bank(sound) => synth_bank(Synth::Synth80, sound),
        Sound::Opus(_)
        | Sound::OpusLive(_)
        | Sound::OpusPackets(_)
        | Sound::Object(_)
        | Sound::Live(_)
        | Sound::Blocks(_)
        | Sound::Play(_)
        | Sound::Stop(_)
        | Sound::Gain(_) => Err((Rule::NotListed, "not a sound of an instrument".to_string())),
    }
}

/// What an analogue synthesizer's OBJECT record says before its length:
/// its codec, its block, its bank, and `more` of its extensions after the
/// bank's.
fn synth_object(synth: Synth, slots: u8, bank: u8, more: &[u8]) -> Object {
    let mut extensions = alloc::vec![TAG_SYNTH7_BANK, 1, bank];
    extensions.extend_from_slice(more);
    Object { codec: synth.codec(), block_bytes: synth7_block_bytes(slots), samples: 0, blocks: 0, extensions }
}

/// The extension that names which model of its family `synth` is, as the
/// engine's `TAG_MODEL` counts it; none for a synthesizer whose family has
/// one model.
fn model(synth: Synth) -> Vec<u8> {
    synth.model().map_or_else(Vec::new, |byte| alloc::vec![TAG_MODEL, 1, byte])
}

/// The keyboard of synth8 as the engine's `TAG_SYNTH8_KEYS` writes it.
const fn synth8_keys(mode: Synth8KeyMode, split_key: u8) -> [u8; 4] {
    let mode = match mode {
        Synth8KeyMode::Whole => 0,
        Synth8KeyMode::Dual => 1,
        Synth8KeyMode::Split => 2,
    };
    [TAG_SYNTH8_KEYS, 2, mode, split_key]
}

/// A stored synth8 sound's records.
fn synth8(sound: &Synth8Sound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(Synth::Synth8, sound.slots, sound.bank, &synth8_keys(sound.key_mode, sound.split_key));
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live synth8 sound's object.
fn synth8_live(sound: &Synth8LiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(Synth::Synth8, sound.slots, sound.bank, &synth8_keys(sound.key_mode, sound.split_key));
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &object)?])
}

/// A stored sound's records, of any analogue synthesizer but synth8: its
/// name gives its model, where its family has more than one.
fn synth(synth: Synth, sound: &SynthSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(synth, sound.slots, sound.bank, &model(synth));
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live sound's object, of any analogue synthesizer but synth8.
fn synth_live(synth: Synth, sound: &SynthLiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &synth_object(synth, sound.slots, sound.bank, &model(synth)))?])
}

/// A live synthesizer sound's next blocks, of any of them.
fn synth_blocks(sound: &SynthBlocksSound) -> Vec<Vec<u8>> {
    data_records(sound.handle, sound.generation, sound.first, synth7_block_bytes(sound.slots), &bytes_of(&sound.blocks))
}

/// A synthesizer's patch bank's records: a sound of codec 11 whose blocks
/// are its patches, whose length is their number, and which names its
/// family.
fn synth_bank(synth: Synth, sound: &SynthBankSound) -> Result<Vec<Vec<u8>>, Refused> {
    let extensions = alloc::vec![TAG_BANK_FAMILY, 1, synth.codec()];
    let object = Object { codec: CODEC_PATCH_BANK, block_bytes: synth.patch_bytes(), samples: 0, blocks: 0, extensions };
    stored(sound.handle, sound.generation, object, 1, &bytes_of(&sound.patches))
}
