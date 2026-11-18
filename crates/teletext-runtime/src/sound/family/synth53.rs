//! synth53's call site into the engine's interface module (server.md
//! SRV-24, SRV-55, SRV-60, SRV-88).
//!
//! TEMPORARY: it waits for the engine's branch `family-synth53` (finished: `synth53::interface::{accepts, Fields, Variant, pack_patch(&Fields, Variant), check_patch, check_bank, build_bank, Part}` under the feature `interface`; its tracker is `Part::new(Variant)`, `take(&Message, &Banks)`, `restate(channel, ..)`; reserved and padding bytes of a program are carried, never refused, as S53-33 says) to be merged into the pinned engine, which
//! brings `synth53::interface::{Program, pack, unpack, edit_buffer_dump, program_dump, read_sysex, Dumps}` (worktree family-synth53).
//! Until it is merged and the pin moved, a patch or bank given as named
//! fields and every named parameter are refused as unavailable, and the
//! bytes forms are carried after the generic checks only.

#![allow(clippy::unnecessary_wraps, reason = "a temporary call site keeps the signature of the engine function that replaces it, which can fail")]

use alloc::vec::Vec;

use neoteletext_audio::midi::restate::SysexKind;

use super::{FamilyFault, sysex_patch};
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, Synth53Bank, Synth53Family, Synth53ParameterEvent, Synth53Patch, Synth53PatchEvent};
use crate::sound::synth::FamilyCalls;
use crate::sound::wire::Wire;

/// The most banks a synth53 sound names (S53-13, S53-26: one bank a group, groups 0 to 9; SRV-58).
/// TEMPORARY: the engine's constant once its interface module is merged.
pub(crate) const MOST_BANKS: usize = 10;

/// Whether synth53 takes `message`. TEMPORARY: every message the block
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

impl FamilyCalls for Synth53Family {
    type Bank = Synth53Bank;

    fn inline_patch(event: &Synth53PatchEvent) -> Result<Vec<Wire>, FamilyFault> {
        match &event.patch {
            Synth53Patch::Fields(_) => Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth53", "patch packer")),
            Synth53Patch::Bytes(bytes) => sysex_patch(&bytes_of(&bytes.bytes)),
        }
    }

    fn parameter(_event: &Synth53ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        Err(FamilyFault::unavailable(Rule::NotTaken, "synth53", "parameter writer"))
    }

    fn bank_image(bank: &Synth53Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth53Bank::Patches(_) => Err(FamilyFault::unavailable(Rule::InvalidBank, "synth53", "bank builder")),
            Synth53Bank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}
