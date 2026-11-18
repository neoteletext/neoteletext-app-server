//! synthD's call site into the engine's interface module,
//! `neoteletext_audio_instruments::synthd::interface` (server.md SRV-24,
//! SRV-34 to SRV-36, SRV-55, SRV-60, SRV-88): which messages synthD takes,
//! its patch packed and checked, its named parameters, and its bank image
//! built and checked, all by the engine, so that nothing here can drift
//! from the receiver (MIDI-21).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::midi::{ChannelMessage, Message, status};
use neoteletext_audio_instruments::synthd::interface::{self as engine, BankFault, LAYOUT, MOST_BANK_BYTES, PATCH_BYTES, PatchFault, Role, byte, pair_bytes};

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, SynthDBank, SynthDFamily, SynthDPanel, SynthDParameter, SynthDParameterEvent, SynthDPatch, SynthDPatchEvent};
use crate::sound::family::Synth;
use crate::sound::synth::FamilyCalls;
use crate::sound::tracker::TrackHint;
use crate::sound::wire::{Wire, channel_bits};

#[cfg(test)]
mod tests;

/// The most banks a synthd sound names (SD-12: no bank select; programs 0 to 127 reach the bank; SRV-58).
/// One: the family reads bank index 0 alone (SD-12; the engine's `bank_patch`), and the engine names no constant for it.
pub(crate) const MOST_BANKS: usize = 1;

/// Whether synthD takes `message`, by the engine's table (SD-1, SRV-24).
pub(crate) fn takes(message: &Wire) -> Result<(), String> {
    if engine::accepts(&message.message()) {
        return Ok(());
    }
    Err(String::from("SD-1, SRV-24: synthD does not act on this message"))
}

/// The engine's patch of the named fields (SRV-34).
pub(crate) const fn patch_of(panel: &SynthDPanel) -> engine::Patch {
    engine::Patch {
        glide: panel.glide.get(),
        oscillator_2_frequency: panel.oscillator_2_frequency.get(),
        oscillator_3_frequency: panel.oscillator_3_frequency.get(),
        external_input_volume: panel.external_input_volume.get(),
        oscillator_1_volume: panel.oscillator_1_volume.get(),
        oscillator_2_volume: panel.oscillator_2_volume.get(),
        oscillator_3_volume: panel.oscillator_3_volume.get(),
        noise_volume: panel.noise_volume.get(),
        cutoff_frequency: panel.cutoff_frequency.get(),
        emphasis: panel.emphasis.get(),
        filter_attack_time: panel.filter_attack_time.get(),
        filter_decay_time: panel.filter_decay_time.get(),
        filter_sustain_level: panel.filter_sustain_level.get(),
        amount_of_contour: panel.amount_of_contour.get(),
        loudness_attack_time: panel.loudness_attack_time.get(),
        loudness_decay_time: panel.loudness_decay_time.get(),
        loudness_sustain_level: panel.loudness_sustain_level.get(),
        glide_switch: panel.glide_switch.get(),
        oscillator_1_range_octave: panel.oscillator_1_range_octave.get(),
        oscillator_2_range_octave: panel.oscillator_2_range_octave.get(),
        oscillator_3_range_octave: panel.oscillator_3_range_octave.get(),
        oscillator_3_control: panel.oscillator_3_control.get(),
        oscillator_3_range_lo_hi: panel.oscillator_3_range_lo_hi.get(),
        external_input_switch: panel.external_input_switch.get(),
        oscillator_1_switch: panel.oscillator_1_switch.get(),
        oscillator_2_switch: panel.oscillator_2_switch.get(),
        oscillator_3_switch: panel.oscillator_3_switch.get(),
        noise_switch: panel.noise_switch.get(),
        decay_switch: panel.decay_switch.get(),
        tune: panel.tune.get(),
        modulation_mix: panel.modulation_mix.get(),
        oscillator_1_range_lo_hi: panel.oscillator_1_range_lo_hi.get(),
        oscillator_1_waveform: panel.oscillator_1_waveform.get(),
        oscillator_2_range_lo_hi: panel.oscillator_2_range_lo_hi.get(),
        oscillator_2_waveform: panel.oscillator_2_waveform.get(),
        oscillator_3_waveform: panel.oscillator_3_waveform.get(),
        main_output_volume: panel.main_output_volume.get(),
        oscillator_modulation: panel.oscillator_modulation.get(),
        noise_colour: panel.noise_colour.get(),
        filter_modulation: panel.filter_modulation.get(),
        keyboard_control_1: panel.keyboard_control_1.get(),
        keyboard_control_2: panel.keyboard_control_2.get(),
    }
}

fn patch_fault(fault: PatchFault) -> FamilyFault {
    let detail = match fault {
        PatchFault::Length(length) => format!("SRV-35, SD-22: a synthD patch is {PATCH_BYTES} bytes, not {length}"),
        PatchFault::Byte { at, value } => format!("SRV-35, SD-22: byte {at} is {value}, above 127"),
    };
    FamilyFault::new(Rule::InvalidPatch, detail)
}

fn bank_fault(fault: BankFault) -> FamilyFault {
    let detail = match fault {
        BankFault::Empty => String::from("SRV-35, SD-27: a bank holds 1 to 128 patches; none given"),
        BankFault::TooManyPatches(count) => format!("SRV-35, SD-27: {count} patches; a bank holds at most 128"),
        BankFault::Length(length) => format!("SRV-35, SD-27: {length} bytes is not 1 to 128 patches of {PATCH_BYTES} bytes"),
        BankFault::Byte { program, at, value } => format!("SRV-35, SD-27: patch {program}'s byte {at} is {value}, above 127"),
        BankFault::Room(length) => format!("SRV-60, MIDI-55: the bank needs {length} bytes, more than {MOST_BANK_BYTES}"),
    };
    FamilyFault::new(Rule::InvalidBank, detail)
}

/// The patch's 60 values: packed by the engine from the fields, or the
/// bytes checked by the engine and carried unchanged (SRV-34, SRV-35).
fn values_of(patch: &SynthDPatch) -> Result<[u8; PATCH_BYTES], FamilyFault> {
    match patch {
        SynthDPatch::Fields(panel) => Ok(engine::pack_patch(&patch_of(panel))),
        SynthDPatch::Bytes(bytes) => engine::check_values(&bytes_of(&bytes.bytes)).map_err(patch_fault),
    }
}

/// Checks a bank image with the engine's bank reader (SRV-35, SRV-60).
pub(crate) fn check_bank(image: &[u8]) -> Result<(), FamilyFault> {
    engine::check_bank(image).map(|_| ()).map_err(bank_fault)
}

/// What a named parameter moves: a pair's coarse byte with its 14-bit
/// value, or a single control's byte with its value (SRV-36).
enum Move {
    /// A coarse and fine pair.
    Pair(usize, u16),
    /// One controller.
    Single(usize, u8),
}

fn move_of(parameter: SynthDParameter) -> Move {
    use Move::{Pair, Single};
    use SynthDParameter as P;
    match parameter {
        P::glide(value) => Pair(byte::GLIDE, value.get()),
        P::oscillator_2_frequency(value) => Pair(byte::OSCILLATOR_2_FREQUENCY, value.get()),
        P::oscillator_3_frequency(value) => Pair(byte::OSCILLATOR_3_FREQUENCY, value.get()),
        P::external_input_volume(value) => Pair(byte::EXTERNAL_INPUT_VOLUME, value.get()),
        P::oscillator_1_volume(value) => Pair(byte::OSCILLATOR_1_VOLUME, value.get()),
        P::oscillator_2_volume(value) => Pair(byte::OSCILLATOR_2_VOLUME, value.get()),
        P::oscillator_3_volume(value) => Pair(byte::OSCILLATOR_3_VOLUME, value.get()),
        P::noise_volume(value) => Pair(byte::NOISE_VOLUME, value.get()),
        P::cutoff_frequency(value) => Pair(byte::CUTOFF_FREQUENCY, value.get()),
        P::emphasis(value) => Pair(byte::EMPHASIS, value.get()),
        P::filter_attack_time(value) => Pair(byte::FILTER_ATTACK_TIME, value.get()),
        P::filter_decay_time(value) => Pair(byte::FILTER_DECAY_TIME, value.get()),
        P::filter_sustain_level(value) => Pair(byte::FILTER_SUSTAIN_LEVEL, value.get()),
        P::amount_of_contour(value) => Pair(byte::AMOUNT_OF_CONTOUR, value.get()),
        P::loudness_attack_time(value) => Pair(byte::LOUDNESS_ATTACK_TIME, value.get()),
        P::loudness_decay_time(value) => Pair(byte::LOUDNESS_DECAY_TIME, value.get()),
        P::loudness_sustain_level(value) => Pair(byte::LOUDNESS_SUSTAIN_LEVEL, value.get()),
        P::glide_switch(value) => Single(byte::GLIDE_SWITCH, value.get()),
        P::oscillator_1_range_octave(value) => Single(byte::OSCILLATOR_1_RANGE_OCTAVE, value.get()),
        P::oscillator_2_range_octave(value) => Single(byte::OSCILLATOR_2_RANGE_OCTAVE, value.get()),
        P::oscillator_3_range_octave(value) => Single(byte::OSCILLATOR_3_RANGE_OCTAVE, value.get()),
        P::oscillator_3_control(value) => Single(byte::OSCILLATOR_3_CONTROL, value.get()),
        P::oscillator_3_range_lo_hi(value) => Single(byte::OSCILLATOR_3_RANGE_LO_HI, value.get()),
        P::external_input_switch(value) => Single(byte::EXTERNAL_INPUT_SWITCH, value.get()),
        P::oscillator_1_switch(value) => Single(byte::OSCILLATOR_1_SWITCH, value.get()),
        P::oscillator_2_switch(value) => Single(byte::OSCILLATOR_2_SWITCH, value.get()),
        P::oscillator_3_switch(value) => Single(byte::OSCILLATOR_3_SWITCH, value.get()),
        P::noise_switch(value) => Single(byte::NOISE_SWITCH, value.get()),
        P::decay_switch(value) => Single(byte::DECAY_SWITCH, value.get()),
        P::tune(value) => Pair(byte::TUNE, value.get()),
        P::modulation_mix(value) => Single(byte::MODULATION_MIX, value.get()),
        P::oscillator_1_range_lo_hi(value) => Single(byte::OSCILLATOR_1_RANGE_LO_HI, value.get()),
        P::oscillator_1_waveform(value) => Single(byte::OSCILLATOR_1_WAVEFORM, value.get()),
        P::oscillator_2_range_lo_hi(value) => Single(byte::OSCILLATOR_2_RANGE_LO_HI, value.get()),
        P::oscillator_2_waveform(value) => Single(byte::OSCILLATOR_2_WAVEFORM, value.get()),
        P::oscillator_3_waveform(value) => Single(byte::OSCILLATOR_3_WAVEFORM, value.get()),
        P::main_output_volume(value) => Single(byte::MAIN_OUTPUT_VOLUME, value.get()),
        P::oscillator_modulation(value) => Single(byte::OSCILLATOR_MODULATION, value.get()),
        P::noise_colour(value) => Single(byte::NOISE_COLOUR, value.get()),
        P::filter_modulation(value) => Single(byte::FILTER_MODULATION, value.get()),
        P::keyboard_control_1(value) => Single(byte::KEYBOARD_CONTROL_1, value.get()),
        P::keyboard_control_2(value) => Single(byte::KEYBOARD_CONTROL_2, value.get()),
    }
}

/// The controller of the patch's byte `at` (SD-22's table).
fn controller_at(at: usize) -> Option<(u8, Role)> {
    LAYOUT.get(at).copied()
}

fn control(channel: u8, number: u8, value: u8) -> Wire {
    Wire::Channel(ChannelMessage::new(status::CONTROL_CHANGE, channel, number, value))
}

impl FamilyCalls for SynthDFamily {
    type Bank = SynthDBank;

    /// SD-26: an inline patch is its 60 control changes, in the patch's
    /// order, on the event's channel, at one tick.
    fn inline_patch(_synth: Synth, event: &SynthDPatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault> {
        let values = values_of(&event.patch)?;
        let mut wires = Vec::with_capacity(PATCH_BYTES);
        engine::control_changes(&values, channel_bits(event.channel), &mut |message| {
            if let Message::Channel(message) = message {
                wires.push(Wire::Channel(message));
            }
        });
        Ok((wires, None))
    }

    /// SRV-36: a single control as its control change, a pair as its two,
    /// coarse then fine (MIDI-37 (b), MIDI-75).
    fn parameter(_synth: Synth, event: &SynthDParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        let channel = channel_bits(event.channel);
        let unknown = || FamilyFault::new(Rule::NotTaken, format!("SD-22: {} names no control of the engine's layout", event.set.name()));
        match move_of(event.set) {
            Move::Pair(at, value) => {
                let (Some((coarse, Role::Coarse)), Some((fine, Role::Fine))) = (controller_at(at), controller_at(at.saturating_add(1))) else {
                    return Err(unknown());
                };
                let [high, low] = pair_bytes(value);
                Ok(alloc::vec![control(channel, coarse, high), control(channel, fine, low)])
            }
            Move::Single(at, value) => {
                let Some((number, Role::Single)) = controller_at(at) else { return Err(unknown()) };
                Ok(alloc::vec![control(channel, number, value)])
            }
        }
    }

    fn bank_image(_synth: Synth, bank: &SynthDBank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            SynthDBank::Patches(list) => {
                let patches =
                    list.patches.iter().map(|patch| values_of(patch).map(|values| engine::Patch::from_values(&values))).collect::<Result<Vec<_>, _>>()?;
                let mut image = alloc::vec![0_u8; MOST_BANK_BYTES];
                let length = engine::build_bank(&patches, &mut image).map_err(bank_fault)?;
                image.truncate(length);
                Ok(image)
            }
            SynthDBank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}
