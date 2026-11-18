//! synth7's call site into the engine's interface module (server.md
//! SRV-24, SRV-26 to SRV-29, SRV-55 to SRV-57, SRV-60, SRV-63, SRV-88): which
//! messages synth7 takes, its voice packed and checked, its parameter
//! changes and its 32-voice bank written and checked, all by the engine's
//! `synth7::interface` and `synth7::sysex`, the receiver's own code
//! (MIDI-21).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio_instruments::synth7::interface::{self as engine, BuildError, Patch};
use neoteletext_audio_instruments::synth7::patch::{NAME_BYTES, OPERATOR_PARAMETERS, OPERATORS, VOICE_PARAMETERS};
use neoteletext_audio_instruments::synth7::sysex::{self, Kind, Parameter, ParameterChange};

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{MidiChannel, Rule, Synth7Bank, Synth7Family, Synth7Parameter, Synth7ParameterEvent, Synth7Patch, Synth7PatchEvent, Synth7Voice};
use crate::sound::family::Synth;
use crate::sound::synth::FamilyCalls;
use crate::sound::tracker::TrackHint;
use crate::sound::wire::{Wire, channel_bits};

/// The most banks a synth7 sound names (S7-50: programs 0 to 31 reach bank
/// index 0 and 32 to 63 bank index 1; SRV-58): the engine's constant.
pub(crate) const MOST_BANKS: usize = engine::MOST_BANKS;

/// The first voice parameter after the six operators' (S7-28): PITCH EG
/// RATE 1.
const FIRST_GLOBAL_PARAMETER: usize = OPERATORS * OPERATOR_PARAMETERS;
/// The first voice parameter of the name (S7-28): VOICE NAME 1.
const FIRST_NAME_PARAMETER: usize = VOICE_PARAMETERS - NAME_BYTES;

/// Whether synth7 takes `message`, by the engine's table (S7-5, S7-37;
/// SRV-24).
pub(crate) fn takes(message: &Wire) -> Result<(), String> {
    if engine::accepts(&message.message()) { Ok(()) } else { Err(String::from("S7-5, S7-37, SRV-24: synth7 does not act on this message")) }
}

/// The part a system exclusive message addresses: the channel of its
/// sub-status byte, for a single-voice dump or a parameter change (S7-38,
/// S7-43).
pub(crate) fn part_of(bytes: &[u8]) -> Option<u8> {
    match sysex::classify_event(bytes) {
        (Kind::VoiceDump | Kind::ParameterChange, channel) => Some(channel),
        (Kind::BankDump | Kind::Other, _) => None,
    }
}

/// Whether a message is a bank dump to lift (SRV-63): a 32-voice dump, by
/// the engine's classifier; its whole check is the bank check it then
/// meets (SRV-60).
#[allow(clippy::unnecessary_wraps, reason = "`Family::bank_dump` gives every family's answer in one type, and synth53's reader can refuse")]
pub(crate) fn bank_dump(bytes: &[u8]) -> Result<Option<u8>, FamilyFault> {
    Ok(matches!(sysex::classify_event(bytes).0, Kind::BankDump).then_some(0))
}

/// Checks a bank image: exactly one 32-voice dump, every voice in range,
/// by the engine's check (S7-41, S7-42, S7-45 to S7-48; SRV-27, SRV-60).
pub(crate) fn check_bank(image: &[u8]) -> Result<(), FamilyFault> {
    engine::check_bank(image)
        .map_err(|fault| FamilyFault::new(Rule::InvalidBank, format!("S7-41, S7-42, S7-45, SRV-27: the 32-voice dump is refused: {fault:?}")))
}

/// The engine's voice of a voice given by named fields (SRV-26): every
/// field the voice parameter S7-28 numbers, operator n's offset f at
/// `(6 - n) * 21 + f`, the name padded with spaces to ten.
fn voice_of_fields(voice: &Synth7Voice) -> Result<Patch, FamilyFault> {
    let mut parameters = [0_u8; VOICE_PARAMETERS];
    for (number, operator) in (1..=OPERATORS).zip(&voice.operators) {
        let values = operator.eg_rates.iter().chain(&operator.eg_levels).map(|value| value.get()).chain([
            operator.break_point.get(),
            operator.left_depth.get(),
            operator.right_depth.get(),
            operator.left_curve.get(),
            operator.right_curve.get(),
            operator.rate_scaling.get(),
            operator.amplitude_sensitivity.get(),
            operator.velocity_sensitivity.get(),
            operator.output_level.get(),
            operator.oscillator_mode.get(),
            operator.frequency_coarse.get(),
            operator.frequency_fine.get(),
            operator.detune.get(),
        ]);
        let first = OPERATORS.saturating_sub(number).saturating_mul(OPERATOR_PARAMETERS);
        for (place, value) in parameters.iter_mut().skip(first).take(OPERATOR_PARAMETERS).zip(values) {
            *place = value;
        }
    }
    let globals = voice.pitch_eg_rates.iter().chain(&voice.pitch_eg_levels).map(|value| value.get()).chain([
        voice.algorithm.get(),
        voice.feedback.get(),
        voice.oscillator_sync.get(),
        voice.lfo_speed.get(),
        voice.lfo_delay.get(),
        voice.lfo_pitch_depth.get(),
        voice.lfo_amplitude_depth.get(),
        voice.lfo_sync.get(),
        voice.lfo_wave.get(),
        voice.pitch_sensitivity.get(),
        voice.transpose.get(),
    ]);
    for (place, value) in parameters.iter_mut().skip(FIRST_GLOBAL_PARAMETER).zip(globals) {
        *place = value;
    }
    // S7-28, S7-32: the name's characters, then spaces to ten (SRV-26).
    let name = voice.name.bytes().chain(core::iter::repeat(b' '));
    for (place, character) in parameters.iter_mut().skip(FIRST_NAME_PARAMETER).zip(name) {
        *place = character;
    }
    Patch::from_parameters(&parameters).map_err(|fault| FamilyFault::new(Rule::InvalidPatch, format!("S7-28, S7-29, SRV-26: the voice is refused: {fault:?}")))
}

/// The engine's voice of a patch in either form: named fields, or a
/// single-voice dump of any channel checked by the engine (SRV-27).
fn voice_of(patch: &Synth7Patch) -> Result<Patch, FamilyFault> {
    match patch {
        Synth7Patch::Fields(voice) => voice_of_fields(voice),
        Synth7Patch::Bytes(bytes) => engine::check_patch(&bytes_of(&bytes.bytes))
            .map_err(|fault| FamilyFault::new(Rule::InvalidPatch, format!("S7-29, S7-38, S7-39, S7-45, SRV-27: the single-voice dump is refused: {fault:?}"))),
    }
}

/// A named parameter's number and value (SRV-28): operator n's offset f is
/// voice parameter `(6 - n) * 21 + f`, the voice's own 126 to 155, and the
/// function parameters 64 to 77 (S7-28, S7-33).
#[allow(clippy::too_many_lines, reason = "one arm for each of the family's 170 named parameters, as SRV-28 lists them")]
const fn parameter_of(parameter: Synth7Parameter) -> (Parameter, u8) {
    use Synth7Parameter as P;
    match parameter {
        P::op1_eg_rate_1(value) => (Parameter::Voice(105), value.get()),
        P::op1_eg_rate_2(value) => (Parameter::Voice(106), value.get()),
        P::op1_eg_rate_3(value) => (Parameter::Voice(107), value.get()),
        P::op1_eg_rate_4(value) => (Parameter::Voice(108), value.get()),
        P::op1_eg_level_1(value) => (Parameter::Voice(109), value.get()),
        P::op1_eg_level_2(value) => (Parameter::Voice(110), value.get()),
        P::op1_eg_level_3(value) => (Parameter::Voice(111), value.get()),
        P::op1_eg_level_4(value) => (Parameter::Voice(112), value.get()),
        P::op1_break_point(value) => (Parameter::Voice(113), value.get()),
        P::op1_left_depth(value) => (Parameter::Voice(114), value.get()),
        P::op1_right_depth(value) => (Parameter::Voice(115), value.get()),
        P::op1_left_curve(value) => (Parameter::Voice(116), value.get()),
        P::op1_right_curve(value) => (Parameter::Voice(117), value.get()),
        P::op1_rate_scaling(value) => (Parameter::Voice(118), value.get()),
        P::op1_amplitude_sensitivity(value) => (Parameter::Voice(119), value.get()),
        P::op1_velocity_sensitivity(value) => (Parameter::Voice(120), value.get()),
        P::op1_output_level(value) => (Parameter::Voice(121), value.get()),
        P::op1_oscillator_mode(value) => (Parameter::Voice(122), value.get()),
        P::op1_frequency_coarse(value) => (Parameter::Voice(123), value.get()),
        P::op1_frequency_fine(value) => (Parameter::Voice(124), value.get()),
        P::op1_detune(value) => (Parameter::Voice(125), value.get()),
        P::op2_eg_rate_1(value) => (Parameter::Voice(84), value.get()),
        P::op2_eg_rate_2(value) => (Parameter::Voice(85), value.get()),
        P::op2_eg_rate_3(value) => (Parameter::Voice(86), value.get()),
        P::op2_eg_rate_4(value) => (Parameter::Voice(87), value.get()),
        P::op2_eg_level_1(value) => (Parameter::Voice(88), value.get()),
        P::op2_eg_level_2(value) => (Parameter::Voice(89), value.get()),
        P::op2_eg_level_3(value) => (Parameter::Voice(90), value.get()),
        P::op2_eg_level_4(value) => (Parameter::Voice(91), value.get()),
        P::op2_break_point(value) => (Parameter::Voice(92), value.get()),
        P::op2_left_depth(value) => (Parameter::Voice(93), value.get()),
        P::op2_right_depth(value) => (Parameter::Voice(94), value.get()),
        P::op2_left_curve(value) => (Parameter::Voice(95), value.get()),
        P::op2_right_curve(value) => (Parameter::Voice(96), value.get()),
        P::op2_rate_scaling(value) => (Parameter::Voice(97), value.get()),
        P::op2_amplitude_sensitivity(value) => (Parameter::Voice(98), value.get()),
        P::op2_velocity_sensitivity(value) => (Parameter::Voice(99), value.get()),
        P::op2_output_level(value) => (Parameter::Voice(100), value.get()),
        P::op2_oscillator_mode(value) => (Parameter::Voice(101), value.get()),
        P::op2_frequency_coarse(value) => (Parameter::Voice(102), value.get()),
        P::op2_frequency_fine(value) => (Parameter::Voice(103), value.get()),
        P::op2_detune(value) => (Parameter::Voice(104), value.get()),
        P::op3_eg_rate_1(value) => (Parameter::Voice(63), value.get()),
        P::op3_eg_rate_2(value) => (Parameter::Voice(64), value.get()),
        P::op3_eg_rate_3(value) => (Parameter::Voice(65), value.get()),
        P::op3_eg_rate_4(value) => (Parameter::Voice(66), value.get()),
        P::op3_eg_level_1(value) => (Parameter::Voice(67), value.get()),
        P::op3_eg_level_2(value) => (Parameter::Voice(68), value.get()),
        P::op3_eg_level_3(value) => (Parameter::Voice(69), value.get()),
        P::op3_eg_level_4(value) => (Parameter::Voice(70), value.get()),
        P::op3_break_point(value) => (Parameter::Voice(71), value.get()),
        P::op3_left_depth(value) => (Parameter::Voice(72), value.get()),
        P::op3_right_depth(value) => (Parameter::Voice(73), value.get()),
        P::op3_left_curve(value) => (Parameter::Voice(74), value.get()),
        P::op3_right_curve(value) => (Parameter::Voice(75), value.get()),
        P::op3_rate_scaling(value) => (Parameter::Voice(76), value.get()),
        P::op3_amplitude_sensitivity(value) => (Parameter::Voice(77), value.get()),
        P::op3_velocity_sensitivity(value) => (Parameter::Voice(78), value.get()),
        P::op3_output_level(value) => (Parameter::Voice(79), value.get()),
        P::op3_oscillator_mode(value) => (Parameter::Voice(80), value.get()),
        P::op3_frequency_coarse(value) => (Parameter::Voice(81), value.get()),
        P::op3_frequency_fine(value) => (Parameter::Voice(82), value.get()),
        P::op3_detune(value) => (Parameter::Voice(83), value.get()),
        P::op4_eg_rate_1(value) => (Parameter::Voice(42), value.get()),
        P::op4_eg_rate_2(value) => (Parameter::Voice(43), value.get()),
        P::op4_eg_rate_3(value) => (Parameter::Voice(44), value.get()),
        P::op4_eg_rate_4(value) => (Parameter::Voice(45), value.get()),
        P::op4_eg_level_1(value) => (Parameter::Voice(46), value.get()),
        P::op4_eg_level_2(value) => (Parameter::Voice(47), value.get()),
        P::op4_eg_level_3(value) => (Parameter::Voice(48), value.get()),
        P::op4_eg_level_4(value) => (Parameter::Voice(49), value.get()),
        P::op4_break_point(value) => (Parameter::Voice(50), value.get()),
        P::op4_left_depth(value) => (Parameter::Voice(51), value.get()),
        P::op4_right_depth(value) => (Parameter::Voice(52), value.get()),
        P::op4_left_curve(value) => (Parameter::Voice(53), value.get()),
        P::op4_right_curve(value) => (Parameter::Voice(54), value.get()),
        P::op4_rate_scaling(value) => (Parameter::Voice(55), value.get()),
        P::op4_amplitude_sensitivity(value) => (Parameter::Voice(56), value.get()),
        P::op4_velocity_sensitivity(value) => (Parameter::Voice(57), value.get()),
        P::op4_output_level(value) => (Parameter::Voice(58), value.get()),
        P::op4_oscillator_mode(value) => (Parameter::Voice(59), value.get()),
        P::op4_frequency_coarse(value) => (Parameter::Voice(60), value.get()),
        P::op4_frequency_fine(value) => (Parameter::Voice(61), value.get()),
        P::op4_detune(value) => (Parameter::Voice(62), value.get()),
        P::op5_eg_rate_1(value) => (Parameter::Voice(21), value.get()),
        P::op5_eg_rate_2(value) => (Parameter::Voice(22), value.get()),
        P::op5_eg_rate_3(value) => (Parameter::Voice(23), value.get()),
        P::op5_eg_rate_4(value) => (Parameter::Voice(24), value.get()),
        P::op5_eg_level_1(value) => (Parameter::Voice(25), value.get()),
        P::op5_eg_level_2(value) => (Parameter::Voice(26), value.get()),
        P::op5_eg_level_3(value) => (Parameter::Voice(27), value.get()),
        P::op5_eg_level_4(value) => (Parameter::Voice(28), value.get()),
        P::op5_break_point(value) => (Parameter::Voice(29), value.get()),
        P::op5_left_depth(value) => (Parameter::Voice(30), value.get()),
        P::op5_right_depth(value) => (Parameter::Voice(31), value.get()),
        P::op5_left_curve(value) => (Parameter::Voice(32), value.get()),
        P::op5_right_curve(value) => (Parameter::Voice(33), value.get()),
        P::op5_rate_scaling(value) => (Parameter::Voice(34), value.get()),
        P::op5_amplitude_sensitivity(value) => (Parameter::Voice(35), value.get()),
        P::op5_velocity_sensitivity(value) => (Parameter::Voice(36), value.get()),
        P::op5_output_level(value) => (Parameter::Voice(37), value.get()),
        P::op5_oscillator_mode(value) => (Parameter::Voice(38), value.get()),
        P::op5_frequency_coarse(value) => (Parameter::Voice(39), value.get()),
        P::op5_frequency_fine(value) => (Parameter::Voice(40), value.get()),
        P::op5_detune(value) => (Parameter::Voice(41), value.get()),
        P::op6_eg_rate_1(value) => (Parameter::Voice(0), value.get()),
        P::op6_eg_rate_2(value) => (Parameter::Voice(1), value.get()),
        P::op6_eg_rate_3(value) => (Parameter::Voice(2), value.get()),
        P::op6_eg_rate_4(value) => (Parameter::Voice(3), value.get()),
        P::op6_eg_level_1(value) => (Parameter::Voice(4), value.get()),
        P::op6_eg_level_2(value) => (Parameter::Voice(5), value.get()),
        P::op6_eg_level_3(value) => (Parameter::Voice(6), value.get()),
        P::op6_eg_level_4(value) => (Parameter::Voice(7), value.get()),
        P::op6_break_point(value) => (Parameter::Voice(8), value.get()),
        P::op6_left_depth(value) => (Parameter::Voice(9), value.get()),
        P::op6_right_depth(value) => (Parameter::Voice(10), value.get()),
        P::op6_left_curve(value) => (Parameter::Voice(11), value.get()),
        P::op6_right_curve(value) => (Parameter::Voice(12), value.get()),
        P::op6_rate_scaling(value) => (Parameter::Voice(13), value.get()),
        P::op6_amplitude_sensitivity(value) => (Parameter::Voice(14), value.get()),
        P::op6_velocity_sensitivity(value) => (Parameter::Voice(15), value.get()),
        P::op6_output_level(value) => (Parameter::Voice(16), value.get()),
        P::op6_oscillator_mode(value) => (Parameter::Voice(17), value.get()),
        P::op6_frequency_coarse(value) => (Parameter::Voice(18), value.get()),
        P::op6_frequency_fine(value) => (Parameter::Voice(19), value.get()),
        P::op6_detune(value) => (Parameter::Voice(20), value.get()),
        P::pitch_eg_rate_1(value) => (Parameter::Voice(126), value.get()),
        P::pitch_eg_rate_2(value) => (Parameter::Voice(127), value.get()),
        P::pitch_eg_rate_3(value) => (Parameter::Voice(128), value.get()),
        P::pitch_eg_rate_4(value) => (Parameter::Voice(129), value.get()),
        P::pitch_eg_level_1(value) => (Parameter::Voice(130), value.get()),
        P::pitch_eg_level_2(value) => (Parameter::Voice(131), value.get()),
        P::pitch_eg_level_3(value) => (Parameter::Voice(132), value.get()),
        P::pitch_eg_level_4(value) => (Parameter::Voice(133), value.get()),
        P::algorithm(value) => (Parameter::Voice(134), value.get()),
        P::feedback(value) => (Parameter::Voice(135), value.get()),
        P::oscillator_sync(value) => (Parameter::Voice(136), value.get()),
        P::lfo_speed(value) => (Parameter::Voice(137), value.get()),
        P::lfo_delay(value) => (Parameter::Voice(138), value.get()),
        P::lfo_pitch_depth(value) => (Parameter::Voice(139), value.get()),
        P::lfo_amplitude_depth(value) => (Parameter::Voice(140), value.get()),
        P::lfo_sync(value) => (Parameter::Voice(141), value.get()),
        P::lfo_wave(value) => (Parameter::Voice(142), value.get()),
        P::pitch_sensitivity(value) => (Parameter::Voice(143), value.get()),
        P::transpose(value) => (Parameter::Voice(144), value.get()),
        P::name_1(value) => (Parameter::Voice(145), value.get()),
        P::name_2(value) => (Parameter::Voice(146), value.get()),
        P::name_3(value) => (Parameter::Voice(147), value.get()),
        P::name_4(value) => (Parameter::Voice(148), value.get()),
        P::name_5(value) => (Parameter::Voice(149), value.get()),
        P::name_6(value) => (Parameter::Voice(150), value.get()),
        P::name_7(value) => (Parameter::Voice(151), value.get()),
        P::name_8(value) => (Parameter::Voice(152), value.get()),
        P::name_9(value) => (Parameter::Voice(153), value.get()),
        P::name_10(value) => (Parameter::Voice(154), value.get()),
        P::operator_switch(value) => (Parameter::Voice(155), value.get()),
        P::mono_poly(value) => (Parameter::Function(64), value.get()),
        P::pitch_bend_range(value) => (Parameter::Function(65), value.get()),
        P::pitch_bend_step(value) => (Parameter::Function(66), value.get()),
        P::portamento_mode(value) => (Parameter::Function(67), value.get()),
        P::glissando(value) => (Parameter::Function(68), value.get()),
        P::portamento_time(value) => (Parameter::Function(69), value.get()),
        P::modulation_wheel_range(value) => (Parameter::Function(70), value.get()),
        P::modulation_wheel_assign(value) => (Parameter::Function(71), value.get()),
        P::foot_controller_range(value) => (Parameter::Function(72), value.get()),
        P::foot_controller_assign(value) => (Parameter::Function(73), value.get()),
        P::breath_controller_range(value) => (Parameter::Function(74), value.get()),
        P::breath_controller_assign(value) => (Parameter::Function(75), value.get()),
        P::after_touch_range(value) => (Parameter::Function(76), value.get()),
        P::after_touch_assign(value) => (Parameter::Function(77), value.get()),
    }
}

/// The bytes after `F0` of a message the engine wrote.
fn after_start(message: &[u8]) -> Vec<u8> {
    message.split_first().map_or_else(Vec::new, |(_, rest)| rest.to_vec())
}

fn channel(channel: MidiChannel) -> u8 {
    channel_bits(channel)
}

impl FamilyCalls for Synth7Family {
    type Bank = Synth7Bank;

    /// SRV-29: one single-voice dump on the event's channel.
    fn inline_patch(_synth: Synth, event: &Synth7PatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault> {
        let voice = voice_of(&event.patch)?;
        Ok((alloc::vec![Wire::Sysex(after_start(&engine::pack_patch(&voice, channel(event.channel))))], None))
    }

    /// SRV-29: one parameter change (S7-43) on the event's channel.
    fn parameter(_synth: Synth, event: &Synth7ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        let (parameter, value) = parameter_of(event.set);
        let message = sysex::parameter_change(ParameterChange { channel: channel(event.channel), parameter, value })
            .ok_or_else(|| FamilyFault::new(Rule::OutOfRange, format!("S7-29, S7-43: {} is above its parameter's range", event.set.name())))?;
        Ok(alloc::vec![Wire::Sysex(after_start(&message))])
    }

    /// SRV-27, SRV-56, SRV-60: a list of voices built into one 32-voice
    /// dump on channel 0 by the engine, or a dump checked by it.
    fn bank_image(_synth: Synth, bank: &Synth7Bank) -> Result<Vec<u8>, FamilyFault> {
        let image = match bank {
            Synth7Bank::Patches(list) => {
                let voices = list.patches.iter().map(voice_of).collect::<Result<Vec<_>, _>>()?;
                engine::build_bank(&voices)
                    .map_err(|fault| match fault {
                        BuildError::Empty => FamilyFault::new(Rule::TooShort, "SRV-20: a bank holds at least one patch"),
                        BuildError::TooLong => FamilyFault::new(Rule::TooLong, "SRV-27, DEC-6465 answer 18: a synth7 bank holds at most 32 voices"),
                    })?
                    .to_vec()
            }
            Synth7Bank::Bytes(bytes) => bytes_of(&bytes.bytes),
        };
        check_bank(&image)?;
        Ok(image)
    }
}

#[cfg(test)]
mod tests;
