//! synth8's call site into the engine's interface module,
//! `neoteletext_audio_instruments::synth8::interface` (server.md SRV-24,
//! SRV-40 to SRV-42, SRV-55, SRV-60, SRV-88): which messages synth8 takes
//! and on which channels, its patch packed and checked, its named
//! parameters by the engine's controller of each control, and its bank
//! image built and checked, all by the engine (MIDI-21).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::midi::{ChannelMessage, status};
use neoteletext_audio_instruments::synth8::interface::{self as engine, BankError, MOST_BANK_BYTES, PATCH_BYTES, Param, Patch, PatchError};

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{Rule, Synth8Bank, Synth8Family, Synth8Panel, Synth8Parameter, Synth8ParameterEvent, Synth8Patch, Synth8PatchEvent};
use crate::sound::family::Synth;
use crate::sound::synth::FamilyCalls;
use crate::sound::tracker::TrackHint;
use crate::sound::wire::{Wire, channel_bits};

#[cfg(test)]
mod tests;

/// The most banks a synth8 sound names (S8-29: one bank of up to 64 patches; SRV-58).
/// The engine's constant.
pub(crate) const MOST_BANKS: usize = neoteletext_audio_instruments::synth8::interface::BANK_INDEX + 1;

/// Whether synth8 takes `message`, and on its channel, by the engine's
/// table (S8-1, S8-2, SRV-24): `Err` names the requirement.
pub(crate) fn takes(message: &Wire) -> Result<(), String> {
    engine::accepts(&message.message()).map_err(|refusal| format!("{}, SRV-24: synth8 does not act on this message here", refusal.requirement))
}

/// The control named `name` (SRV-40: the fields are named as the engine
/// names its controls).
fn param_named(name: &str) -> Option<Param> {
    Param::ALL.into_iter().find(|param| param.name() == name)
}

/// The named fields' values, by name (SRV-40).
const fn named_values(panel: &Synth8Panel) -> [(&'static str, u8); PATCH_BYTES] {
    [
        ("lfo_rate", panel.lfo_rate.get()),
        ("lfo_delay_time", panel.lfo_delay_time.get()),
        ("lfo_waveform", panel.lfo_waveform.get()),
        ("vco_lfo_mod", panel.vco_lfo_mod.get()),
        ("vco_env_mod", panel.vco_env_mod.get()),
        ("vco_modulator", panel.vco_modulator.get()),
        ("pulse_width_mod", panel.pulse_width_mod.get()),
        ("pulse_width_mod_source", panel.pulse_width_mod_source.get()),
        ("cross_mod", panel.cross_mod.get()),
        ("vco1_range", panel.vco1_range.get()),
        ("vco1_waveform", panel.vco1_waveform.get()),
        ("vco2_sync", panel.vco2_sync.get()),
        ("vco2_low_freq", panel.vco2_low_freq.get()),
        ("vco2_range", panel.vco2_range.get()),
        ("vco2_fine_tune", panel.vco2_fine_tune.get()),
        ("vco2_waveform", panel.vco2_waveform.get()),
        ("vco1_level", panel.vco1_level.get()),
        ("vco2_level", panel.vco2_level.get()),
        ("hpf_cutoff", panel.hpf_cutoff.get()),
        ("vcf_cutoff", panel.vcf_cutoff.get()),
        ("vcf_resonance", panel.vcf_resonance.get()),
        ("vcf_slope", panel.vcf_slope.get()),
        ("vcf_env_mod", panel.vcf_env_mod.get()),
        ("vcf_envelope_select", panel.vcf_envelope_select.get()),
        ("vcf_lfo_mod", panel.vcf_lfo_mod.get()),
        ("vcf_key_follow", panel.vcf_key_follow.get()),
        ("vca_level", panel.vca_level.get()),
        ("vca_lfo_mod", panel.vca_lfo_mod.get()),
        ("env1_attack", panel.env1_attack.get()),
        ("env1_decay", panel.env1_decay.get()),
        ("env1_sustain", panel.env1_sustain.get()),
        ("env1_release", panel.env1_release.get()),
        ("env1_polarity", panel.env1_polarity.get()),
        ("env2_attack", panel.env2_attack.get()),
        ("env2_decay", panel.env2_decay.get()),
        ("env2_sustain", panel.env2_sustain.get()),
        ("env2_release", panel.env2_release.get()),
        ("assign_mode", panel.assign_mode.get()),
        ("env1_key_follow", panel.env1_key_follow.get()),
        ("env2_key_follow", panel.env2_key_follow.get()),
        ("vco1_bend", panel.vco1_bend.get()),
        ("vco2_bend", panel.vco2_bend.get()),
    ]
}

/// The engine's patch of the named fields (SRV-40, SRV-55).
pub(crate) fn patch_of(panel: &Synth8Panel) -> Result<Patch, FamilyFault> {
    let mut patch = Patch::DEFAULT;
    for (name, value) in named_values(panel) {
        let param = param_named(name).ok_or_else(|| FamilyFault::new(Rule::InvalidPatch, format!("S8-23: the engine names no control {name}")))?;
        patch.set(param, value);
    }
    Ok(patch)
}

fn patch_fault(error: PatchError) -> FamilyFault {
    let detail = match error {
        PatchError::Length(length) => format!("SRV-41, S8-23: a synth8 patch is {PATCH_BYTES} bytes, not {length}"),
        PatchError::Value(at) => format!("SRV-41, S8-23: byte {at} is above 127"),
    };
    FamilyFault::new(Rule::InvalidPatch, detail)
}

fn bank_fault(error: BankError) -> FamilyFault {
    let detail = match error {
        BankError::Empty => String::from("SRV-41, S8-29 (a): a bank holds 1 to 64 patches; none given"),
        BankError::TooMany(count) => format!("SRV-41, S8-29 (a): {count} patches; a bank holds at most 64"),
        BankError::Length(length) => format!("SRV-41, S8-29 (a): {length} bytes is not a whole number of {PATCH_BYTES}-byte patches"),
        BankError::Room(length) => format!("SRV-60, MIDI-55: the bank needs {length} bytes, more than {MOST_BANK_BYTES}"),
        BankError::Patch { index, error } => match error {
            PatchError::Length(length) => format!("SRV-41, S8-29 (a): patch {index} is {length} bytes"),
            PatchError::Value(at) => format!("SRV-41, S8-29 (a): patch {index}'s byte {at} is above 127"),
        },
    };
    FamilyFault::new(Rule::InvalidBank, detail)
}

/// The engine's patch of either form: packed from the fields, or the bytes
/// checked by the engine and carried unchanged (SRV-40, SRV-41).
fn engine_patch(patch: &Synth8Patch) -> Result<Patch, FamilyFault> {
    match patch {
        Synth8Patch::Fields(panel) => patch_of(panel),
        Synth8Patch::Bytes(bytes) => engine::check_patch(&bytes_of(&bytes.bytes)).map_err(patch_fault),
    }
}

/// Checks a bank image with the engine's bank reader (SRV-41, SRV-60).
pub(crate) fn check_bank(image: &[u8]) -> Result<(), FamilyFault> {
    engine::check_bank(image).map(|_| ()).map_err(bank_fault)
}

const fn value_of(parameter: Synth8Parameter) -> u8 {
    use Synth8Parameter as P;
    match parameter {
        P::lfo_rate(value)
        | P::lfo_delay_time(value)
        | P::lfo_waveform(value)
        | P::vco_lfo_mod(value)
        | P::vco_env_mod(value)
        | P::vco_modulator(value)
        | P::pulse_width_mod(value)
        | P::pulse_width_mod_source(value)
        | P::cross_mod(value)
        | P::vco1_range(value)
        | P::vco1_waveform(value)
        | P::vco2_sync(value)
        | P::vco2_low_freq(value)
        | P::vco2_range(value)
        | P::vco2_fine_tune(value)
        | P::vco2_waveform(value)
        | P::vco1_level(value)
        | P::vco2_level(value)
        | P::hpf_cutoff(value)
        | P::vcf_cutoff(value)
        | P::vcf_resonance(value)
        | P::vcf_slope(value)
        | P::vcf_env_mod(value)
        | P::vcf_envelope_select(value)
        | P::vcf_lfo_mod(value)
        | P::vcf_key_follow(value)
        | P::vca_level(value)
        | P::vca_lfo_mod(value)
        | P::env1_attack(value)
        | P::env1_decay(value)
        | P::env1_sustain(value)
        | P::env1_release(value)
        | P::env1_polarity(value)
        | P::env2_attack(value)
        | P::env2_decay(value)
        | P::env2_sustain(value)
        | P::env2_release(value)
        | P::assign_mode(value)
        | P::env1_key_follow(value)
        | P::env2_key_follow(value)
        | P::vco1_bend(value)
        | P::vco2_bend(value) => value.get(),
    }
}

fn control(channel: u8, number: u8, value: u8) -> Wire {
    Wire::Channel(ChannelMessage::new(status::CONTROL_CHANGE, channel, number, value))
}

impl FamilyCalls for Synth8Family {
    type Bank = Synth8Bank;

    /// SRV-42, S8-27: an inline patch is its 42 control changes in byte
    /// order on its channel, at one tick; a channel but 1 and 2 is refused
    /// by the engine's table as each is checked (S8-2).
    fn inline_patch(_synth: Synth, event: &Synth8PatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault> {
        let patch = engine_patch(&event.patch)?;
        let channel = channel_bits(event.channel);
        let wires = Param::ALL.into_iter().zip(engine::pack_patch(&patch)).map(|(param, value)| control(channel, param.controller(), value)).collect();
        Ok((wires, None))
    }

    /// SRV-42: the control change of the parameter's controller on the
    /// event's channel.
    fn parameter(_synth: Synth, event: &Synth8ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        let name = event.set.name();
        let param = param_named(name).ok_or_else(|| FamilyFault::new(Rule::NotTaken, format!("S8-23: the engine names no control {name}")))?;
        Ok(alloc::vec![control(channel_bits(event.channel), param.controller(), value_of(event.set))])
    }

    fn bank_image(_synth: Synth, bank: &Synth8Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth8Bank::Patches(list) => {
                let patches = list.patches.iter().map(engine_patch).collect::<Result<Vec<_>, _>>()?;
                let mut image = alloc::vec![0_u8; MOST_BANK_BYTES];
                let length = engine::build_bank(&patches, &mut image).map_err(bank_fault)?;
                image.truncate(length);
                Ok(image)
            }
            Synth8Bank::Bytes(bytes) => {
                let image = bytes_of(&bytes.bytes);
                check_bank(&image)?;
                Ok(image)
            }
        }
    }
}
