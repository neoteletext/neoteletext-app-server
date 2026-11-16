//! The instrument sounds' rules (REQ-6413, DEC-6410): a SID sound's images
//! of the chip's registers, a DX7 sound's timed MIDI messages and a bank
//! of DX7 patches, each held to what its receiver plays (the engine's
//! REQ-6411), so that a program learns of a fault where it made it and no
//! receiver is sent a block it would refuse.

use alloc::format;

use super::super::{Connections, DX7_MOST_PATCHES, DX7_MOST_SLOTS, DX7_PATCH_BYTES, DX7_SLOT_BYTES, Rule, SID_IMAGE_BYTES, SID_IMAGES_PER_BLOCK};
use super::{Checked, Path, check_base64, check_connections, fault};

/// The last time a DX7 slot may carry: tenths of a millisecond within a
/// 20 ms block, 0 to 199.
const DX7_LAST_TIME: u8 = 199;
/// The most a MIDI data byte holds: 7 bits (MIDI 1.0 Detailed
/// Specification, "Data Bytes"). A DX7 patch's bytes are held to it too,
/// as the instrument's own bulk dump is.
const MIDI_DATA_MOST: u8 = 0x7F;
/// A MIDI status byte's message, its high four bits; the low four are the
/// MIDI channel, which is the part that plays it.
const MIDI_MESSAGE_BITS: u8 = 0xF0;
/// Note off.
const NOTE_OFF: u8 = 0x80;
/// Note on.
const NOTE_ON: u8 = 0x90;
/// Control change.
const CONTROL_CHANGE: u8 = 0xB0;
/// Program change.
const PROGRAM_CHANGE: u8 = 0xC0;
/// Pitch bend.
const PITCH_BEND: u8 = 0xE0;
/// The controllers the DX7 codec takes: the modulation wheel (1), volume
/// (7), the sustain pedal (64), all sound off (120), reset all controllers
/// (121) and all notes off (123).
const CONTROLLERS: [u8; 6] = [1, 7, 64, 120, 121, 123];
/// The last patch a program change may choose: a bank holds 32.
const LAST_PATCH: u8 = 31;
const _: () = assert!(LAST_PATCH as usize + 1 == DX7_MOST_PATCHES);

/// Whole blocks of `block_bytes` in the base64 at `path`, 1 to `most` of
/// them.
fn check_blocks(path: &Path<'_>, text: &str, block_bytes: u16, most: usize, block: &str) -> Result<alloc::vec::Vec<u8>, super::Fault> {
    let size = usize::from(block_bytes);
    let bytes = check_base64(path, text, most.saturating_mul(size))?;
    if bytes.is_empty() || size == 0 || !bytes.len().is_multiple_of(size) {
        return Err(fault(path, Rule::WrongLength, format!("{} bytes: whole {block} of {block_bytes}", bytes.len())));
    }
    let count = bytes.len().checked_div(size).unwrap_or(0);
    if count > most {
        return Err(fault(path, Rule::TooLong, format!("{count} {block}, at most {most}")));
    }
    Ok(bytes)
}

/// A SID sound's kinds: who it is for, images to a block, and, where the
/// kind carries blocks, `images` as 1 to `most` whole blocks. Every byte
/// is a value some register takes, so the images themselves are not read.
pub(super) fn sid(root: &Path<'_>, connections: &Connections, images_per_block: u8, images: Option<(&str, usize)>) -> Checked {
    check_connections(&root.field("connections"), connections)?;
    if !SID_IMAGES_PER_BLOCK.contains(&images_per_block) {
        return Err(fault(&root.field("images_per_block"), Rule::NotListed, format!("{images_per_block}: one of {SID_IMAGES_PER_BLOCK:?}")));
    }
    let Some((images, most)) = images else { return Ok(()) };
    let block_bytes = SID_IMAGE_BYTES.saturating_mul(u16::from(images_per_block));
    check_blocks(&root.field("images"), images, block_bytes, most, "blocks").map(|_| ())
}

/// A DX7 sound's kinds: who it is for, slots to a block, and, where the
/// kind carries blocks, `blocks` as 1 to `most` whole blocks whose every
/// slot is empty or a message the codec takes.
pub(super) fn dx7(root: &Path<'_>, connections: &Connections, slots: u8, blocks: Option<(&str, usize)>) -> Checked {
    check_connections(&root.field("connections"), connections)?;
    if slots == 0 || slots > DX7_MOST_SLOTS {
        return Err(fault(&root.field("slots"), Rule::OutOfRange, format!("{slots}: 1 to {DX7_MOST_SLOTS}")));
    }
    let Some((blocks, most)) = blocks else { return Ok(()) };
    let path = root.field("blocks");
    let block_bytes = DX7_SLOT_BYTES.saturating_mul(u16::from(slots));
    let bytes = check_blocks(&path, blocks, block_bytes, most, "blocks")?;
    for (block, slots) in bytes.chunks_exact(usize::from(block_bytes)).enumerate() {
        for (index, slot) in slots.as_chunks::<{ DX7_SLOT_BYTES as usize }>().0.iter().enumerate() {
            if let Some(why) = slot_fault(*slot) {
                return Err(fault(&path, Rule::OutsideProfile, format!("block {block} slot {index} ({slot:02X?}): {why}")));
            }
        }
    }
    Ok(())
}

/// Why a DX7 block's slot is not one the codec takes, if it is not: an
/// empty slot is four zero bytes, and any other is a time within the block
/// and one of the five MIDI messages the instrument is played with.
fn slot_fault([time, status, data1, data2]: [u8; DX7_SLOT_BYTES as usize]) -> Option<&'static str> {
    if [time, status, data1, data2] == [0; DX7_SLOT_BYTES as usize] {
        return None;
    }
    if time > DX7_LAST_TIME {
        return Some("its time is 0 to 199, tenths of a millisecond within the block");
    }
    let seven_bits = data1 <= MIDI_DATA_MOST && data2 <= MIDI_DATA_MOST;
    match status & MIDI_MESSAGE_BITS {
        NOTE_OFF | NOTE_ON if seven_bits => None,
        NOTE_OFF | NOTE_ON => Some("a note's key and velocity are 0 to 127"),
        CONTROL_CHANGE if !CONTROLLERS.contains(&data1) => Some("a control change is of controller 1, 7, 64, 120, 121 or 123"),
        CONTROL_CHANGE if data2 > MIDI_DATA_MOST => Some("a controller's value is 0 to 127"),
        PROGRAM_CHANGE if data1 > LAST_PATCH => Some("a program change chooses patch 0 to 31"),
        PROGRAM_CHANGE if data2 != 0 => Some("a program change's second data byte is 0"),
        PITCH_BEND if !seven_bits => Some("a pitch bend's two data bytes are 0 to 127"),
        CONTROL_CHANGE | PROGRAM_CHANGE | PITCH_BEND => None,
        _ => Some("its status is a note off, note on, control change, program change or pitch bend"),
    }
}

/// A bank of DX7 patches: who it is for, and `patches` as 1 to 32 whole
/// patches whose every byte is one the instrument's bulk dump can carry.
pub(super) fn bank(root: &Path<'_>, connections: &Connections, patches: &str) -> Checked {
    check_connections(&root.field("connections"), connections)?;
    let path = root.field("patches");
    let bytes = check_blocks(&path, patches, DX7_PATCH_BYTES, DX7_MOST_PATCHES, "patches")?;
    for (patch, packed) in bytes.chunks_exact(usize::from(DX7_PATCH_BYTES)).enumerate() {
        if let Some((index, byte)) = packed.iter().enumerate().find(|&(_, &byte)| byte > MIDI_DATA_MOST) {
            return Err(fault(&path, Rule::OutsideProfile, format!("patch {patch} byte {index} is {byte}: at most {MIDI_DATA_MOST}")));
        }
    }
    Ok(())
}
