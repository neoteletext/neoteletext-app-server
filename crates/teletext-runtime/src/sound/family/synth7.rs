//! synth7's call site into the engine's interface module (server.md
//! SRV-24, SRV-55, SRV-60, SRV-88).
//!
//! TEMPORARY: it waits for the engine's branch `family-synth7`, which
//! brings `synth7::interface::{accepts, Voice, pack_patch, check_patch, build_bank, check_bank, Tracker}` and its parameter change (S7-43).
//! Until it is merged and the pin moved, a patch or bank given as named
//! fields and every named parameter are refused as unavailable, and the
//! bytes forms are carried after the generic checks only.

#![allow(clippy::unnecessary_wraps, reason = "a temporary call site keeps the signature of the engine function that replaces it, which can fail")]

use alloc::vec::Vec;

use super::{FamilyFault, sysex_patch};
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, Synth7Bank, Synth7Family, Synth7ParameterEvent, Synth7Patch, Synth7PatchEvent};
use crate::sound::family::Synth;
use crate::sound::synth::FamilyCalls;
use crate::sound::tracker::TrackHint;
use crate::sound::wire::Wire;

/// The most banks a synth7 sound names (S7-50: programs 0 to 31 reach bank index 0 and 32 to 63 bank index 1; SRV-58).
/// The engine's constant.
pub(crate) const MOST_BANKS: usize = neoteletext_audio_instruments::synth7::interface::MOST_BANKS;

/// Whether synth7 takes `message`. TEMPORARY: every message the block
/// grammar carries, until the engine's table of the family is pinned.
pub(crate) const fn takes(_message: &Wire) -> Result<(), alloc::string::String> {
    Ok(())
}

/// The part a system exclusive message addresses (S7-38, S7-43).
pub(crate) const fn part_of(_bytes: &[u8]) -> Option<u8> {
    None
}

/// Whether a message is a bank dump to lift (SRV-63). TEMPORARY: a
/// message longer than an event, which only a bank dump may be, is refused
/// as unavailable until the engine's bank reader is pinned; a shorter one
/// is none.
pub(crate) fn bank_dump(bytes: &[u8]) -> Result<Option<u8>, FamilyFault> {
    if bytes.len() > neoteletext_audio::midi::MOST_SYSTEM_EXCLUSIVE_BYTES {
        return Err(FamilyFault::unavailable(Rule::InvalidBank, "synth7", "bank dump reader"));
    }
    Ok(None)
}

/// Checks a bank image (SRV-60). TEMPORARY: carried after the generic
/// bounds only, until the engine's bank reader is pinned.
pub(crate) const fn check_bank(_image: &[u8]) -> Result<(), FamilyFault> {
    Ok(())
}

impl FamilyCalls for Synth7Family {
    type Bank = Synth7Bank;

    fn inline_patch(_synth: Synth, event: &Synth7PatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault> {
        let wires = match &event.patch {
            Synth7Patch::Fields(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth7", "patch packer")),
            Synth7Patch::Bytes(bytes) => sysex_patch(&bytes_of(&bytes.bytes)),
        }?;
        Ok((wires, None))
    }

    fn parameter(_synth: Synth, _event: &Synth7ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        Err(FamilyFault::unavailable(Rule::NotTaken, "synth7", "parameter writer"))
    }

    fn bank_image(_synth: Synth, bank: &Synth7Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth7Bank::Patches(_) => Err(FamilyFault::unavailable(Rule::InvalidBank, "synth7", "bank builder")),
            Synth7Bank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}
