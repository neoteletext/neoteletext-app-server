//! The synth53 family's call site into the engine's interface module
//! (server.md SRV-24, SRV-37 to SRV-39, SRV-55, SRV-60, SRV-63, SRV-88):
//! which messages synth53, synth52 and synth51 take, a program as named
//! fields or an edit buffer dump, written inline as the edit buffer dump
//! of the sound's variant, the named parameters as their NRPN, and a bank
//! of one group's program dumps. Reserved and padding bytes of a program
//! given as bytes are carried as they are, never refused (S53-33, which
//! governs where SRV-38 asks them to be 0). Every table is the engine's
//! (MIDI-21).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::midi::{ChannelMessage, DATA_MAX, controller, status};
use neoteletext_audio_instruments::synth53::interface::{self as engine, EDIT_BUFFER_DUMP_BYTES, Fields, GROUP_PROGRAMS, PROGRAM_DUMP_BYTES, Program, Sysex};

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, Synth53Bank, Synth53Family, Synth53Parameter, Synth53ParameterEvent, Synth53Patch, Synth53PatchEvent, Synth53Program, U8In};
use crate::sound::family::Synth;
use crate::sound::synth::FamilyCalls;
use crate::sound::tracker::TrackHint;
use crate::sound::wire::{Wire, channel_bits};

#[cfg(test)]
mod tests;

/// The most banks a synth53 sound names (S53-13, S53-26: one bank a group, groups 0 to 9; SRV-58).
/// The engine's constant.
pub(crate) const MOST_BANKS: usize = engine::GROUPS as usize;

/// The bits of a data byte (MIDI 1.0 4.2): an NRPN number or value is
/// sent seven bits at a time (MIDI-37 (c)).
const SEVEN_BITS: u8 = 7;

/// Whether the family takes `message` (S53-2, SRV-24), by the engine's
/// table: a program dump travels only in a bank (S53-28).
pub(crate) fn takes(message: &Wire) -> Result<(), String> {
    if engine::accepts(&message.message()) { Ok(()) } else { Err(String::from("S53-2, S53-28: the synth53 family does not act on this message")) }
}

/// Whether a message is a program dump, which a stored sound's music lifts
/// into the bank of its group (SRV-63; S53-28): `Some` with the group.
/// A dump whose group cannot be read is refused.
pub(crate) fn bank_dump(bytes: &[u8]) -> Result<Option<u8>, FamilyFault> {
    match engine::read_sysex(bytes) {
        Sysex::ProgramDump { group, .. } | Sysex::FaultyProgramDump { group: Some(group), .. } => Ok(Some(group)),
        Sysex::FaultyProgramDump { group: None, .. } => Err(FamilyFault::new(Rule::InvalidBank, "S53-28: a program dump without its group byte")),
        Sysex::EditBuffer(_) | Sysex::FaultyEditBuffer | Sysex::Other => Ok(None),
    }
}

/// Checks a bank image (SRV-38, SRV-60; S53-26 (a), S53-28) with the
/// engine's reader.
pub(crate) fn check_bank(image: &[u8]) -> Result<(), FamilyFault> {
    engine::check_bank(image).map(|_| ()).map_err(|error| {
        FamilyFault::new(Rule::InvalidBank, format!("S53-26, S53-28, S53-33: a bank is 1 to 40 program dumps of one group, each program once: {error:?}"))
    })
}

/// The engine's fields of named fields (SRV-37).
fn fields_of(program: &Synth53Program) -> Fields {
    Fields {
        osc_a_frequency: program.osc_a_frequency.get(),
        osc_b_frequency: program.osc_b_frequency.get(),
        osc_b_fine_tune: program.osc_b_fine_tune.get(),
        osc_a_saw: program.osc_a_saw.get(),
        osc_a_square: program.osc_a_square.get(),
        osc_b_saw: program.osc_b_saw.get(),
        osc_b_tri: program.osc_b_tri.get(),
        osc_b_square: program.osc_b_square.get(),
        osc_a_pulse_width: program.osc_a_pulse_width.get(),
        osc_b_pulse_width: program.osc_b_pulse_width.get(),
        osc_sync: program.osc_sync.get(),
        osc_b_low_freq: program.osc_b_low_freq.get(),
        osc_b_keyboard: program.osc_b_keyboard.get(),
        glide_rate: program.glide_rate.get(),
        osc_a_level: program.osc_a_level.get(),
        osc_b_level: program.osc_b_level.get(),
        noise_level: program.noise_level.get(),
        cutoff: program.cutoff.get(),
        resonance: program.resonance.get(),
        filter_keyboard_track: program.filter_keyboard_track.get(),
        lfo_frequency: program.lfo_frequency.get(),
        lfo_initial_amount: program.lfo_initial_amount.get(),
        lfo_saw: program.lfo_saw.get(),
        lfo_tri: program.lfo_tri.get(),
        lfo_square: program.lfo_square.get(),
        lfo_source_mix: program.lfo_source_mix.get(),
        lfo_freq_a: program.lfo_freq_a.get(),
        lfo_freq_b: program.lfo_freq_b.get(),
        lfo_freq_pw_a: program.lfo_freq_pw_a.get(),
        lfo_freq_pw_b: program.lfo_freq_pw_b.get(),
        lfo_filter: program.lfo_filter.get(),
        poly_mod_filt_env_amount: program.poly_mod_filt_env_amount.get(),
        poly_mod_osc_b_amount: program.poly_mod_osc_b_amount.get(),
        poly_mod_freq_a: program.poly_mod_freq_a.get(),
        poly_mod_pw: program.poly_mod_pw.get(),
        poly_mod_filter: program.poly_mod_filter.get(),
        vintage: program.vintage.get(),
        aftertouch_to_filter: program.aftertouch_to_filter.get(),
        aftertouch_to_lfo: program.aftertouch_to_lfo.get(),
        env_filter_amount: program.env_filter_amount.get(),
        velocity_to_filter: program.velocity_to_filter.get(),
        velocity_to_amp: program.velocity_to_amp.get(),
        attack_filter: program.attack_filter.get(),
        attack_vca: program.attack_vca.get(),
        decay_filter: program.decay_filter.get(),
        decay_vca: program.decay_vca.get(),
        sustain_filter: program.sustain_filter.get(),
        sustain_vca: program.sustain_vca.get(),
        release_filter: program.release_filter.get(),
        release_vca: program.release_vca.get(),
        release_switch: program.release_switch.get(),
        unison: program.unison.get(),
        unison_voice_count: program.unison_voice_count.get(),
        unison_detune: program.unison_detune.get(),
        unison_notes: program.unison_notes.map(U8In::get),
        pitch_wheel_range: program.pitch_wheel_range.get(),
        retrigger_and_unison: program.retrigger_and_unison.get(),
    }
}

/// The program a patch holds for the synthesizer's variant (SRV-37,
/// SRV-38): named fields with FILTER REV SELECT from the kind (S53-38),
/// or an edit buffer dump as the engine checks it.
fn program_of(synth: Synth, patch: &Synth53Patch) -> Result<Program, FamilyFault> {
    match patch {
        Synth53Patch::Fields(program) => fields_of(program).program(synth.synth53_variant()).map_err(|error| {
            FamilyFault::new(Rule::InvalidPatch, format!("S53-33: parameter {} is {}, above its maximum {}", error.parameter, error.value, error.maximum))
        }),
        Synth53Patch::Bytes(bytes) => engine::check_patch(&bytes_of(&bytes.bytes))
            .map_err(|error| FamilyFault::new(Rule::InvalidPatch, format!("S53-29, S53-33, SRV-38: not an edit buffer dump of 157 bytes: {error:?}"))),
    }
}

/// A named parameter's program parameter number and value (SRV-39): every
/// parameter but 20, the ten UNISON NOTE parameters 55 to 64.
#[allow(clippy::too_many_lines, reason = "one arm for each of the 66 parameters of SRV-39")]
pub(crate) const fn number_of_parameter(parameter: Synth53Parameter) -> (u8, u8) {
    use Synth53Parameter as P;
    match parameter {
        P::osc_a_frequency(value) => (0, value.get()),
        P::osc_b_frequency(value) => (1, value.get()),
        P::osc_b_fine_tune(value) => (2, value.get()),
        P::osc_a_saw(value) => (3, value.get()),
        P::osc_a_square(value) => (4, value.get()),
        P::osc_b_saw(value) => (5, value.get()),
        P::osc_b_tri(value) => (6, value.get()),
        P::osc_b_square(value) => (7, value.get()),
        P::osc_a_pulse_width(value) => (8, value.get()),
        P::osc_b_pulse_width(value) => (9, value.get()),
        P::osc_sync(value) => (10, value.get()),
        P::osc_b_low_freq(value) => (11, value.get()),
        P::osc_b_keyboard(value) => (12, value.get()),
        P::glide_rate(value) => (13, value.get()),
        P::osc_a_level(value) => (14, value.get()),
        P::osc_b_level(value) => (15, value.get()),
        P::noise_level(value) => (16, value.get()),
        P::cutoff(value) => (17, value.get()),
        P::resonance(value) => (18, value.get()),
        P::filter_keyboard_track(value) => (19, value.get()),
        P::lfo_frequency(value) => (21, value.get()),
        P::lfo_initial_amount(value) => (22, value.get()),
        P::lfo_saw(value) => (23, value.get()),
        P::lfo_tri(value) => (24, value.get()),
        P::lfo_square(value) => (25, value.get()),
        P::lfo_source_mix(value) => (26, value.get()),
        P::lfo_freq_a(value) => (27, value.get()),
        P::lfo_freq_b(value) => (28, value.get()),
        P::lfo_freq_pw_a(value) => (29, value.get()),
        P::lfo_freq_pw_b(value) => (30, value.get()),
        P::lfo_filter(value) => (31, value.get()),
        P::poly_mod_filt_env_amount(value) => (32, value.get()),
        P::poly_mod_osc_b_amount(value) => (33, value.get()),
        P::poly_mod_freq_a(value) => (34, value.get()),
        P::poly_mod_pw(value) => (35, value.get()),
        P::poly_mod_filter(value) => (36, value.get()),
        P::vintage(value) => (37, value.get()),
        P::aftertouch_to_filter(value) => (38, value.get()),
        P::aftertouch_to_lfo(value) => (39, value.get()),
        P::env_filter_amount(value) => (40, value.get()),
        P::velocity_to_filter(value) => (41, value.get()),
        P::velocity_to_amp(value) => (42, value.get()),
        P::attack_filter(value) => (43, value.get()),
        P::attack_vca(value) => (44, value.get()),
        P::decay_filter(value) => (45, value.get()),
        P::decay_vca(value) => (46, value.get()),
        P::sustain_filter(value) => (47, value.get()),
        P::sustain_vca(value) => (48, value.get()),
        P::release_filter(value) => (49, value.get()),
        P::release_vca(value) => (50, value.get()),
        P::release_switch(value) => (51, value.get()),
        P::unison(value) => (52, value.get()),
        P::unison_voice_count(value) => (53, value.get()),
        P::unison_detune(value) => (54, value.get()),
        P::unison_note_1(value) => (55, value.get()),
        P::unison_note_2(value) => (56, value.get()),
        P::unison_note_3(value) => (57, value.get()),
        P::unison_note_4(value) => (58, value.get()),
        P::unison_note_5(value) => (59, value.get()),
        P::unison_note_6(value) => (60, value.get()),
        P::unison_note_7(value) => (61, value.get()),
        P::unison_note_8(value) => (62, value.get()),
        P::unison_note_9(value) => (63, value.get()),
        P::unison_note_10(value) => (64, value.get()),
        P::pitch_wheel_range(value) => (86, value.get()),
        P::retrigger_and_unison(value) => (87, value.get()),
    }
}

impl FamilyCalls for Synth53Family {
    type Bank = Synth53Bank;

    /// MIDI-60, S53-47: the edit buffer dump of the program, which the
    /// tracker takes as the part's inline patch (S53-46).
    fn inline_patch(synth: Synth, event: &Synth53PatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault> {
        let program = program_of(synth, &event.patch)?;
        let dump: [u8; EDIT_BUFFER_DUMP_BYTES] = engine::edit_buffer_dump(&program);
        let after_start = dump.get(1..).unwrap_or(&[]).to_vec();
        Ok((alloc::vec![Wire::Sysex(after_start)], None))
    }

    /// SRV-39: controllers 99 and 98, the parameter number's high and low
    /// seven bits, then 6 and 38, the value's.
    fn parameter(_synth: Synth, event: &Synth53ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        let (number, value) = number_of_parameter(event.set);
        let channel = channel_bits(event.channel);
        let cc = |number: u8, value: u8| Wire::Channel(ChannelMessage::new(status::CONTROL_CHANGE, channel, number, value));
        Ok(alloc::vec![
            cc(controller::NRPN_COARSE, number >> SEVEN_BITS),
            cc(controller::NRPN_FINE, number & DATA_MAX),
            cc(controller::DATA_ENTRY, value >> SEVEN_BITS),
            cc(controller::DATA_ENTRY_FINE, value & DATA_MAX),
        ])
    }

    /// SRV-38: a group's programs written as its program dumps, or 1 to 40
    /// program dumps as given.
    fn bank_image(synth: Synth, bank: &Synth53Bank) -> Result<Vec<u8>, FamilyFault> {
        let image = match bank {
            Synth53Bank::Patches(list) => {
                let programs = list.patches.iter().map(|patch| program_of(synth, patch)).collect::<Result<Vec<_>, _>>()?;
                let mut out = alloc::vec![0_u8; PROGRAM_DUMP_BYTES * usize::from(GROUP_PROGRAMS)];
                let length = engine::build_bank(list.group.get(), &programs, &mut out)
                    .map_err(|error| FamilyFault::new(Rule::InvalidBank, format!("S53-26, SRV-38: 1 to 40 programs of a group 0 to 9: {error:?}")))?;
                out.truncate(length);
                out
            }
            Synth53Bank::Bytes(bytes) => bytes_of(&bytes.bytes),
        };
        check_bank(&image)?;
        Ok(image)
    }
}
