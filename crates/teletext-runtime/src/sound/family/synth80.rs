//! synth80's call site into the engine's interface module (server.md
//! SRV-24, SRV-55, SRV-60, SRV-88).
//!
//! TEMPORARY: it waits for the engine's branch `family-synth80` (finished: `synth80::interface::{accepts, pack_patch, check_patch, build_bank, check_bank, preset, Tracker}` with `Tracker::inline_patch`) to be merged into the pinned engine, which
//! brings `synth80::interface::{accepts, pack_patch, check_patch, build_bank, check_bank, patch_messages, preset, Tracker}` and `synth80::presets::PRESETS` (worktree family-synth80).
//! Until it is merged and the pin moved, a patch or bank given as named
//! fields and every named parameter are refused as unavailable, and the
//! bytes forms are carried after the generic checks only.

#![allow(clippy::unnecessary_wraps, reason = "a temporary call site keeps the signature of the engine function that replaces it, which can fail")]

use alloc::vec::Vec;

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, Synth80Bank, Synth80Family, Synth80ParameterEvent, Synth80Patch, Synth80PatchEvent};
use crate::sound::synth::FamilyCalls;
use crate::sound::wire::Wire;

/// The most banks a synth80 sound names (S80-36: controller 0 values 0 to 2 choose among three banks; SRV-58).
/// TEMPORARY: the engine's constant once its interface module is merged.
pub(crate) const MOST_BANKS: usize = 3;

/// Whether synth80 takes `message`. TEMPORARY: every message the block
/// grammar carries, until the engine's table of the family is pinned.
pub(crate) const fn takes(_message: &Wire) -> Result<(), alloc::string::String> {
    Ok(())
}

/// Checks a bank image (SRV-60). TEMPORARY: carried after the generic
/// bounds only, until the engine's bank reader is pinned.
pub(crate) const fn check_bank(_image: &[u8]) -> Result<(), FamilyFault> {
    Ok(())
}

impl FamilyCalls for Synth80Family {
    type Bank = Synth80Bank;

    fn inline_patch(event: &Synth80PatchEvent) -> Result<Vec<Wire>, FamilyFault> {
        match &event.patch {
            Synth80Patch::Fields(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth80", "patch packer")),
            Synth80Patch::Bytes(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth80", "inline patch writer")),
        }
    }

    fn parameter(_event: &Synth80ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        Err(FamilyFault::unavailable(Rule::NotTaken, "synth80", "parameter writer"))
    }

    fn bank_image(bank: &Synth80Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth80Bank::Patches(_) => Err(FamilyFault::unavailable(Rule::InvalidBank, "synth80", "bank builder")),
            Synth80Bank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}
