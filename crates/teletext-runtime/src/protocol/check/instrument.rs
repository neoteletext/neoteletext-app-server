//! The instrument sounds' rules (REQ-6413, DEC-6410, DEC-6429): a SID
//! sound's images of the chip's registers, the timed MIDI messages of a DX7
//! sound or of an analogue synthesizer's, and a bank of DX7 patches or of
//! one synthesizer's, each held to what its receiver plays (the engine's
//! REQ-6411), so that a program learns of a fault where it made it and no
//! receiver is sent a block it would refuse, or a patch it would replace
//! with its default.

use alloc::format;

use super::super::{
    Connections, DX7_MOST_PATCHES, DX7_MOST_SLOTS, DX7_PATCH_BYTES, DX7_SLOT_BYTES, LAST_MIDI_KEY, Rule, SID_IMAGE_BYTES, SID_IMAGES_PER_BLOCK,
    SYNTH_MOST_PATCHES, Synth,
};
use super::{Checked, Path, check_base64, check_connections, fault};

mod patch;

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
/// Polyphonic key pressure, which the CS-80 alone takes.
const KEY_PRESSURE: u8 = 0xA0;
/// Channel pressure, which the CS-80 alone takes.
const CHANNEL_PRESSURE: u8 = 0xD0;
/// The bits of a MIDI status byte that are its channel, counted from 0.
const MIDI_CHANNEL_BITS: u8 = 0x0F;
/// The controllers the DX7 codec takes: the modulation wheel (1), volume
/// (7), the sustain pedal (64), all sound off (120), reset all controllers
/// (121) and all notes off (123).
const CONTROLLERS: [u8; 6] = [1, 7, 64, 120, 121, 123];
/// The controllers the Jupiter-8's codec takes beyond [`CONTROLLERS`]:
/// portamento time (5), the balance of its two halves (8) and portamento
/// on or off (65) (the engine's `receiver/instruments/analogue.rs`).
const JUPITER8_CONTROLLERS: [u8; 3] = [5, 8, 65];
/// The controllers the Juno's codec takes beyond [`CONTROLLERS`]: omni off
/// (124), omni on (125), mono on (126) and poly on (127), each of which is to
/// a Juno-106 an all notes off (the engine's
/// `receiver/instruments/analogue.rs`, from Roland's Juno-106 owner's manual,
/// MIDI Implementation 2.1: "Mode messages (123 - 127) are also recognized as
/// ALL NOTES OFF").
const JUNO_CONTROLLERS: [u8; 4] = [124, 125, 126, 127];
/// The MIDI channels, counted from 0, whose program changes the Jupiter-8
/// takes: channel 1 chooses the upper patch and channel 2 the lower.
const JUPITER8_PROGRAM_CHANNELS: u8 = 2;
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
    timed(root, connections, Player::Dx7, slots, blocks)
}

/// An analogue synthesizer's kinds of sound: as a DX7 sound's, each slot
/// held to the messages that instrument's codec takes.
pub(super) fn synth(root: &Path<'_>, connections: &Connections, synth: Synth, slots: u8, blocks: Option<(&str, usize)>) -> Checked {
    timed(root, connections, Player::Synth(synth), slots, blocks)
}

/// A Jupiter-8 sound's split point: a MIDI key.
pub(super) fn split_key(root: &Path<'_>, split_key: u8) -> Checked {
    if split_key > LAST_MIDI_KEY {
        return Err(fault(&root.field("split_key"), Rule::OutOfRange, format!("{split_key}: 0 to {LAST_MIDI_KEY}")));
    }
    Ok(())
}

/// What plays a block of timed MIDI messages, whose codec says which
/// messages it takes.
#[derive(Clone, Copy)]
enum Player {
    /// The DX7.
    Dx7,
    /// One of the analogue synthesizers.
    Synth(Synth),
}

/// The kinds of sound whose blocks are slots of timed MIDI messages: who it
/// is for, slots to a block, and, where the kind carries blocks, `blocks`
/// as 1 to `most` whole blocks whose every slot is empty or a message the
/// codec takes.
fn timed(root: &Path<'_>, connections: &Connections, player: Player, slots: u8, blocks: Option<(&str, usize)>) -> Checked {
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
            if let Some(why) = slot_fault(*slot, player) {
                return Err(fault(&path, Rule::OutsideProfile, format!("block {block} slot {index} ({slot:02X?}): {why}")));
            }
        }
    }
    Ok(())
}

/// Why a block's slot is not one the codec of `player` takes, if it is
/// not: an empty slot is four zero bytes, and any other is a time within
/// the block and one of the MIDI messages the instrument is played with,
/// the DX7's five, and for the CS-80 the two kinds of pressure, and for the
/// Jupiter-8 three more controllers and program changes on its two
/// channels only.
fn slot_fault([time, status, data1, data2]: [u8; DX7_SLOT_BYTES as usize], player: Player) -> Option<&'static str> {
    if [time, status, data1, data2] == [0; DX7_SLOT_BYTES as usize] {
        return None;
    }
    if time > DX7_LAST_TIME {
        return Some("its time is 0 to 199, tenths of a millisecond within the block");
    }
    let seven_bits = data1 <= MIDI_DATA_MOST && data2 <= MIDI_DATA_MOST;
    let message = status & MIDI_MESSAGE_BITS;
    match (player, message) {
        (Player::Synth(Synth::Cs80), KEY_PRESSURE) if seven_bits => return None,
        (Player::Synth(Synth::Cs80), KEY_PRESSURE) => return Some("a key pressure's key and pressure are 0 to 127"),
        (Player::Synth(Synth::Cs80), CHANNEL_PRESSURE) if data1 > MIDI_DATA_MOST => return Some("a channel pressure is 0 to 127"),
        (Player::Synth(Synth::Cs80), CHANNEL_PRESSURE) if data2 != 0 => return Some("a channel pressure's second data byte is 0"),
        (Player::Synth(Synth::Cs80), CHANNEL_PRESSURE) => return None,
        (Player::Synth(Synth::Juno), CONTROL_CHANGE) if JUNO_CONTROLLERS.contains(&data1) => {
            return (data2 > MIDI_DATA_MOST).then_some("a controller's value is 0 to 127");
        }
        (Player::Synth(Synth::Juno), CONTROL_CHANGE) if !CONTROLLERS.contains(&data1) => {
            return Some("a Juno's control change is of controller 1, 7, 64, 120, 121 or 123 to 127");
        }
        (Player::Synth(Synth::Jupiter8), CONTROL_CHANGE) if JUPITER8_CONTROLLERS.contains(&data1) => {
            return (data2 > MIDI_DATA_MOST).then_some("a controller's value is 0 to 127");
        }
        (Player::Synth(Synth::Jupiter8), CONTROL_CHANGE) if !CONTROLLERS.contains(&data1) => {
            return Some("a Jupiter-8's control change is of controller 1, 5, 7, 8, 64, 65, 120, 121 or 123");
        }
        (Player::Synth(Synth::Jupiter8), PROGRAM_CHANGE) if status & MIDI_CHANNEL_BITS >= JUPITER8_PROGRAM_CHANNELS => {
            return Some("a Jupiter-8's program change is on MIDI channel 1, the upper patch, or 2, the lower");
        }
        _ => {}
    }
    match message {
        NOTE_OFF | NOTE_ON if seven_bits => None,
        NOTE_OFF | NOTE_ON => Some("a note's key and velocity are 0 to 127"),
        CONTROL_CHANGE if !CONTROLLERS.contains(&data1) => Some("a control change is of controller 1, 7, 64, 120, 121 or 123"),
        CONTROL_CHANGE if data2 > MIDI_DATA_MOST => Some("a controller's value is 0 to 127"),
        PROGRAM_CHANGE if data1 > LAST_PATCH => Some("a program change chooses patch 0 to 31"),
        PROGRAM_CHANGE if data2 != 0 => Some("a program change's second data byte is 0"),
        PITCH_BEND if !seven_bits => Some("a pitch bend's two data bytes are 0 to 127"),
        CONTROL_CHANGE | PROGRAM_CHANGE | PITCH_BEND => None,
        _ if matches!(player, Player::Synth(Synth::Cs80)) => {
            Some("its status is a note off, note on, key pressure, control change, program change, channel pressure or pitch bend")
        }
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

/// A bank of an analogue synthesizer's patches: who it is for, and
/// `patches` as 1 to 32 whole patches of that instrument, every byte within
/// the range of the control it holds, since a receiver plays its default
/// patch in place of one it cannot read, and says nothing.
pub(super) fn synth_bank(root: &Path<'_>, connections: &Connections, synth: Synth, patches: &str) -> Checked {
    check_connections(&root.field("connections"), connections)?;
    let path = root.field("patches");
    let size = synth.patch_bytes();
    let bytes = check_blocks(&path, patches, size, SYNTH_MOST_PATCHES, "patches")?;
    for (index, one) in bytes.chunks_exact(usize::from(size)).enumerate() {
        if let Err(why) = patch::check(synth, one) {
            return Err(fault(&path, Rule::OutsideProfile, format!("patch {index} {why}")));
        }
    }
    Ok(())
}
