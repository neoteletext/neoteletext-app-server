//! synth8's call site into the engine's interface module (server.md
//! SRV-24, SRV-55, SRV-60, SRV-88).
//!
//! TEMPORARY: it waits for the engine's branch `family-synth8` (finished: `synth8::interface::{accepts, pack_patch, check_patch, build_bank, check_bank, Tracker}`) to be merged into the pinned engine, which
//! brings `synth8::interface::{accepts, pack_patch, check_patch, build_bank, check_bank, Tracker}` (worktree family-synth8).
//! Until it is merged and the pin moved, a patch or bank given as named
//! fields and every named parameter are refused as unavailable, and the
//! bytes forms are carried after the generic checks only.

#![allow(clippy::unnecessary_wraps, reason = "a temporary call site keeps the signature of the engine function that replaces it, which can fail")]

use alloc::vec::Vec;

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, Synth8Bank, Synth8Family, Synth8ParameterEvent, Synth8Patch, Synth8PatchEvent};
use crate::sound::synth::FamilyCalls;
use crate::sound::wire::Wire;

/// The most banks a synth8 sound names (S8-29: one bank of up to 64 patches; SRV-58).
/// TEMPORARY: the engine's constant once its interface module is merged.
pub(crate) const MOST_BANKS: usize = 1;

/// Whether synth8 takes `message`. TEMPORARY: every message the block
/// grammar carries, until the engine's table of the family is pinned.
pub(crate) const fn takes(_message: &Wire) -> Result<(), alloc::string::String> {
    Ok(())
}

/// Checks a bank image (SRV-60). TEMPORARY: carried after the generic
/// bounds only, until the engine's bank reader is pinned.
pub(crate) const fn check_bank(_image: &[u8]) -> Result<(), FamilyFault> {
    Ok(())
}

impl FamilyCalls for Synth8Family {
    type Bank = Synth8Bank;

    fn inline_patch(event: &Synth8PatchEvent) -> Result<Vec<Wire>, FamilyFault> {
        match &event.patch {
            Synth8Patch::Fields(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth8", "patch packer")),
            Synth8Patch::Bytes(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth8", "inline patch writer")),
        }
    }

    fn parameter(_event: &Synth8ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        Err(FamilyFault::unavailable(Rule::NotTaken, "synth8", "parameter writer"))
    }

    fn bank_image(bank: &Synth8Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth8Bank::Patches(_) => Err(FamilyFault::unavailable(Rule::InvalidBank, "synth8", "bank builder")),
            Synth8Bank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}
