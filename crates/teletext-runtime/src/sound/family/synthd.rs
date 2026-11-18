//! synthd's call site into the engine's interface module (server.md
//! SRV-24, SRV-55, SRV-60, SRV-88).
//!
//! TEMPORARY: it waits for the engine's branch `family-synthd` (finished at 8b58d822: `synthd::interface::{Patch, pack_patch, check_patch, check_values, build_bank, check_bank, accepts, control_changes, Tracker}`, feature `interface`) to be merged into the pinned engine, which
//! brings `synthd::interface::{accepts, Patch, pack_patch, check_patch, build_bank, check_bank, control_changes, Tracker}` (worktree family-synthd).
//! Until it is merged and the pin moved, a patch or bank given as named
//! fields and every named parameter are refused as unavailable, and the
//! bytes forms are carried after the generic checks only.

#![allow(clippy::unnecessary_wraps, reason = "a temporary call site keeps the signature of the engine function that replaces it, which can fail")]

use alloc::vec::Vec;

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, SynthDBank, SynthDFamily, SynthDParameterEvent, SynthDPatch, SynthDPatchEvent};
use crate::sound::synth::FamilyCalls;
use crate::sound::wire::Wire;

/// The most banks a synthd sound names (SD-12: no bank select; programs 0 to 127 reach the bank; SRV-58).
/// TEMPORARY: the engine's constant once its interface module is merged.
pub(crate) const MOST_BANKS: usize = 1;

/// Whether synthd takes `message`. TEMPORARY: every message the block
/// grammar carries, until the engine's table of the family is pinned.
pub(crate) const fn takes(_message: &Wire) -> Result<(), alloc::string::String> {
    Ok(())
}

/// Checks a bank image (SRV-60). TEMPORARY: carried after the generic
/// bounds only, until the engine's bank reader is pinned.
pub(crate) const fn check_bank(_image: &[u8]) -> Result<(), FamilyFault> {
    Ok(())
}

impl FamilyCalls for SynthDFamily {
    type Bank = SynthDBank;

    fn inline_patch(event: &SynthDPatchEvent) -> Result<Vec<Wire>, FamilyFault> {
        match &event.patch {
            SynthDPatch::Fields(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synthd", "patch packer")),
            SynthDPatch::Bytes(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synthd", "inline patch writer")),
        }
    }

    fn parameter(_event: &SynthDParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        Err(FamilyFault::unavailable(Rule::NotTaken, "synthd", "parameter writer"))
    }

    fn bank_image(bank: &SynthDBank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            SynthDBank::Patches(_) => Err(FamilyFault::unavailable(Rule::InvalidBank, "synthd", "bank builder")),
            SynthDBank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}
