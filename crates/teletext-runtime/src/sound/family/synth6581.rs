//! The synth6581 family's call site into the engine's interface module
//! (server.md SRV-12, SRV-24, SRV-25, SRV-55, SRV-88): the register data
//! message of a `register_update` is written by the engine's
//! `RegisterData`, and an `asid` message is read by the engine's ASID
//! reader, so that the server and the receiver read the same messages
//! (MIDI-21).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio_instruments::synth6581::interface::{self as engine, Asid, Fault, MOST_MESSAGE_BYTES, RegisterData};

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{RegisterUpdate, Rule, Synth6581Event};
use crate::sound::wire::Wire;

/// Whether the family takes `message`: an ASID message the engine's reader
/// takes as the server must (S6581-54 items 1 to 5; SRV-24).
pub(crate) fn takes(message: &Wire) -> Result<(), String> {
    engine::accepts(&message.message()).map(|_| ()).map_err(|fault| format!("S6581-7, S6581-54, SRV-24: synth6581 does not take this message: {fault:?}"))
}

/// The register data message of a `register_update` (SRV-12), written by
/// the engine: the IDs given in ID order, each value's most-significant bit
/// in its byte (S6581-10 to S6581-13).
pub(crate) fn register_update(update: &RegisterUpdate) -> Result<Vec<Wire>, FamilyFault> {
    let data = RegisterData::from_values(&update.registers.by_id());
    if data.count() == 0 {
        return Err(FamilyFault::new(Rule::TooShort, "SRV-12, S6581-16: an update writes at least one register"));
    }
    let mut message = [0_u8; MOST_MESSAGE_BYTES];
    let length = data.write_midi(&mut message);
    let after_start = message.get(1..length).unwrap_or(&[]);
    Ok(alloc::vec![Wire::Sysex(after_start.to_vec())])
}

/// An `asid` message as one event (SRV-12), read by the engine's ASID
/// reader in its MIDI form, `F0` first (S6581-4 to S6581-18).
pub(crate) fn asid(bytes: &[u8]) -> Result<Vec<Wire>, FamilyFault> {
    let Some((&neoteletext_audio::midi::status::SYSTEM_EXCLUSIVE, after_start)) = bytes.split_first() else {
        return Err(FamilyFault::new(Rule::MalformedMidi, "S6581-4, SRV-12: an asid message begins 0xF0"));
    };
    match engine::accepts(&neoteletext_audio::midi::Message::SystemExclusive(after_start)) {
        Ok(_) => Ok(alloc::vec![Wire::Sysex(after_start.to_vec())]),
        Err(Fault::NotTaken(command)) => {
            Err(FamilyFault::new(Rule::NotTaken, format!("S6581-9, SRV-24: ASID command 0x{command:02X} is not one synth6581 takes")))
        }
        Err(fault) => Err(FamilyFault::new(Rule::MalformedMidi, format!("S6581-5 to S6581-18, S6581-54: the asid message is malformed: {fault:?}"))),
    }
}

/// Whether an event is a full register update, the opening S6581-45
/// requires: IDs 0 to 24 each once and no other (S6581-50), as the engine
/// reads it, given by named fields or as an `asid` message.
pub(crate) fn is_full_update(event: &Synth6581Event) -> bool {
    match event {
        Synth6581Event::RegisterUpdate(update) => RegisterData::from_values(&update.registers.by_id()).is_full_update(),
        Synth6581Event::Asid(message) => {
            let bytes = bytes_of(&message.bytes);
            matches!(engine::read_midi(&bytes), Ok(Asid::RegisterData(data)) if data.is_full_update())
        }
        Synth6581Event::Cue(_) => false,
    }
}

#[cfg(test)]
mod tests;
