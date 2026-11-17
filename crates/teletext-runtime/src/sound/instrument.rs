//! The instrument sounds' records (REQ-6413, DEC-6410, DEC-6429): a SID
//! sound, a DX7 sound and a bank of DX7 patches as the audio profile's
//! OBJECT and DATA records of codecs 3, 4 and 5 (the engine's REQ-6411),
//! and an analogue synthesizer's sound and bank as those of codecs 6 to 10
//! and 11. The engine this
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
    DX7_PATCH_BYTES, DX7_SLOT_BYTES, Dx7BankSound, Dx7BlocksSound, Dx7LiveSound, Dx7Sound, INSTRUMENT_BLOCK_SAMPLES, INSTRUMENT_RATE, JunoLiveSound, JunoModel,
    JunoSound, Jupiter8KeyMode, Jupiter8LiveSound, Jupiter8Sound, Prophet5LiveSound, Prophet5Model, Prophet5Sound, Rule, SID_IMAGE_BYTES, SidBlocksSound,
    SidChip, SidClock, SidLiveSound, SidSound, Sound, Synth, SynthBankSound, SynthBlocksSound, SynthLiveSound, SynthSound,
};

/// The profile's codec of a SID sound.
const CODEC_SID: u8 = 3;
/// The profile's codec of a DX7 sound.
const CODEC_DX7: u8 = 4;
/// The profile's codec of a bank of DX7 patches.
const CODEC_DX7_BANK: u8 = 5;
/// The OBJECT record's extension that names a SID sound's chip and clock:
/// one byte of the two bits below.
const TAG_SID_CHIP: u8 = 3;
/// The chip is the 8580; clear, the 6581.
const SID_CHIP_8580: u8 = 1;
/// The clock is NTSC's; clear, PAL's.
const SID_CLOCK_NTSC: u8 = 2;
/// The OBJECT record's extension that names a DX7 sound's bank: one byte,
/// the bank's handle.
const TAG_DX7_BANK: u8 = 4;
/// The OBJECT record's extension that names the model of an analogue
/// synthesizer a sound plays: one byte, counted from 0.
const TAG_MODEL: u8 = 5;
/// The OBJECT record's extension that names the family a patch bank is
/// for: one byte, the code of that family's codec.
const TAG_BANK_FAMILY: u8 = 6;
/// The OBJECT record's extension of a Jupiter-8 sound's keyboard: two
/// bytes, the key mode and the split point.
const TAG_JUPITER8_KEYS: u8 = 7;
/// The profile's codec of a bank of an analogue synthesizer's patches.
const CODEC_PATCH_BANK: u8 = 11;

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

/// What a SID sound's OBJECT record says before its length.
fn sid_object(images_per_block: u8, chip: SidChip, clock: SidClock) -> Object {
    let mut byte = 0;
    if chip == SidChip::Mos8580 {
        byte |= SID_CHIP_8580;
    }
    if clock == SidClock::Ntsc {
        byte |= SID_CLOCK_NTSC;
    }
    Object { codec: CODEC_SID, block_bytes: sid_block_bytes(images_per_block), samples: 0, blocks: 0, extensions: alloc::vec![TAG_SID_CHIP, 1, byte] }
}

/// The bytes of a SID sound's block.
fn sid_block_bytes(images_per_block: u8) -> u16 {
    SID_IMAGE_BYTES.saturating_mul(u16::from(images_per_block))
}

/// What a DX7 sound's OBJECT record says before its length.
fn dx7_object(slots: u8, bank: u8) -> Object {
    Object { codec: CODEC_DX7, block_bytes: dx7_block_bytes(slots), samples: 0, blocks: 0, extensions: alloc::vec![TAG_DX7_BANK, 1, bank] }
}

/// The bytes of a DX7 sound's block.
fn dx7_block_bytes(slots: u8) -> u16 {
    DX7_SLOT_BYTES.saturating_mul(u16::from(slots))
}

/// A stored SID sound's records.
fn sid(sound: &SidSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = sid_object(sound.images_per_block, sound.chip, sound.clock);
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.images))
}

/// A live SID sound's object.
fn sid_live(sound: &SidLiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &sid_object(sound.images_per_block, sound.chip, sound.clock))?])
}

/// A live SID sound's next blocks.
fn sid_blocks(sound: &SidBlocksSound) -> Vec<Vec<u8>> {
    data_records(sound.handle, sound.generation, sound.first, sid_block_bytes(sound.images_per_block), &bytes_of(&sound.images))
}

/// A stored DX7 sound's records.
fn dx7(sound: &Dx7Sound) -> Result<Vec<Vec<u8>>, Refused> {
    stored(sound.handle, sound.generation, dx7_object(sound.slots, sound.bank), INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live DX7 sound's object.
fn dx7_live(sound: &Dx7LiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &dx7_object(sound.slots, sound.bank))?])
}

/// A live DX7 sound's next blocks.
fn dx7_blocks(sound: &Dx7BlocksSound) -> Vec<Vec<u8>> {
    data_records(sound.handle, sound.generation, sound.first, dx7_block_bytes(sound.slots), &bytes_of(&sound.blocks))
}

/// A bank's records: a sound whose blocks are its patches, and whose
/// length is their number.
fn dx7_bank(sound: &Dx7BankSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = Object { codec: CODEC_DX7_BANK, block_bytes: DX7_PATCH_BYTES, samples: 0, blocks: 0, extensions: Vec::new() };
    stored(sound.handle, sound.generation, object, 1, &bytes_of(&sound.patches))
}

/// The records of an instrument's sound or bank, of whichever kind: the
/// SID's, the DX7's or an analogue synthesizer's. Any other kind of sound
/// is written by [`super::Sounds::records`] and has none here.
pub(super) fn records(sound: &Sound) -> Result<Vec<Vec<u8>>, Refused> {
    match sound {
        Sound::Sid(sound) => sid(sound),
        Sound::SidLive(sound) => sid_live(sound),
        Sound::SidBlocks(sound) => Ok(sid_blocks(sound)),
        Sound::Dx7(sound) => dx7(sound),
        Sound::Dx7Live(sound) => dx7_live(sound),
        Sound::Dx7Blocks(sound) => Ok(dx7_blocks(sound)),
        Sound::Dx7Bank(sound) => dx7_bank(sound),
        Sound::Juno(sound) => juno(sound),
        Sound::JunoLive(sound) => juno_live(sound),
        Sound::Minimoog(sound) => synth(Synth::Minimoog, sound),
        Sound::MinimoogLive(sound) => synth_live(Synth::Minimoog, sound),
        Sound::Prophet5(sound) => prophet5(sound),
        Sound::Prophet5Live(sound) => prophet5_live(sound),
        Sound::Jupiter8(sound) => jupiter8(sound),
        Sound::Jupiter8Live(sound) => jupiter8_live(sound),
        Sound::Cs80(sound) => synth(Synth::Cs80, sound),
        Sound::Cs80Live(sound) => synth_live(Synth::Cs80, sound),
        Sound::JunoBlocks(sound) | Sound::MinimoogBlocks(sound) | Sound::Prophet5Blocks(sound) | Sound::Jupiter8Blocks(sound) | Sound::Cs80Blocks(sound) => {
            Ok(synth_blocks(sound))
        }
        Sound::JunoBank(sound) => synth_bank(Synth::Juno, sound),
        Sound::MinimoogBank(sound) => synth_bank(Synth::Minimoog, sound),
        Sound::Prophet5Bank(sound) => synth_bank(Synth::Prophet5, sound),
        Sound::Jupiter8Bank(sound) => synth_bank(Synth::Jupiter8, sound),
        Sound::Cs80Bank(sound) => synth_bank(Synth::Cs80, sound),
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
    let mut extensions = alloc::vec![TAG_DX7_BANK, 1, bank];
    extensions.extend_from_slice(more);
    Object { codec: synth.codec(), block_bytes: dx7_block_bytes(slots), samples: 0, blocks: 0, extensions }
}

/// A Juno's model as the engine's `TAG_MODEL` counts it.
const fn juno_model(model: JunoModel) -> [u8; 3] {
    let byte = match model {
        JunoModel::Juno106 => 0,
        JunoModel::Juno60 => 1,
    };
    [TAG_MODEL, 1, byte]
}

/// A Prophet-5's revision as the engine's `TAG_MODEL` counts it.
const fn prophet5_model(model: Prophet5Model) -> [u8; 3] {
    let byte = match model {
        Prophet5Model::Revision3 => 0,
        Prophet5Model::Revisions1And2 => 1,
    };
    [TAG_MODEL, 1, byte]
}

/// A Jupiter-8's keyboard as the engine's `TAG_JUPITER8_KEYS` writes it.
const fn jupiter8_keys(mode: Jupiter8KeyMode, split_key: u8) -> [u8; 4] {
    let mode = match mode {
        Jupiter8KeyMode::Whole => 0,
        Jupiter8KeyMode::Dual => 1,
        Jupiter8KeyMode::Split => 2,
    };
    [TAG_JUPITER8_KEYS, 2, mode, split_key]
}

/// A stored Juno sound's records.
fn juno(sound: &JunoSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(Synth::Juno, sound.slots, sound.bank, &juno_model(sound.model));
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live Juno sound's object.
fn juno_live(sound: &JunoLiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(Synth::Juno, sound.slots, sound.bank, &juno_model(sound.model));
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &object)?])
}

/// A stored Prophet-5 sound's records.
fn prophet5(sound: &Prophet5Sound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(Synth::Prophet5, sound.slots, sound.bank, &prophet5_model(sound.model));
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live Prophet-5 sound's object.
fn prophet5_live(sound: &Prophet5LiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(Synth::Prophet5, sound.slots, sound.bank, &prophet5_model(sound.model));
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &object)?])
}

/// A stored Jupiter-8 sound's records.
fn jupiter8(sound: &Jupiter8Sound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(Synth::Jupiter8, sound.slots, sound.bank, &jupiter8_keys(sound.key_mode, sound.split_key));
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live Jupiter-8 sound's object.
fn jupiter8_live(sound: &Jupiter8LiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(Synth::Jupiter8, sound.slots, sound.bank, &jupiter8_keys(sound.key_mode, sound.split_key));
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &object)?])
}

/// A stored sound's records of a synthesizer with one model and one way
/// of playing its keys: the Minimoog or the CS-80.
fn synth(synth: Synth, sound: &SynthSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = synth_object(synth, sound.slots, sound.bank, &[]);
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live sound's object, of the Minimoog or the CS-80.
fn synth_live(synth: Synth, sound: &SynthLiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &synth_object(synth, sound.slots, sound.bank, &[]))?])
}

/// A live synthesizer sound's next blocks, of any of them.
fn synth_blocks(sound: &SynthBlocksSound) -> Vec<Vec<u8>> {
    data_records(sound.handle, sound.generation, sound.first, dx7_block_bytes(sound.slots), &bytes_of(&sound.blocks))
}

/// A synthesizer's patch bank's records: a sound of codec 11 whose blocks
/// are its patches, whose length is their number, and which names its
/// family.
fn synth_bank(synth: Synth, sound: &SynthBankSound) -> Result<Vec<Vec<u8>>, Refused> {
    let extensions = alloc::vec![TAG_BANK_FAMILY, 1, synth.codec()];
    let object = Object { codec: CODEC_PATCH_BANK, block_bytes: synth.patch_bytes(), samples: 0, blocks: 0, extensions };
    stored(sound.handle, sound.generation, object, 1, &bytes_of(&sound.patches))
}
