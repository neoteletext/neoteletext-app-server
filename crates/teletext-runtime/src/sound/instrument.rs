//! The instrument sounds' records (REQ-6413, DEC-6410): a SID sound, a DX7
//! sound and a bank of DX7 patches as the audio profile's OBJECT and DATA
//! records of codecs 3, 4 and 5 (the engine's REQ-6411). The engine this
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
    DX7_PATCH_BYTES, DX7_SLOT_BYTES, Dx7BankSound, Dx7BlocksSound, Dx7LiveSound, Dx7Sound, INSTRUMENT_BLOCK_SAMPLES, INSTRUMENT_RATE, Rule, SID_IMAGE_BYTES,
    SidBlocksSound, SidChip, SidClock, SidLiveSound, SidSound,
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
    /// Its extension, a tag and its one byte.
    extension: Option<(u8, u8)>,
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
    if let Some((tag, value)) = object.extension {
        body.extend_from_slice(&[tag, 1, value]);
    }
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
    Object { codec: CODEC_SID, block_bytes: sid_block_bytes(images_per_block), samples: 0, blocks: 0, extension: Some((TAG_SID_CHIP, byte)) }
}

/// The bytes of a SID sound's block.
fn sid_block_bytes(images_per_block: u8) -> u16 {
    SID_IMAGE_BYTES.saturating_mul(u16::from(images_per_block))
}

/// What a DX7 sound's OBJECT record says before its length.
fn dx7_object(slots: u8, bank: u8) -> Object {
    Object { codec: CODEC_DX7, block_bytes: dx7_block_bytes(slots), samples: 0, blocks: 0, extension: Some((TAG_DX7_BANK, bank)) }
}

/// The bytes of a DX7 sound's block.
fn dx7_block_bytes(slots: u8) -> u16 {
    DX7_SLOT_BYTES.saturating_mul(u16::from(slots))
}

/// A stored SID sound's records.
pub(super) fn sid(sound: &SidSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = sid_object(sound.images_per_block, sound.chip, sound.clock);
    stored(sound.handle, sound.generation, object, INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.images))
}

/// A live SID sound's object.
pub(super) fn sid_live(sound: &SidLiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &sid_object(sound.images_per_block, sound.chip, sound.clock))?])
}

/// A live SID sound's next blocks.
pub(super) fn sid_blocks(sound: &SidBlocksSound) -> Vec<Vec<u8>> {
    data_records(sound.handle, sound.generation, sound.first, sid_block_bytes(sound.images_per_block), &bytes_of(&sound.images))
}

/// A stored DX7 sound's records.
pub(super) fn dx7(sound: &Dx7Sound) -> Result<Vec<Vec<u8>>, Refused> {
    stored(sound.handle, sound.generation, dx7_object(sound.slots, sound.bank), INSTRUMENT_BLOCK_SAMPLES, &bytes_of(&sound.blocks))
}

/// A live DX7 sound's object.
pub(super) fn dx7_live(sound: &Dx7LiveSound) -> Result<Vec<Vec<u8>>, Refused> {
    Ok(alloc::vec![object_record(sound.handle, sound.generation, &dx7_object(sound.slots, sound.bank))?])
}

/// A live DX7 sound's next blocks.
pub(super) fn dx7_blocks(sound: &Dx7BlocksSound) -> Vec<Vec<u8>> {
    data_records(sound.handle, sound.generation, sound.first, dx7_block_bytes(sound.slots), &bytes_of(&sound.blocks))
}

/// A bank's records: a sound whose blocks are its patches, and whose
/// length is their number.
pub(super) fn dx7_bank(sound: &Dx7BankSound) -> Result<Vec<Vec<u8>>, Refused> {
    let object = Object { codec: CODEC_DX7_BANK, block_bytes: DX7_PATCH_BYTES, samples: 0, blocks: 0, extension: None };
    stored(sound.handle, sound.generation, object, 1, &bytes_of(&sound.patches))
}
