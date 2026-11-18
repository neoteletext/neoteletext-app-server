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

use neoteletext_audio::midi::restate::SysexKind;

use super::{FamilyFault, sysex_patch};
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, Synth7Bank, Synth7Family, Synth7ParameterEvent, Synth7Patch, Synth7PatchEvent};
use crate::sound::synth::FamilyCalls;
use crate::sound::wire::Wire;

/// The most banks a synth7 sound names (S7-50: programs 0 to 31 reach bank index 0 and 32 to 63 bank index 1; SRV-58).
/// TEMPORARY: the engine's constant once its interface module is merged.
pub(crate) const MOST_BANKS: usize = 2;

/// Whether synth7 takes `message`. TEMPORARY: every message the block
/// grammar carries, until the engine's table of the family is pinned.
pub(crate) const fn takes(_message: &Wire) -> Result<(), alloc::string::String> {
    Ok(())
}

/// What a system exclusive message is to the tracker. TEMPORARY: none,
/// until the engine's tracker is pinned, so an inline patch is not yet
/// restated.
pub(crate) const fn sysex_kind(_bytes: &[u8]) -> Option<(u8, SysexKind)> {
    None
}

/// Whether a message is a bank dump to lift (SRV-63). TEMPORARY: none,
/// until the engine's bank reader is pinned.
pub(crate) const fn bank_dump(_bytes: &[u8]) -> Option<u8> {
    None
}

/// Checks a bank image (SRV-60). TEMPORARY: carried after the generic
/// bounds only, until the engine's bank reader is pinned.
pub(crate) const fn check_bank(_image: &[u8]) -> Result<(), FamilyFault> {
    Ok(())
}

impl FamilyCalls for Synth7Family {
    type Bank = Synth7Bank;

    fn inline_patch(event: &Synth7PatchEvent) -> Result<Vec<Wire>, FamilyFault> {
        match &event.patch {
            Synth7Patch::Fields(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth7", "patch packer")),
            Synth7Patch::Bytes(bytes) => sysex_patch(&bytes_of(&bytes.bytes)),
        }
    }

    fn parameter(_event: &Synth7ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        Err(FamilyFault::unavailable(Rule::NotTaken, "synth7", "parameter writer"))
    }

    fn bank_image(bank: &Synth7Bank) -> Result<Vec<u8>, FamilyFault> {
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
