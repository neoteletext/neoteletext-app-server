//! A program's timed events as the MIDI messages the server writes
//! (server.md SRV-10 to SRV-13): each typed event expanded to its
//! messages in SRV-10's order, raw bytes read with running status, and the
//! messages MIDI-8 never carries refused.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::midi::{ChannelMessage, DATA_MAX, Message, STATUS_BIT, controller, data_bytes, status};

use super::family::Deferred;
use crate::protocol::{MidiChannel, ParameterDirection, Rule, SharedEvent};

/// One MIDI message the server writes into a block.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Wire {
    /// A channel message.
    Channel(ChannelMessage),
    /// A system exclusive message: its bytes after `0xF0`, ending `0xF7`.
    Sysex(Vec<u8>),
    /// A message written from its part's tracked state, resolved as its
    /// block is written (SRV-33, SRV-68).
    Deferred(Deferred),
}

/// What a deferred message is as a message before it is resolved: no
/// message, which the engine's writer refuses and its tracker passes over,
/// so that one never resolved cannot be written.
const UNRESOLVED: &[u8] = &[];

impl Wire {
    /// As the engine's writer takes it.
    pub(crate) fn message(&self) -> Message<'_> {
        match self {
            Self::Channel(message) => Message::Channel(*message),
            Self::Sysex(bytes) => Message::SystemExclusive(bytes),
            Self::Deferred(_) => Message::SystemExclusive(UNRESOLVED),
        }
    }
}

/// A message refused: its rule and the detail, beginning with the
/// requirement.
pub(crate) type WireFault = (Rule, String);

/// The value byte written for a parameter step (SRV-11).
const STEP_VALUE: u8 = 0;
/// Bits of a data byte.
const SEVEN_BITS: u32 = 7;

/// The status byte's channel bits for MIDI channel `channel`, 1 to 16:
/// `channel - 1` (SRV-7; MIDI 1.0 4.2, Table I, "N-1, where N = Channel #").
pub(crate) fn channel_bits(channel: MidiChannel) -> u8 {
    channel.get().saturating_sub(1)
}

fn cc(channel: u8, number: u8, value: u8) -> Wire {
    Wire::Channel(ChannelMessage::new(status::CONTROL_CHANGE, channel, number, value))
}

fn high(value: u16) -> u8 {
    u8::try_from(value >> SEVEN_BITS).unwrap_or(DATA_MAX) & DATA_MAX
}

fn low(value: u16) -> u8 {
    u8::try_from(value & u16::from(DATA_MAX)).unwrap_or(0)
}

/// The messages of a shared typed event, in SRV-10's order, or the raw
/// bytes and system exclusive message given, read and checked.
pub(crate) fn expand(event: SharedEvent<'_>, out: &mut Vec<Wire>) -> Result<(), WireFault> {
    match event {
        SharedEvent::NoteOff(event) => {
            out.push(Wire::Channel(ChannelMessage::new(status::NOTE_OFF, channel_bits(event.channel), event.key.get(), event.velocity.get())));
        }
        SharedEvent::NoteOn(event) => {
            out.push(Wire::Channel(ChannelMessage::new(status::NOTE_ON, channel_bits(event.channel), event.key.get(), event.velocity.get())));
        }
        SharedEvent::KeyPressure(event) => {
            out.push(Wire::Channel(ChannelMessage::new(status::KEY_PRESSURE, channel_bits(event.channel), event.key.get(), event.pressure.get())));
        }
        SharedEvent::ControlChange(event) => {
            let wire = cc(channel_bits(event.channel), event.controller.get(), event.value.get());
            never_carried(&wire)?;
            out.push(wire);
        }
        SharedEvent::ProgramChange(event) => {
            let channel = channel_bits(event.channel);
            if let Some(msb) = event.bank_msb {
                out.push(cc(channel, controller::BANK_SELECT, msb.get()));
            }
            if let Some(lsb) = event.bank_lsb {
                out.push(cc(channel, controller::BANK_SELECT_FINE, lsb.get()));
            }
            out.push(Wire::Channel(ChannelMessage::new(status::PROGRAM_CHANGE, channel, event.program.get(), 0)));
        }
        SharedEvent::ChannelPressure(event) => {
            out.push(Wire::Channel(ChannelMessage::new(status::CHANNEL_PRESSURE, channel_bits(event.channel), event.pressure.get(), 0)));
        }
        SharedEvent::PitchBend(event) => {
            let value = event.value.get();
            out.push(Wire::Channel(ChannelMessage::new(status::PITCH_BEND, channel_bits(event.channel), low(value), high(value))));
        }
        SharedEvent::Rpn(number) | SharedEvent::Nrpn(number) => {
            let channel = channel_bits(number.channel);
            let (coarse, fine) = if matches!(event, SharedEvent::Rpn(_)) {
                (controller::RPN_COARSE, controller::RPN_FINE)
            } else {
                (controller::NRPN_COARSE, controller::NRPN_FINE)
            };
            let (parameter, value) = (number.parameter.get(), number.value.get());
            out.extend([
                cc(channel, coarse, high(parameter)),
                cc(channel, fine, low(parameter)),
                cc(channel, controller::DATA_ENTRY, high(value)),
                cc(channel, controller::DATA_ENTRY_FINE, low(value)),
            ]);
        }
        SharedEvent::RpnStep(step) | SharedEvent::NrpnStep(step) => {
            let channel = channel_bits(step.channel);
            let (coarse, fine) = if matches!(event, SharedEvent::RpnStep(_)) {
                (controller::RPN_COARSE, controller::RPN_FINE)
            } else {
                (controller::NRPN_COARSE, controller::NRPN_FINE)
            };
            let parameter = step.parameter.get();
            let way = match step.direction {
                ParameterDirection::Increment => controller::DATA_INCREMENT,
                ParameterDirection::Decrement => controller::DATA_DECREMENT,
            };
            out.extend([cc(channel, coarse, high(parameter)), cc(channel, fine, low(parameter)), cc(channel, way, STEP_VALUE)]);
        }
        SharedEvent::SystemExclusive(event) => out.push(Wire::Sysex(system_exclusive(&crate::protocol::check::bytes_of(&event.bytes))?)),
        SharedEvent::Raw(event) => raw(&crate::protocol::check::bytes_of(&event.bytes), out)?,
    }
    Ok(())
}

/// A whole system exclusive message, `F0` to `F7`, every byte between a
/// data byte (MIDI-27): its bytes after `F0`.
pub(crate) fn system_exclusive(bytes: &[u8]) -> Result<Vec<u8>, WireFault> {
    let Some(after) = bytes.strip_prefix(&[status::SYSTEM_EXCLUSIVE]) else {
        return Err((Rule::MalformedMidi, "MIDI-27: a system exclusive message begins 0xF0".into()));
    };
    let Some((&last, data)) = after.split_last() else { return Err((Rule::MalformedMidi, "MIDI-27: a system exclusive message ends 0xF7".into())) };
    if last != status::END_OF_EXCLUSIVE {
        return Err((Rule::MalformedMidi, "MIDI-27: a system exclusive message ends 0xF7".into()));
    }
    if let Some(offset) = data.iter().position(|&byte| byte > DATA_MAX) {
        return Err((
            Rule::MalformedMidi,
            format!("MIDI-11, MIDI-27: byte {} is 0x{:02X}, not a data byte", offset.saturating_add(1), data.get(offset).copied().unwrap_or(0)),
        ));
    }
    Ok(after.to_vec())
}

/// Refuses a message MIDI-8 never carries in a block: local control
/// (controller 122) among the channel messages.
pub(crate) fn never_carried(wire: &Wire) -> Result<(), WireFault> {
    if let Wire::Channel(message) = wire
        && message.kind() == status::CONTROL_CHANGE
        && message.data1 == controller::LOCAL_CONTROL
    {
        return Err((Rule::NeverCarried, "MIDI-8: local control (controller 122) is never carried".into()));
    }
    Ok(())
}

/// Whole MIDI messages back to back, running status allowed within them
/// (SRV-13; MIDI-12): for channel voice and mode messages only, cleared
/// by a system exclusive or system common message.
pub(crate) fn raw(bytes: &[u8], out: &mut Vec<Wire>) -> Result<(), WireFault> {
    let mut running: Option<u8> = None;
    let mut at = 0_usize;
    while let Some(&first) = bytes.get(at) {
        let (status_byte, data_at) = if first & STATUS_BIT != 0 {
            (first, at.saturating_add(1))
        } else if let Some(running) = running {
            (running, at)
        } else {
            return Err((Rule::RunningStatus, format!("MIDI-12, SRV-13: byte {at} is a data byte where a status is expected")));
        };
        if let Some(count) = data_bytes(status_byte) {
            running = Some(status_byte);
            let data = bytes
                .get(data_at..data_at.saturating_add(count))
                .ok_or_else(|| (Rule::MalformedMidi, format!("MIDI-26: the message at byte {at} is cut short")))?;
            if let Some(offset) = data.iter().position(|&byte| byte > DATA_MAX) {
                return Err((Rule::MalformedMidi, format!("MIDI-11: byte {} is not a data byte", data_at.saturating_add(offset))));
            }
            let message = ChannelMessage { status: status_byte, data1: data.first().copied().unwrap_or(0), data2: data.get(1).copied().unwrap_or(0) };
            let wire = Wire::Channel(message);
            never_carried(&wire)?;
            out.push(wire);
            at = data_at.saturating_add(count);
            continue;
        }
        match status_byte {
            status::SYSTEM_EXCLUSIVE => {
                running = None;
                let rest = bytes.get(at..).unwrap_or(&[]);
                let end = rest
                    .iter()
                    .position(|&byte| byte == status::END_OF_EXCLUSIVE)
                    .ok_or_else(|| (Rule::MalformedMidi, format!("MIDI-27: the system exclusive message at byte {at} never ends")))?;
                let message = rest.get(..=end).unwrap_or(&[]);
                out.push(Wire::Sysex(system_exclusive(message).map_err(|(rule, detail)| (rule, format!("{detail} (at byte {at})")))?));
                at = at.saturating_add(end).saturating_add(1);
            }
            status::END_OF_EXCLUSIVE => return Err((Rule::MalformedMidi, format!("MIDI-27: byte {at} ends a system exclusive message that never began"))),
            other => return Err((Rule::NeverCarried, format!("MIDI-8: 0x{other:02X} at byte {at} is a system message, never carried in a block"))),
        }
    }
    Ok(())
}
