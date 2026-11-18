//! synth106's call site into the engine's interface module (server.md
//! SRV-24, SRV-55, SRV-60, SRV-88).
//!
//! TEMPORARY: it waits for the engine's branch `family-synth106` (finished: `synth106::interface::{accepts, pack_patch, check_patch, build_bank, check_bank, Tracker}`) to be merged into the pinned engine, which
//! brings `synth106::interface::{read_message, pack_patch, check_patch, check_fields, parameter_message, ju06a}` (worktree family-synth106).
//! Until it is merged and the pin moved, a patch or bank given as named
//! fields and every named parameter are refused as unavailable, and the
//! bytes forms are carried after the generic checks only.

#![allow(clippy::unnecessary_wraps, reason = "a temporary call site keeps the signature of the engine function that replaces it, which can fail")]

use alloc::vec::Vec;

use neoteletext_audio::midi::restate::SysexKind;

use super::{FamilyFault, sysex_patch};
use crate::protocol::check::bytes_of;
use crate::protocol::{
    Rule, Synth60Bank, Synth60Family, Synth60ParameterEvent, Synth60Patch, Synth60PatchEvent, Synth106Bank, Synth106Family, Synth106ParameterEvent,
    Synth106Patch, Synth106PatchEvent,
};
use crate::sound::synth::FamilyCalls;
use crate::sound::wire::Wire;

/// The most banks a synth106 sound names (S106-10: no bank select; programs 0 to 127 reach bank index 0; SRV-58).
/// TEMPORARY: the engine's constant once its interface module is merged.
pub(crate) const MOST_BANKS: usize = 1;

/// Whether synth106 takes `message`. TEMPORARY: every message the block
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

/// Checks a bank image (SRV-60). TEMPORARY: carried after the generic
/// bounds only, until the engine's bank reader is pinned.
pub(crate) const fn check_bank(_image: &[u8]) -> Result<(), FamilyFault> {
    Ok(())
}

impl FamilyCalls for Synth106Family {
    type Bank = Synth106Bank;

    fn inline_patch(event: &Synth106PatchEvent) -> Result<Vec<Wire>, FamilyFault> {
        match &event.patch {
            Synth106Patch::Fields(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth106", "patch packer")),
            Synth106Patch::Bytes(bytes) => sysex_patch(&bytes_of(&bytes.bytes)),
        }
    }

    fn parameter(_event: &Synth106ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        Err(FamilyFault::unavailable(Rule::NotTaken, "synth106", "parameter writer"))
    }

    fn bank_image(bank: &Synth106Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth106Bank::Patches(_) => Err(FamilyFault::unavailable(Rule::InvalidBank, "synth106", "bank builder")),
            Synth106Bank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}

impl FamilyCalls for Synth60Family {
    type Bank = Synth60Bank;

    fn inline_patch(event: &Synth60PatchEvent) -> Result<Vec<Wire>, FamilyFault> {
        match &event.patch {
            Synth60Patch::Fields(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth106", "patch packer")),
            Synth60Patch::Bytes(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth106", "inline patch writer")),
        }
    }

    fn parameter(_event: &Synth60ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        Err(FamilyFault::unavailable(Rule::NotTaken, "synth106", "parameter writer"))
    }

    fn bank_image(bank: &Synth60Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth60Bank::Patches(_) => Err(FamilyFault::unavailable(Rule::InvalidBank, "synth106", "bank builder")),
            Synth60Bank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}
