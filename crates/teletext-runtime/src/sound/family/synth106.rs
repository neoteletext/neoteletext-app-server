//! synth106's and synth60's call site into the engine's interface module
//! (server.md SRV-24, SRV-30 to SRV-33, SRV-55, SRV-60, SRV-88): which
//! messages each model takes, its patch built from named fields and packed,
//! a patch given as bytes checked, its banks built and checked, and its
//! named parameters written. Every table, packer and check is the engine's
//! (`synth106::interface`), so the server and the receiver read one
//! implementation (MIDI-21).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::midi::{ChannelMessage, MOST_BANK_PATCHES, status};
use neoteletext_audio_instruments::synth106::interface::{self as engine, ChorusMode, Model, PATCH_MESSAGE_BYTES, Patch, PwmSource, Range, Switch, Tracker};

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{
    Rule, Synth60Bank, Synth60Chorus, Synth60Family, Synth60Panel, Synth60Parameter, Synth60ParameterEvent, Synth60Patch, Synth60PatchEvent, Synth60Pwm,
    Synth106Bank, Synth106Chorus, Synth106Family, Synth106Panel, Synth106Parameter, Synth106ParameterEvent, Synth106Patch, Synth106PatchEvent,
    Synth106Polarity, Synth106Pwm, Synth106Range, Synth106Vca,
};
use crate::sound::family::{Deferred, Synth};
use crate::sound::synth::FamilyCalls;
use crate::sound::tracker::TrackHint;
use crate::sound::wire::{Wire, channel_bits};

#[cfg(test)]
mod tests;

/// The most banks a synth106 sound names (S106-10: no bank select; programs 0 to 127 reach bank index 0; SRV-58).
/// One: the family reads bank index 0 alone (S106-10; the engine's `Tracker::start` and `bank_panel`), and the engine names no constant for it.
pub(crate) const MOST_BANKS: usize = 1;

/// The program number an inline patch's function type `30` message
/// carries in byte e: the receiver does not use it (S106-16), and an inline
/// patch has no program, so the server writes one value for byte-identical
/// sounds (MIDI-44).
const INLINE_PROGRAM: u8 = 0;
/// The channel a bank built from patches is written on: its messages are
/// read by position, never by channel (S106-2, S106-38), so one value
/// keeps banks byte-identical (MIDI-44).
const BANK_CHANNEL: u8 = 0;
/// The parameter number of the first switch byte (S106-13: byte 16).
const FIRST_SWITCHES: u8 = 16;
/// The parameter number of the second switch byte (S106-13: byte 17).
const SECOND_SWITCHES: u8 = 17;
/// The bytes of one control change after a synth60 patch's message
/// (S106-31: "as 9 bytes", three of three).
const CONTROL_BYTES: usize = 3;

/// Whether `synth` (synth106 or synth60) takes `message` (SRV-24), by the
/// engine's table of the model.
pub(crate) fn takes(synth: Synth, message: &Wire) -> Result<(), String> {
    if engine::accepts(synth.model(), &message.message()) {
        return Ok(());
    }
    Err(format!("SRV-24, S106-1 to S106-30: {} does not act on this message", name(synth.model())))
}

/// The model's name, for refusals.
const fn name(model: Model) -> &'static str {
    match model {
        Model::Synth106 => "synth106",
        Model::Synth60 => "synth60",
    }
}

/// Checks a bank image of the model as the receiver's bank reader reads it
/// (SRV-32, SRV-60; S106-38).
pub(crate) fn check_bank(synth: Synth, image: &[u8]) -> Result<(), FamilyFault> {
    engine::check_bank(synth.model(), image)
        .map(|_| ())
        .map_err(|refusal| FamilyFault::new(Rule::InvalidBank, format!("SRV-32, S106-38: byte {} of the bank image breaks {:?}", refusal.byte, refusal.rule)))
}

/// A named switch parameter of synth106 on `channel` (0 to 15): one
/// function type `32` message whose value is the whole switch byte of the
/// panel as the tracker holds it at the message's tick, with the one field
/// changed (SRV-33 item 1, SRV-68).
pub(crate) fn switch_message(tracker: Option<&Tracker>, channel: u8, parameter: Synth106Parameter) -> Result<Vec<Wire>, FamilyFault> {
    let tracker = tracker.ok_or_else(|| FamilyFault::new(Rule::NotTaken, "SRV-33, SRV-68: a synth106 switch parameter outside a synth106 sound"))?;
    let mut panel = *tracker.panel();
    let Some(number) = set_synth106_switch(&mut panel, parameter) else {
        return Err(FamilyFault::new(Rule::NotTaken, format!("SRV-33: {} is not a switch", parameter.name())));
    };
    let value = panel.parameter(number).ok_or_else(|| FamilyFault::new(Rule::NotTaken, format!("SRV-33: no switch byte {number}")))?;
    Ok(alloc::vec![sysex(&engine::parameter_message(channel, number, value))])
}

/// The bytes after `F0` of a whole system exclusive message, as a block
/// carries it (MIDI-27).
fn sysex(message: &[u8]) -> Wire {
    Wire::Sysex(message.get(1..).unwrap_or(&[]).to_vec())
}

/// A patch's messages as the engine packed them, `F0` first: the function
/// type `30` or `31` message, then on synth60 its three control changes
/// (S106-31), every one at the event's tick (MIDI-37).
fn packed_wires(bytes: &[u8]) -> Vec<Wire> {
    let (message, controls) = bytes.split_at(bytes.len().min(PATCH_MESSAGE_BYTES));
    let mut wires = alloc::vec![sysex(message)];
    for &[status, data1, data2] in controls.as_chunks::<CONTROL_BYTES>().0 {
        wires.push(Wire::Channel(ChannelMessage { status, data1, data2 }));
    }
    wires
}

/// A patch of `model` given as the interface's bytes, checked as the
/// receiver checks it (SRV-32, SRV-57; S106-37), as its messages.
fn bytes_patch(model: Model, bytes: &[u8]) -> Result<Vec<Wire>, FamilyFault> {
    checked_bytes(model, bytes)?;
    Ok(packed_wires(bytes))
}

/// The engine's check of a patch given as bytes (SRV-32, S106-37), and the
/// patch it holds.
fn checked_bytes(model: Model, bytes: &[u8]) -> Result<Patch, FamilyFault> {
    engine::check_patch(model, bytes).map_err(|refusal| {
        FamilyFault::new(Rule::InvalidPatch, format!("SRV-32, S106-37: byte {} of the {} patch breaks {:?}", refusal.byte, name(model), refusal.rule))
    })
}

/// A patch of `model` built from named fields, checked (S106-36) and
/// packed by the engine on `channel` (SRV-30, SRV-31).
fn fields_patch(model: Model, patch: &Patch, channel: u8) -> Result<Vec<Wire>, FamilyFault> {
    checked(model, patch)?;
    let mut bytes = [0_u8; engine::MOST_PATCH_BYTES];
    let length = engine::pack_patch(model, patch, INLINE_PROGRAM, channel, &mut bytes);
    Ok(packed_wires(bytes.get(..length).unwrap_or(&[])))
}

/// The engine's check of a patch built from fields (S106-36).
fn checked(model: Model, patch: &Patch) -> Result<(), FamilyFault> {
    engine::check_fields(model, patch)
        .map_err(|field| FamilyFault::new(Rule::InvalidPatch, format!("SRV-30, SRV-31, S106-36: {} takes no such {field:?}", name(model))))
}

/// A bank of `model` built by the engine from patches, patch k at program
/// k (SRV-32, S106-38), and checked as the receiver reads it (SRV-60).
fn build(synth: Synth, patches: &[Patch]) -> Result<Vec<u8>, FamilyFault> {
    let model = synth.model();
    for (index, patch) in patches.iter().enumerate() {
        checked(model, patch).map_err(|fault| FamilyFault::new(fault.rule, format!("patch {index}: {}", fault.detail)))?;
    }
    let mut image = alloc::vec![0_u8; patches.len().min(MOST_BANK_PATCHES).saturating_mul(engine::patch_bytes(model))];
    let length = engine::build_bank(model, patches, BANK_CHANNEL, &mut image)
        .map_err(|refusal| FamilyFault::new(Rule::InvalidBank, format!("SRV-32, S106-38: 1 to 128 patches ({} given): {:?}", patches.len(), refusal.rule)))?;
    image.truncate(length);
    check_bank(synth, &image)?;
    Ok(image)
}

/// A bank given as bytes, checked (SRV-32, SRV-60).
fn bank_bytes(synth: Synth, bytes: &str) -> Result<Vec<u8>, FamilyFault> {
    let image = bytes_of(bytes);
    check_bank(synth, &image)?;
    Ok(image)
}

/// The range switch (S106-22).
const fn range(range: Synth106Range) -> Range {
    match range {
        Synth106Range::Feet16 => Range::Sixteen,
        Synth106Range::Feet8 => Range::Eight,
        Synth106Range::Feet4 => Range::Four,
    }
}

/// synth106's chorus (S106-22).
const fn chorus106(chorus: Synth106Chorus) -> ChorusMode {
    match chorus {
        Synth106Chorus::Off => ChorusMode::Off,
        Synth106Chorus::One => ChorusMode::One,
        Synth106Chorus::Two => ChorusMode::Two,
    }
}

/// synth60's chorus, with both (S106-31).
const fn chorus60(chorus: Synth60Chorus) -> ChorusMode {
    match chorus {
        Synth60Chorus::Off => ChorusMode::Off,
        Synth60Chorus::One => ChorusMode::One,
        Synth60Chorus::Two => ChorusMode::Two,
        Synth60Chorus::Both => ChorusMode::Both,
    }
}

/// synth106's PWM source (S106-22).
const fn pwm106(pwm: Synth106Pwm) -> PwmSource {
    match pwm {
        Synth106Pwm::Lfo => PwmSource::Lfo,
        Synth106Pwm::Manual => PwmSource::Manual,
    }
}

/// synth60's PWM source, with the envelope (S106-31).
const fn pwm60(pwm: Synth60Pwm) -> PwmSource {
    match pwm {
        Synth60Pwm::Lfo => PwmSource::Lfo,
        Synth60Pwm::Manual => PwmSource::Manual,
        Synth60Pwm::Envelope => PwmSource::Envelope,
    }
}

/// Whether the envelope's polarity is inverted (S106-22, byte 17 bit 1).
const fn inverted(polarity: Synth106Polarity) -> bool {
    matches!(polarity, Synth106Polarity::Negative)
}

/// Whether the VCA follows the gate (S106-22, byte 17 bit 2).
const fn gate(vca: Synth106Vca) -> bool {
    matches!(vca, Synth106Vca::Gate)
}

/// Sets the sixteen sliders, Juno-106 parameters 0 to 15 (S106-13).
fn set_sliders(patch: &mut Patch, sliders: [u8; 16]) {
    for (number, value) in (0_u8..).zip(sliders) {
        // Every slider value is 0 to 127 by its field's type (SRV-30), which
        // `set_parameter` takes.
        let _taken = patch.set_parameter(number, value);
    }
}

/// The engine's patch of a synth106 panel (SRV-30).
fn synth106_patch(panel: &Synth106Panel) -> Patch {
    let mut patch = Patch::default_for(Model::Synth106);
    set_sliders(
        &mut patch,
        [
            panel.lfo_rate.get(),
            panel.lfo_delay.get(),
            panel.dco_lfo.get(),
            panel.dco_pwm.get(),
            panel.noise_level.get(),
            panel.vcf_frequency.get(),
            panel.vcf_resonance.get(),
            panel.vcf_envelope.get(),
            panel.vcf_lfo.get(),
            panel.vcf_keyboard.get(),
            panel.vca_level.get(),
            panel.attack.get(),
            panel.decay.get(),
            panel.sustain.get(),
            panel.release.get(),
            panel.sub_level.get(),
        ],
    );
    patch.range = range(panel.range);
    patch.pulse = panel.pulse;
    patch.sawtooth = panel.sawtooth;
    patch.chorus = chorus106(panel.chorus);
    patch.pwm = pwm106(panel.pwm);
    patch.envelope_inverted = inverted(panel.envelope_polarity);
    patch.vca_gate = gate(panel.vca);
    patch.hpf = panel.hpf.get();
    // synth106 has no sub-oscillator switch: the sub is always on (S106-22).
    patch.sub = true;
    patch
}

/// The engine's patch of a synth60 panel (SRV-31).
fn synth60_patch(panel: &Synth60Panel) -> Patch {
    let mut patch = Patch::default_for(Model::Synth60);
    set_sliders(
        &mut patch,
        [
            panel.lfo_rate.get(),
            panel.lfo_delay.get(),
            panel.dco_lfo.get(),
            panel.dco_pwm.get(),
            panel.noise_level.get(),
            panel.vcf_frequency.get(),
            panel.vcf_resonance.get(),
            panel.vcf_envelope.get(),
            panel.vcf_lfo.get(),
            panel.vcf_keyboard.get(),
            panel.vca_level.get(),
            panel.attack.get(),
            panel.decay.get(),
            panel.sustain.get(),
            panel.release.get(),
            panel.sub_level.get(),
        ],
    );
    patch.range = range(panel.range);
    patch.pulse = panel.pulse;
    patch.sawtooth = panel.sawtooth;
    patch.chorus = chorus60(panel.chorus);
    patch.pwm = pwm60(panel.pwm);
    patch.envelope_inverted = inverted(panel.envelope_polarity);
    patch.vca_gate = gate(panel.vca);
    patch.hpf = panel.hpf.get();
    patch.sub = panel.sub_switch;
    patch
}

/// A slider parameter's number (0 to 15) and value, or none for a switch
/// (S106-13).
const fn synth106_slider(parameter: Synth106Parameter) -> Option<(u8, u8)> {
    use Synth106Parameter as P;
    Some(match parameter {
        P::lfo_rate(value) => (0, value.get()),
        P::lfo_delay(value) => (1, value.get()),
        P::dco_lfo(value) => (2, value.get()),
        P::dco_pwm(value) => (3, value.get()),
        P::noise_level(value) => (4, value.get()),
        P::vcf_frequency(value) => (5, value.get()),
        P::vcf_resonance(value) => (6, value.get()),
        P::vcf_envelope(value) => (7, value.get()),
        P::vcf_lfo(value) => (8, value.get()),
        P::vcf_keyboard(value) => (9, value.get()),
        P::vca_level(value) => (10, value.get()),
        P::attack(value) => (11, value.get()),
        P::decay(value) => (12, value.get()),
        P::sustain(value) => (13, value.get()),
        P::release(value) => (14, value.get()),
        P::sub_level(value) => (15, value.get()),
        P::range(_) | P::pulse(_) | P::sawtooth(_) | P::chorus(_) | P::pwm(_) | P::envelope_polarity(_) | P::vca(_) | P::hpf(_) => return None,
    })
}

/// Sets a synth106 switch parameter's field on `patch`, giving the switch
/// byte it is in (SRV-33 item 1: 16 for range, pulse, sawtooth and chorus;
/// 17 for pwm, envelope polarity, VCA and the high-pass filter), or none
/// for a slider.
fn set_synth106_switch(patch: &mut Patch, parameter: Synth106Parameter) -> Option<u8> {
    use Synth106Parameter as P;
    match parameter {
        P::range(value) => patch.range = range(value),
        P::pulse(value) => patch.pulse = value,
        P::sawtooth(value) => patch.sawtooth = value,
        P::chorus(value) => patch.chorus = chorus106(value),
        P::pwm(value) => patch.pwm = pwm106(value),
        P::envelope_polarity(value) => patch.envelope_inverted = inverted(value),
        P::vca(value) => patch.vca_gate = gate(value),
        P::hpf(value) => patch.hpf = value.get(),
        _ => return None,
    }
    Some(match parameter {
        P::range(_) | P::pulse(_) | P::sawtooth(_) | P::chorus(_) => FIRST_SWITCHES,
        _ => SECOND_SWITCHES,
    })
}

/// A synth60 slider parameter as synth106's, which is the same message
/// (SRV-33 item 2), or none for a switch.
const fn synth60_slider(parameter: Synth60Parameter) -> Option<(u8, u8)> {
    use Synth60Parameter as P;
    use Synth106Parameter as Q;
    synth106_slider(match parameter {
        P::lfo_rate(value) => Q::lfo_rate(value),
        P::lfo_delay(value) => Q::lfo_delay(value),
        P::dco_lfo(value) => Q::dco_lfo(value),
        P::dco_pwm(value) => Q::dco_pwm(value),
        P::noise_level(value) => Q::noise_level(value),
        P::vcf_frequency(value) => Q::vcf_frequency(value),
        P::vcf_resonance(value) => Q::vcf_resonance(value),
        P::vcf_envelope(value) => Q::vcf_envelope(value),
        P::vcf_lfo(value) => Q::vcf_lfo(value),
        P::vcf_keyboard(value) => Q::vcf_keyboard(value),
        P::vca_level(value) => Q::vca_level(value),
        P::attack(value) => Q::attack(value),
        P::decay(value) => Q::decay(value),
        P::sustain(value) => Q::sustain(value),
        P::release(value) => Q::release(value),
        P::sub_level(value) => Q::sub_level(value),
        P::range(_) | P::pulse(_) | P::sawtooth(_) | P::chorus(_) | P::pwm(_) | P::envelope_polarity(_) | P::vca(_) | P::hpf(_) | P::sub_switch(_) => {
            return None;
        }
    })
}

/// A synth60 switch parameter as the JU-06A controller S106-22 names for
/// its field, at the canonical value of its position (SRV-33 item 2;
/// S106-24): the switch, and a patch holding the field so that the
/// engine's `switch_value` reads it.
fn synth60_switch(parameter: Synth60Parameter) -> Option<(Switch, Patch)> {
    use Synth60Parameter as P;
    let mut patch = Patch::default_for(Model::Synth60);
    let switch = match parameter {
        P::range(value) => {
            patch.range = range(value);
            Switch::Range
        }
        P::pulse(value) => {
            patch.pulse = value;
            Switch::Pulse
        }
        P::sawtooth(value) => {
            patch.sawtooth = value;
            Switch::Sawtooth
        }
        P::chorus(value) => {
            patch.chorus = chorus60(value);
            Switch::Chorus
        }
        P::pwm(value) => {
            patch.pwm = pwm60(value);
            Switch::PwmSource
        }
        P::envelope_polarity(value) => {
            patch.envelope_inverted = inverted(value);
            Switch::Polarity
        }
        P::vca(value) => {
            patch.vca_gate = gate(value);
            Switch::Vca
        }
        P::hpf(value) => {
            patch.hpf = value.get();
            Switch::Hpf
        }
        P::sub_switch(value) => {
            patch.sub = value;
            Switch::Sub
        }
        _ => return None,
    };
    Some((switch, patch))
}

impl FamilyCalls for Synth106Family {
    type Bank = Synth106Bank;

    fn inline_patch(synth: Synth, event: &Synth106PatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault> {
        let wires = match &event.patch {
            Synth106Patch::Fields(panel) => fields_patch(synth.model(), &synth106_patch(panel), channel_bits(event.channel))?,
            Synth106Patch::Bytes(bytes) => bytes_patch(synth.model(), &bytes_of(&bytes.bytes))?,
        };
        Ok((wires, None))
    }

    fn parameter(_synth: Synth, event: &Synth106ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        let channel = channel_bits(event.channel);
        Ok(alloc::vec![match synth106_slider(event.set) {
            Some((number, value)) => sysex(&engine::parameter_message(channel, number, value)),
            None => Wire::Deferred(Deferred::Synth106Switch { channel, parameter: event.set }),
        }])
    }

    fn bank_image(synth: Synth, bank: &Synth106Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth106Bank::Patches(list) => {
                let patches: Vec<Patch> = list
                    .patches
                    .iter()
                    .map(|patch| match patch {
                        Synth106Patch::Fields(panel) => Ok(synth106_patch(panel)),
                        Synth106Patch::Bytes(bytes) => checked_bytes(Model::Synth106, &bytes_of(&bytes.bytes)),
                    })
                    .collect::<Result<_, _>>()?;
                build(synth, &patches)
            }
            Synth106Bank::Bytes(bytes) => bank_bytes(synth, &bytes.bytes),
        }
    }
}

impl FamilyCalls for Synth60Family {
    type Bank = Synth60Bank;

    fn inline_patch(synth: Synth, event: &Synth60PatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault> {
        let wires = match &event.patch {
            Synth60Patch::Fields(panel) => fields_patch(synth.model(), &synth60_patch(panel), channel_bits(event.channel))?,
            Synth60Patch::Bytes(bytes) => bytes_patch(synth.model(), &bytes_of(&bytes.bytes))?,
        };
        Ok((wires, None))
    }

    fn parameter(_synth: Synth, event: &Synth60ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        let channel = channel_bits(event.channel);
        if let Some((number, value)) = synth60_slider(event.set) {
            return Ok(alloc::vec![sysex(&engine::parameter_message(channel, number, value))]);
        }
        let (switch, patch) = synth60_switch(event.set).ok_or_else(|| FamilyFault::new(Rule::NotTaken, format!("SRV-33: {}", event.set.name())))?;
        Ok(alloc::vec![Wire::Channel(ChannelMessage::new(status::CONTROL_CHANGE, channel, switch.controller(), engine::switch_value(&patch, switch)))])
    }

    fn bank_image(synth: Synth, bank: &Synth60Bank) -> Result<Vec<u8>, FamilyFault> {
        match bank {
            Synth60Bank::Patches(list) => {
                let patches: Vec<Patch> = list
                    .patches
                    .iter()
                    .map(|patch| match patch {
                        Synth60Patch::Fields(panel) => Ok(synth60_patch(panel)),
                        Synth60Patch::Bytes(bytes) => checked_bytes(Model::Synth60, &bytes_of(&bytes.bytes)),
                    })
                    .collect::<Result<_, _>>()?;
                build(synth, &patches)
            }
            Synth60Bank::Bytes(bytes) => bank_bytes(synth, &bytes.bytes),
        }
    }
}
