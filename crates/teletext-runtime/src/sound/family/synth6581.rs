//! The synth6581 family's call site into the engine's interface module
//! (server.md SRV-12, SRV-24, SRV-25, SRV-55, SRV-88).
//!
//! TEMPORARY: it waits for the engine's branch `family-synth6581` (finished: `synth6581::interface::{accepts, pack_patch, check_patch, Tracker, BlockBudget}`) to be merged into the pinned engine, which
//! brings `synth6581::interface::{RegisterData, read_midi, accepts,
//! BlockBudget, Tracker}` (the register data message of S6581-10 to
//! S6581-13, the ASID reader, the block's budget of S6581-42 and the
//! restatement of S6581-46). Until it is merged and the pin moved, a
//! `register_update` is refused as unavailable, and an `asid` message is
//! carried after the generic checks only (one whole system exclusive
//! message of the block grammar).

#![allow(clippy::unnecessary_wraps, reason = "a temporary call site keeps the signature of the engine function that replaces it, which can fail")]

use alloc::vec::Vec;

use super::{FamilyFault, whole_message};
use crate::protocol::{RegisterUpdate, Rule};
use crate::sound::wire::Wire;

/// Whether the family takes `message`. TEMPORARY: every message the block
/// grammar carries, until the engine's ASID reader is pinned.
pub(crate) const fn takes(_message: &Wire) -> Result<(), alloc::string::String> {
    Ok(())
}

/// The register data message of a `register_update` (SRV-12), written by
/// the engine's packer. TEMPORARY: unavailable.
pub(crate) fn register_update(_update: &RegisterUpdate) -> Result<Vec<Wire>, FamilyFault> {
    Err(FamilyFault::unavailable(Rule::InvalidPatch, "synth6581", "register data writer"))
}

/// An `asid` message as one event (SRV-12). TEMPORARY: the generic checks
/// only, until the engine's ASID reader (S6581-10 to S6581-18) is pinned.
pub(crate) fn asid(bytes: &[u8]) -> Result<Vec<Wire>, FamilyFault> {
    whole_message(bytes).map(|after| alloc::vec![Wire::Sysex(after)]).map_err(|fault| FamilyFault::new(Rule::MalformedMidi, fault.detail))
}

/// Whether an event is a full register update, the opening S6581-45
/// requires: the 25 registers of IDs 0 to 24 given and none of 25 to 27
/// (S6581-50). An `asid` message is taken as one TEMPORARILY, until the
/// engine's reader can say what it writes.
pub(crate) fn is_full_update(update: Option<&RegisterUpdate>) -> bool {
    /// The IDs a full update writes (S6581-50).
    const FULL_UPDATE_IDS: usize = 25;
    let Some(update) = update else { return true };
    let by_id = update.registers.by_id();
    by_id.iter().take(FULL_UPDATE_IDS).all(Option::is_some) && by_id.iter().skip(FULL_UPDATE_IDS).all(Option::is_none)
}
