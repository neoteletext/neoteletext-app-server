//! synth80's call site into the engine's interface module (server.md
//! SRV-24, SRV-43 to SRV-45, SRV-55, SRV-60, SRV-88): which messages it
//! takes, its patch as named fields, preset tones or its 85 bytes, written
//! inline as the 85 control changes of S80-40, its named parameters as
//! their controller, and its bank image of whole patches (S80-41 (a)).
//! The server writes nothing of synth80's own: every table is the
//! engine's (MIDI-21).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio_instruments::synth80::TONE_BYTES;
use neoteletext_audio_instruments::synth80::interface::{self as engine, MOST_BANK_IMAGE_BYTES, PATCH_BYTES, Patch, Tone, controller_of};

use super::FamilyFault;
use crate::protocol::check::bytes_of;
use crate::protocol::{
    Rule, Synth80Bank, Synth80Family, Synth80Panel, Synth80Parameter, Synth80ParameterEvent, Synth80Patch, Synth80PatchEvent, Synth80Preset, Synth80Tone,
    Synth80ToneFields,
};
use crate::sound::family::Synth;
use crate::sound::synth::FamilyCalls;
use crate::sound::tracker::TrackHint;
use crate::sound::wire::{Wire, channel_bits};

#[cfg(test)]
mod tests;

/// The most banks a synth80 sound names (S80-36: controller 0 values 0 to 2 choose among three banks; SRV-58).
/// The engine's constant.
pub(crate) const MOST_BANKS: usize = engine::BANKS;

/// The bytes of a patch outside its two tones: the 33 master controls
/// (S80-33).
const MASTER_BYTES: usize = PATCH_BYTES - 2 * TONE_BYTES;

/// Whether synth80 takes `message` (S80-1, SRV-24), by the engine's table.
pub(crate) fn takes(message: &Wire) -> Result<(), String> {
    if engine::accepts(&message.message()) {
        Ok(())
    } else {
        Err(String::from("S80-1, S80-9: synth80 does not act on this message; it takes no system exclusive message"))
    }
}

/// Checks a bank image (SRV-60; S80-41 (a)) with the engine's reader.
pub(crate) fn check_bank(image: &[u8]) -> Result<(), FamilyFault> {
    engine::check_bank(image).map(|_| ()).map_err(|error| {
        FamilyFault::new(Rule::InvalidBank, format!("S80-41, MIDI-53: the bank is not 1 to 128 whole patches of 85 bytes each 0 to 127: {error:?}"))
    })
}

/// The engine's tone of a preset's name (S80-42; SRV-43): the name on the
/// instrument's button, as `PRESETS` holds it.
const fn preset_label(preset: Synth80Preset) -> &'static str {
    match preset {
        Synth80Preset::String1 => "STRING 1",
        Synth80Preset::String3 => "STRING 3",
        Synth80Preset::Brass1 => "BRASS 1",
        Synth80Preset::Flute => "FLUTE",
        Synth80Preset::ElectricPiano => "ELECTRIC PIANO",
        Synth80Preset::Clavichord1 => "CLAVICHORD 1",
        Synth80Preset::Harpsichord1 => "HARPSICHORD 1",
        Synth80Preset::Organ1 => "ORGAN 1",
        Synth80Preset::Guitar1 => "GUITAR 1",
        Synth80Preset::Funky1 => "FUNKY 1",
        Synth80Preset::Funky3 => "FUNKY 3",
        Synth80Preset::String2 => "STRING 2",
        Synth80Preset::String4 => "STRING 4",
        Synth80Preset::Brass2 => "BRASS 2",
        Synth80Preset::Brass3 => "BRASS 3",
        Synth80Preset::Bass => "BASS",
        Synth80Preset::Clavichord2 => "CLAVICHORD 2",
        Synth80Preset::Harpsichord2 => "HARPSICHORD 2",
        Synth80Preset::Organ2 => "ORGAN 2",
        Synth80Preset::Guitar2 => "GUITAR 2",
        Synth80Preset::Funky2 => "FUNKY 2",
        Synth80Preset::Funky4 => "FUNKY 4",
    }
}

/// A tone's 26 bytes in the order of S80-33 (SRV-43).
fn tone_bytes(tone: &Synth80Tone) -> Result<[u8; TONE_BYTES], FamilyFault> {
    match tone {
        Synth80Tone::Fields(fields) => Ok(fields_bytes(fields)),
        Synth80Tone::Preset(preset) => {
            let label = preset_label(preset.name);
            engine::preset(label)
                .map(|tone: Tone| tone.to_bytes())
                .ok_or_else(|| FamilyFault::new(Rule::InvalidPatch, format!("S80-42: the engine holds no preset tone {label}")))
        }
    }
}

fn fields_bytes(tone: &Synth80ToneFields) -> [u8; TONE_BYTES] {
    [
        tone.pwm_speed.get(),
        tone.pwm.get(),
        tone.pw.get(),
        tone.square.get(),
        tone.sawtooth.get(),
        tone.noise.get(),
        tone.hpf.get(),
        tone.hpf_resonance.get(),
        tone.lpf.get(),
        tone.lpf_resonance.get(),
        tone.filter_initial_level.get(),
        tone.filter_attack_level.get(),
        tone.filter_attack.get(),
        tone.filter_decay.get(),
        tone.filter_release.get(),
        tone.vcf_level.get(),
        tone.sine.get(),
        tone.amplifier_attack.get(),
        tone.amplifier_decay.get(),
        tone.amplifier_sustain.get(),
        tone.amplifier_release.get(),
        tone.level.get(),
        tone.initial_brilliance.get(),
        tone.initial_level.get(),
        tone.after_brilliance.get(),
        tone.after_level.get(),
    ]
}

fn master_bytes(panel: &Synth80Panel) -> [u8; MASTER_BYTES] {
    [
        panel.feet_1.get(),
        panel.feet_2.get(),
        panel.detune_channel_2.get(),
        panel.mix.get(),
        panel.brilliance.get(),
        panel.resonance.get(),
        panel.keyboard_brilliance_low.get(),
        panel.keyboard_brilliance_high.get(),
        panel.keyboard_level_low.get(),
        panel.keyboard_level_high.get(),
        panel.sub_oscillator_function.get(),
        panel.sub_oscillator_speed.get(),
        panel.sub_oscillator_vco.get(),
        panel.sub_oscillator_vcf.get(),
        panel.sub_oscillator_vca.get(),
        panel.touch_pitch_bend.get(),
        panel.touch_speed.get(),
        panel.touch_vco.get(),
        panel.touch_vcf.get(),
        panel.ring_modulator_attack_time.get(),
        panel.ring_modulator_decay_time.get(),
        panel.ring_modulator_depth.get(),
        panel.ring_modulator_speed.get(),
        panel.ring_modulator_modulation.get(),
        panel.sustain_mode.get(),
        panel.sustain.get(),
        panel.glide_mode.get(),
        panel.glide_speed.get(),
        panel.tremolo_chorus.get(),
        panel.chorus.get(),
        panel.tremolo.get(),
        panel.tremolo_chorus_speed.get(),
        panel.tremolo_chorus_depth.get(),
    ]
}

/// A patch of named fields (SRV-43): channel I's tone, channel II's, then
/// the master controls, read as the engine reads 85 bytes.
pub(crate) fn panel_patch(panel: &Synth80Panel) -> Result<Patch, FamilyFault> {
    let mut bytes = [0_u8; PATCH_BYTES];
    let values = tone_bytes(&panel.channel_1)?.into_iter().chain(tone_bytes(&panel.channel_2)?).chain(master_bytes(panel));
    for (byte, value) in bytes.iter_mut().zip(values) {
        *byte = value;
    }
    checked(&bytes)
}

/// The patch 85 bytes hold, by the engine's check (S80-33; SRV-44).
fn checked(bytes: &[u8]) -> Result<Patch, FamilyFault> {
    engine::check_patch(bytes).map_err(|error| FamilyFault::new(Rule::InvalidPatch, format!("S80-33, SRV-44: a patch is 85 bytes each 0 to 127: {error:?}")))
}

fn patch_of(patch: &Synth80Patch) -> Result<Patch, FamilyFault> {
    match patch {
        Synth80Patch::Fields(panel) => panel_patch(panel),
        Synth80Patch::Bytes(bytes) => checked(&bytes_of(&bytes.bytes)),
    }
}

/// The byte of the patch a named parameter moves and its value (SRV-45):
/// the master controls first, bytes 52 to 84, then channel I's tone,
/// bytes 0 to 25, then channel II's, bytes 26 to 51, the order of the
/// family document's table.
#[allow(clippy::too_many_lines, reason = "one arm for each of the 85 parameters of SRV-45")]
pub(crate) const fn place_of_parameter(parameter: Synth80Parameter) -> (usize, u8) {
    use Synth80Parameter as P;
    match parameter {
        P::feet_1(value) => (52, value.get()),
        P::feet_2(value) => (53, value.get()),
        P::detune_channel_2(value) => (54, value.get()),
        P::mix(value) => (55, value.get()),
        P::brilliance(value) => (56, value.get()),
        P::resonance(value) => (57, value.get()),
        P::keyboard_brilliance_low(value) => (58, value.get()),
        P::keyboard_brilliance_high(value) => (59, value.get()),
        P::keyboard_level_low(value) => (60, value.get()),
        P::keyboard_level_high(value) => (61, value.get()),
        P::sub_oscillator_function(value) => (62, value.get()),
        P::sub_oscillator_speed(value) => (63, value.get()),
        P::sub_oscillator_vco(value) => (64, value.get()),
        P::sub_oscillator_vcf(value) => (65, value.get()),
        P::sub_oscillator_vca(value) => (66, value.get()),
        P::touch_pitch_bend(value) => (67, value.get()),
        P::touch_speed(value) => (68, value.get()),
        P::touch_vco(value) => (69, value.get()),
        P::touch_vcf(value) => (70, value.get()),
        P::ring_modulator_attack_time(value) => (71, value.get()),
        P::ring_modulator_decay_time(value) => (72, value.get()),
        P::ring_modulator_depth(value) => (73, value.get()),
        P::ring_modulator_speed(value) => (74, value.get()),
        P::ring_modulator_modulation(value) => (75, value.get()),
        P::sustain_mode(value) => (76, value.get()),
        P::sustain(value) => (77, value.get()),
        P::glide_mode(value) => (78, value.get()),
        P::glide_speed(value) => (79, value.get()),
        P::tremolo_chorus(value) => (80, value.get()),
        P::chorus(value) => (81, value.get()),
        P::tremolo(value) => (82, value.get()),
        P::tremolo_chorus_speed(value) => (83, value.get()),
        P::tremolo_chorus_depth(value) => (84, value.get()),
        P::channel_1_pwm_speed(value) => (0, value.get()),
        P::channel_1_pwm(value) => (1, value.get()),
        P::channel_1_pw(value) => (2, value.get()),
        P::channel_1_square(value) => (3, value.get()),
        P::channel_1_sawtooth(value) => (4, value.get()),
        P::channel_1_noise(value) => (5, value.get()),
        P::channel_1_hpf(value) => (6, value.get()),
        P::channel_1_hpf_resonance(value) => (7, value.get()),
        P::channel_1_lpf(value) => (8, value.get()),
        P::channel_1_lpf_resonance(value) => (9, value.get()),
        P::channel_1_filter_initial_level(value) => (10, value.get()),
        P::channel_1_filter_attack_level(value) => (11, value.get()),
        P::channel_1_filter_attack(value) => (12, value.get()),
        P::channel_1_filter_decay(value) => (13, value.get()),
        P::channel_1_filter_release(value) => (14, value.get()),
        P::channel_1_vcf_level(value) => (15, value.get()),
        P::channel_1_sine(value) => (16, value.get()),
        P::channel_1_amplifier_attack(value) => (17, value.get()),
        P::channel_1_amplifier_decay(value) => (18, value.get()),
        P::channel_1_amplifier_sustain(value) => (19, value.get()),
        P::channel_1_amplifier_release(value) => (20, value.get()),
        P::channel_1_level(value) => (21, value.get()),
        P::channel_1_initial_brilliance(value) => (22, value.get()),
        P::channel_1_initial_level(value) => (23, value.get()),
        P::channel_1_after_brilliance(value) => (24, value.get()),
        P::channel_1_after_level(value) => (25, value.get()),
        P::channel_2_pwm_speed(value) => (26, value.get()),
        P::channel_2_pwm(value) => (27, value.get()),
        P::channel_2_pw(value) => (28, value.get()),
        P::channel_2_square(value) => (29, value.get()),
        P::channel_2_sawtooth(value) => (30, value.get()),
        P::channel_2_noise(value) => (31, value.get()),
        P::channel_2_hpf(value) => (32, value.get()),
        P::channel_2_hpf_resonance(value) => (33, value.get()),
        P::channel_2_lpf(value) => (34, value.get()),
        P::channel_2_lpf_resonance(value) => (35, value.get()),
        P::channel_2_filter_initial_level(value) => (36, value.get()),
        P::channel_2_filter_attack_level(value) => (37, value.get()),
        P::channel_2_filter_attack(value) => (38, value.get()),
        P::channel_2_filter_decay(value) => (39, value.get()),
        P::channel_2_filter_release(value) => (40, value.get()),
        P::channel_2_vcf_level(value) => (41, value.get()),
        P::channel_2_sine(value) => (42, value.get()),
        P::channel_2_amplifier_attack(value) => (43, value.get()),
        P::channel_2_amplifier_decay(value) => (44, value.get()),
        P::channel_2_amplifier_sustain(value) => (45, value.get()),
        P::channel_2_amplifier_release(value) => (46, value.get()),
        P::channel_2_level(value) => (47, value.get()),
        P::channel_2_initial_brilliance(value) => (48, value.get()),
        P::channel_2_initial_level(value) => (49, value.get()),
        P::channel_2_after_brilliance(value) => (50, value.get()),
        P::channel_2_after_level(value) => (51, value.get()),
    }
}

impl FamilyCalls for Synth80Family {
    type Bank = Synth80Bank;

    /// S80-40: an inline patch is its 85 control changes at one tick, and
    /// the tracker is told the patch so that it restates it whole (S80-46).
    fn inline_patch(_synth: Synth, event: &Synth80PatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault> {
        let patch = patch_of(&event.patch)?;
        let channel = channel_bits(event.channel);
        let wires = engine::patch_messages(&patch, channel)
            .map(|message| match message {
                neoteletext_audio::midi::Message::Channel(message) => Wire::Channel(message),
                neoteletext_audio::midi::Message::SystemExclusive(bytes) => Wire::Sysex(bytes.to_vec()),
            })
            .collect();
        Ok((wires, Some(TrackHint::Synth80Patch(Box::new(patch)))))
    }

    /// SRV-45: the parameter's own controller, on the event's channel.
    fn parameter(_synth: Synth, event: &Synth80ParameterEvent) -> Result<Vec<Wire>, FamilyFault> {
        let (place, value) = place_of_parameter(event.set);
        let number = controller_of(place).ok_or_else(|| FamilyFault::new(Rule::NotTaken, format!("S80-33: byte {place} of a patch has no controller")))?;
        Ok(alloc::vec![Wire::Channel(neoteletext_audio::midi::ChannelMessage::new(
            neoteletext_audio::midi::status::CONTROL_CHANGE,
            channel_bits(event.channel),
            number,
            value,
        ))])
    }

    fn bank_image(_synth: Synth, bank: &Synth80Bank) -> Result<Vec<u8>, FamilyFault> {
        let image = match bank {
            Synth80Bank::Patches(list) => {
                let patches = list.patches.iter().map(patch_of).collect::<Result<Vec<_>, _>>()?;
                let mut out = Box::new([0_u8; MOST_BANK_IMAGE_BYTES]);
                let length = engine::build_bank(&patches, &mut out)
                    .map_err(|error| FamilyFault::new(Rule::InvalidBank, format!("S80-41: 1 to 128 patches: {error:?}")))?;
                out.get(..length).unwrap_or(&[]).to_vec()
            }
            Synth80Bank::Bytes(bytes) => bytes_of(&bytes.bytes),
        };
        check_bank(&image)?;
        Ok(image)
    }
}
