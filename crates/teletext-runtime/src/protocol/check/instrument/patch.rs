//! Every byte of an analogue synthesizer's patch, held to the range of the
//! control it holds (DEC-6429). A receiver reads a bank's patch with the
//! instrument's own reader and plays its default patch in place of one that
//! reader refuses, saying nothing; so the server refuses such a patch where
//! the program made it. Each table follows the engine's
//! `neoteletext-audio-instruments` `<instrument>/patch.rs`, the layout the
//! receiver reads, and names each control as the instrument's panel does.

use alloc::format;
use alloc::string::String;

use super::super::super::{SYNTH8_PATCH_BYTES, SYNTH53_PATCH_BYTES, SYNTH80_PATCH_BYTES, SYNTH106_PATCH_BYTES, Synth};

/// What a field of a patch may hold.
#[derive(Clone, Copy)]
enum Range {
    /// One byte, 0 to this.
    Byte(u8),
    /// Two bytes, the low first, a signed number from minus this to this.
    Signed(i16),
    /// Two bytes, the low first, a number from 0 to this.
    Word(u16),
    /// One byte of switches: no bit outside this mask.
    Bits(u8),
    /// The first switch byte of a synth106 patch: seven bits, and its
    /// three range bits exactly one of 16', 8' and 4'.
    Synth106Feet,
}

impl Range {
    /// The bytes the field takes.
    const fn bytes(self) -> usize {
        match self {
            Self::Signed(_) | Self::Word(_) => 2,
            Self::Byte(_) | Self::Bits(_) | Self::Synth106Feet => 1,
        }
    }
}

/// One field: its first byte, its control's name and its range.
type Field = (u8, &'static str, Range);

/// A slider's or knob's largest value, the MIDI data byte's seven bits:
/// synth106's sliders, synth53's knobs and synth80's levers. Source:
/// Juno-106 owner's manual, MIDI Implementation 3.1, "value 0 to 127";
/// Prophet-5 Operation Manual, section 6-7, "0-127 steps".
const SEVEN_BITS: u8 = 0x7F;

/// The range bits of a synth106 patch's first switch byte, one for each of
/// 16', 8' and 4'. Source: SYNTH106-106 owner's manual, MIDI Implementation
/// 3.4, "for switches".
const SYNTH106_RANGE_BITS: [u8; 3] = [0b001, 0b010, 0b100];
/// The mask of those bits.
const SYNTH106_RANGE_MASK: u8 = 0b111;
/// The second switch byte of a synth106 patch: PWM, ENV polarity, VCA mode and two bits
/// of the high-pass filter (the same table).
const SYNTH106_SWITCHES: u8 = 0b1_1111;

/// The patch of synth106 and synth60: sixteen sliders, then two bytes of
/// switches. Source: Juno-106 owner's manual, MIDI Implementation 3.1 and
/// 3.4.
const SYNTH106: [Field; 18] = [
    (0, "LFO RATE", Range::Byte(SEVEN_BITS)),
    (1, "LFO DELAY TIME", Range::Byte(SEVEN_BITS)),
    (2, "DCO LFO", Range::Byte(SEVEN_BITS)),
    (3, "DCO PWM", Range::Byte(SEVEN_BITS)),
    (4, "DCO NOISE", Range::Byte(SEVEN_BITS)),
    (5, "VCF FREQ", Range::Byte(SEVEN_BITS)),
    (6, "VCF RES", Range::Byte(SEVEN_BITS)),
    (7, "VCF ENV", Range::Byte(SEVEN_BITS)),
    (8, "VCF LFO", Range::Byte(SEVEN_BITS)),
    (9, "VCF KYBD", Range::Byte(SEVEN_BITS)),
    (10, "VCA LEVEL", Range::Byte(SEVEN_BITS)),
    (11, "ENV A", Range::Byte(SEVEN_BITS)),
    (12, "ENV D", Range::Byte(SEVEN_BITS)),
    (13, "ENV S", Range::Byte(SEVEN_BITS)),
    (14, "ENV R", Range::Byte(SEVEN_BITS)),
    (15, "DCO SUB", Range::Byte(SEVEN_BITS)),
    (16, "range, waveforms and chorus", Range::Synth106Feet),
    (17, "PWM, ENV, VCA and HPF switches", Range::Bits(SYNTH106_SWITCHES)),
];

/// A synthD knob's largest value: tenths of its dial's 0 to 10.
const SYNTHD_KNOB: u8 = 100;
/// A range or waveform switch's last position: six of each. Source:
/// Minimoog owner's manual, page 4.
const SYNTHD_POSITION: u8 = 5;
/// TUNE's travel either way, in cents (the engine's `synthd::law`).
const SYNTHD_TUNE_CENTS: i16 = 242;
/// An oscillator's FREQUENCY travel either way, in cents: seven semitones.
const SYNTHD_FREQUENCY_CENTS: i16 = 700;
/// CUTOFF FREQUENCY's ends: hundredths of the dial's -5 to 5.
const SYNTHD_CUTOFF: i16 = 500;
/// A contour's longest time, in milliseconds: the dial's 10 seconds.
const SYNTHD_TIME_MS: u16 = 10_000;
/// The first switch byte: oscillator modulation, oscillator 3 control and
/// the mixer's five switches.
const SYNTHD_FIRST_SWITCHES: u8 = (1 << 7) - 1;
/// The second: pink noise, filter modulation, the two keyboard controls,
/// glide and decay.
const SYNTHD_SECOND_SWITCHES: u8 = (1 << 6) - 1;

/// synthD's panel in the engine's own layout (`synthd/patch.rs`). Source:
/// Minimoog owner's manual, sections E to J.
const SYNTHD: [Field; 28] = [
    (0, "TUNE", Range::Signed(SYNTHD_TUNE_CENTS)),
    (2, "GLIDE", Range::Byte(SYNTHD_KNOB)),
    (3, "MODULATION MIX", Range::Byte(SYNTHD_KNOB)),
    (4, "OSCILLATOR 1 RANGE", Range::Byte(SYNTHD_POSITION)),
    (5, "OSCILLATOR 1 WAVEFORM", Range::Byte(SYNTHD_POSITION)),
    (6, "OSCILLATOR 2 RANGE", Range::Byte(SYNTHD_POSITION)),
    (7, "OSCILLATOR 2 WAVEFORM", Range::Byte(SYNTHD_POSITION)),
    (8, "OSCILLATOR 2 FREQUENCY", Range::Signed(SYNTHD_FREQUENCY_CENTS)),
    (10, "OSCILLATOR 3 RANGE", Range::Byte(SYNTHD_POSITION)),
    (11, "OSCILLATOR 3 WAVEFORM", Range::Byte(SYNTHD_POSITION)),
    (12, "OSCILLATOR 3 FREQUENCY", Range::Signed(SYNTHD_FREQUENCY_CENTS)),
    (14, "OSCILLATOR 1 VOLUME", Range::Byte(SYNTHD_KNOB)),
    (15, "OSCILLATOR 2 VOLUME", Range::Byte(SYNTHD_KNOB)),
    (16, "OSCILLATOR 3 VOLUME", Range::Byte(SYNTHD_KNOB)),
    (17, "EXTERNAL INPUT VOLUME", Range::Byte(SYNTHD_KNOB)),
    (18, "NOISE VOLUME", Range::Byte(SYNTHD_KNOB)),
    (19, "CUTOFF FREQUENCY", Range::Signed(SYNTHD_CUTOFF)),
    (21, "EMPHASIS", Range::Byte(SYNTHD_KNOB)),
    (22, "AMOUNT OF CONTOUR", Range::Byte(SYNTHD_KNOB)),
    (23, "filter ATTACK TIME", Range::Word(SYNTHD_TIME_MS)),
    (25, "filter DECAY TIME", Range::Word(SYNTHD_TIME_MS)),
    (27, "filter SUSTAIN LEVEL", Range::Byte(SYNTHD_KNOB)),
    (28, "loudness ATTACK TIME", Range::Word(SYNTHD_TIME_MS)),
    (30, "loudness DECAY TIME", Range::Word(SYNTHD_TIME_MS)),
    (32, "loudness SUSTAIN LEVEL", Range::Byte(SYNTHD_KNOB)),
    (33, "main output VOLUME", Range::Byte(SYNTHD_KNOB)),
    (34, "modulation and mixer switches", Range::Bits(SYNTHD_FIRST_SWITCHES)),
    (35, "noise, filter, keyboard, glide and decay switches", Range::Bits(SYNTHD_SECOND_SWITCHES)),
];

/// Every value of a byte: synth53 keeps a knob in its low seven bits
/// and a switch in its top bit.
const ANY: u8 = u8::MAX;

/// The program of synth53, synth52 and synth51 in its memory: a knob and
/// a switch in each byte but the last two, whose top bit holds nothing and
/// is refused here though the instrument would drop it, so that a bank
/// means what it says. Source: Prophet-5 Operation Manual, CM1000D,
/// section 6-7, the table of the 24 bytes.
const SYNTH53: [Field; 24] = [
    (0, "FILTER ATTACK, OSC A PULSE", Range::Byte(ANY)),
    (1, "FILTER DECAY, OSC A SAW", Range::Byte(ANY)),
    (2, "FILTER SUSTAIN, SYNC", Range::Byte(ANY)),
    (3, "FILTER RELEASE, OSC B SAW", Range::Byte(ANY)),
    (4, "AMPLIFIER ATTACK, OSC B TRIANGLE", Range::Byte(ANY)),
    (5, "AMPLIFIER DECAY, OSC B PULSE", Range::Byte(ANY)),
    (6, "AMPLIFIER SUSTAIN, OSC B KEYBOARD", Range::Byte(ANY)),
    (7, "AMPLIFIER RELEASE, UNISON", Range::Byte(ANY)),
    (8, "CUTOFF, POLY-MOD FREQ A", Range::Byte(ANY)),
    (9, "FILTER ENVELOPE AMOUNT, POLY-MOD PW A", Range::Byte(ANY)),
    (10, "MIXER OSC B, POLY-MOD FILTER", Range::Byte(ANY)),
    (11, "OSC B PULSE WIDTH, LFO SAW", Range::Byte(ANY)),
    (12, "MIXER OSC A, LFO TRIANGLE", Range::Byte(ANY)),
    (13, "OSC A PULSE WIDTH, LFO SQUARE", Range::Byte(ANY)),
    (14, "MIXER NOISE, FILTER KEYBOARD", Range::Byte(ANY)),
    (15, "RESONANCE, RELEASE", Range::Byte(ANY)),
    (16, "GLIDE, WHEEL-MOD FREQ A", Range::Byte(ANY)),
    (17, "LFO FREQUENCY, WHEEL-MOD FREQ B", Range::Byte(ANY)),
    (18, "WHEEL-MOD SOURCE MIX, WHEEL-MOD PW A", Range::Byte(ANY)),
    (19, "POLY-MOD OSC B, WHEEL-MOD PW B", Range::Byte(ANY)),
    (20, "POLY-MOD FILTER ENVELOPE, WHEEL-MOD FILTER", Range::Byte(ANY)),
    (21, "OSC A FREQUENCY, OSC B LO FREQ", Range::Byte(ANY)),
    (22, "OSC B FREQUENCY", Range::Byte(SEVEN_BITS)),
    (23, "OSC B FINE", Range::Byte(SEVEN_BITS)),
];

/// A synth8 switch of two positions.
const SYNTH8_SWITCH: u8 = 1;
/// A selector of four positions: waveforms, VCO-1's ranges, assign modes.
/// Source: Jupiter-8 owner's manual, printed pages 12 and 15 to 17.
const SYNTH8_SELECTOR: u8 = 3;
/// The pulse width's sources: LFO, manual, envelope 1 (printed page 17).
const SYNTH8_PWM_SOURCE: u8 = 2;
/// VCO-2's range: three octaves of semitones above 16' (printed page 16).
const SYNTH8_VCO2_RANGE: u8 = 36;

/// synth8's panel (the engine's `synth8/patch.rs`). A slider is any byte;
/// a signed control is any byte too, 128 being none. Source: Roland's
/// Jupiter-8/System-8 Parameter Correspondence Table, whose order this is.
const SYNTH8: [Field; 43] = [
    (0, "LFO RATE", Range::Byte(ANY)),
    (1, "LFO DELAY TIME", Range::Byte(ANY)),
    (2, "LFO WAVEFORM", Range::Byte(SYNTH8_SELECTOR)),
    (3, "VCO MODULATOR LFO MOD", Range::Byte(ANY)),
    (4, "VCO MODULATOR ENV MOD", Range::Byte(ANY)),
    (5, "VCO MODULATOR to VCO-1", Range::Byte(SYNTH8_SWITCH)),
    (6, "VCO MODULATOR to VCO-2", Range::Byte(SYNTH8_SWITCH)),
    (7, "PULSE WIDTH MODULATION", Range::Byte(ANY)),
    (8, "PWM source", Range::Byte(SYNTH8_PWM_SOURCE)),
    (9, "VCO-1 CROSS MOD", Range::Byte(ANY)),
    (10, "VCO-1 RANGE", Range::Byte(SYNTH8_SELECTOR)),
    (11, "VCO-1 WAVEFORM", Range::Byte(SYNTH8_SELECTOR)),
    (12, "VCO-2 SYNC", Range::Byte(SYNTH8_SWITCH)),
    (13, "VCO-2 LOW FREQ", Range::Byte(SYNTH8_SWITCH)),
    (14, "VCO-2 RANGE", Range::Byte(SYNTH8_VCO2_RANGE)),
    (15, "VCO-2 FINE TUNE", Range::Byte(ANY)),
    (16, "VCO-2 WAVEFORM", Range::Byte(SYNTH8_SELECTOR)),
    (17, "SOURCE MIX, VCO-1", Range::Byte(ANY)),
    (18, "SOURCE MIX, VCO-2", Range::Byte(ANY)),
    (19, "HPF CUTOFF FREQ", Range::Byte(ANY)),
    (20, "VCF CUTOFF FREQ", Range::Byte(ANY)),
    (21, "VCF RESONANCE", Range::Byte(ANY)),
    (22, "VCF SLOPE", Range::Byte(SYNTH8_SWITCH)),
    (23, "VCF ENV MOD", Range::Byte(ANY)),
    (24, "VCF envelope", Range::Byte(SYNTH8_SWITCH)),
    (25, "VCF LFO MOD", Range::Byte(ANY)),
    (26, "VCF KEY FOLLOW", Range::Byte(ANY)),
    (27, "VCA LEVEL", Range::Byte(ANY)),
    (28, "VCA LFO MOD", Range::Byte(ANY)),
    (29, "ENV-1 ATTACK", Range::Byte(ANY)),
    (30, "ENV-1 DECAY", Range::Byte(ANY)),
    (31, "ENV-1 SUSTAIN", Range::Byte(ANY)),
    (32, "ENV-1 RELEASE", Range::Byte(ANY)),
    (33, "ENV-1 polarity", Range::Byte(SYNTH8_SWITCH)),
    (34, "ENV-2 ATTACK", Range::Byte(ANY)),
    (35, "ENV-2 DECAY", Range::Byte(ANY)),
    (36, "ENV-2 SUSTAIN", Range::Byte(ANY)),
    (37, "ENV-2 RELEASE", Range::Byte(ANY)),
    (38, "ASSIGN MODE", Range::Byte(SYNTH8_SELECTOR)),
    (39, "ENV-1 KEY FOLLOW", Range::Byte(SYNTH8_SWITCH)),
    (40, "ENV-2 KEY FOLLOW", Range::Byte(SYNTH8_SWITCH)),
    (41, "BENDER to VCO-1", Range::Byte(SYNTH8_SWITCH)),
    (42, "BENDER to VCO-2", Range::Byte(SYNTH8_SWITCH)),
];

/// A synth80 switch.
const SYNTH80_SWITCH: u8 = 1;
/// A selector of six positions: FEET and SUB OSCILLATOR FUNCTION.
const SYNTH80_SELECTOR: u8 = 5;

/// A synth80 tone, one row of the panel, as its memory panel orders it.
/// Source: CS-80 owner's manual, page 36.
const SYNTH80_TONE: [(&str, u8); 26] = [
    ("PWM SPEED", SEVEN_BITS),
    ("PWM", SEVEN_BITS),
    ("PW", SEVEN_BITS),
    ("SQUARE", SYNTH80_SWITCH),
    ("SAWTOOTH", SYNTH80_SWITCH),
    ("NOISE", SEVEN_BITS),
    ("HPF", SEVEN_BITS),
    ("RESh", SEVEN_BITS),
    ("LPF", SEVEN_BITS),
    ("RESl", SEVEN_BITS),
    ("IL", SEVEN_BITS),
    ("AL", SEVEN_BITS),
    ("filter A", SEVEN_BITS),
    ("filter D", SEVEN_BITS),
    ("filter R", SEVEN_BITS),
    ("VCF LEVEL", SEVEN_BITS),
    ("SINE", SEVEN_BITS),
    ("A", SEVEN_BITS),
    ("D", SEVEN_BITS),
    ("S", SEVEN_BITS),
    ("R", SEVEN_BITS),
    ("LEVEL", SEVEN_BITS),
    ("INITIAL BRILLIANCE", SEVEN_BITS),
    ("INITIAL LEVEL", SEVEN_BITS),
    ("AFTER BRILLIANCE", SEVEN_BITS),
    ("AFTER LEVEL", SEVEN_BITS),
];

/// synth80's controls outside its tones (the engine's `synth80/patch.rs`).
/// Source: CS-80 owner's manual, items 1 to 46.
const SYNTH80_GLOBAL: [(&str, u8); 33] = [
    ("FEET of channel I", SYNTH80_SELECTOR),
    ("FEET of channel II", SYNTH80_SELECTOR),
    ("DETUNE CH II", SEVEN_BITS),
    ("MIX", SEVEN_BITS),
    ("BRILLIANCE", SEVEN_BITS),
    ("RESONANCE", SEVEN_BITS),
    ("KEYBOARD CONTROL BRILLIANCE LOW", SEVEN_BITS),
    ("KEYBOARD CONTROL BRILLIANCE HIGH", SEVEN_BITS),
    ("KEYBOARD CONTROL LEVEL LOW", SEVEN_BITS),
    ("KEYBOARD CONTROL LEVEL HIGH", SEVEN_BITS),
    ("SUB OSCILLATOR FUNCTION", SYNTH80_SELECTOR),
    ("SUB OSCILLATOR SPEED", SEVEN_BITS),
    ("SUB OSCILLATOR VCO", SEVEN_BITS),
    ("SUB OSCILLATOR VCF", SEVEN_BITS),
    ("SUB OSCILLATOR VCA", SEVEN_BITS),
    ("TOUCH RESPONSE PITCHBEND", SEVEN_BITS),
    ("TOUCH RESPONSE SPEED", SEVEN_BITS),
    ("TOUCH RESPONSE VCO", SEVEN_BITS),
    ("TOUCH RESPONSE VCF", SEVEN_BITS),
    ("RING MODULATOR ATTACK TIME", SEVEN_BITS),
    ("RING MODULATOR DECAY TIME", SEVEN_BITS),
    ("RING MODULATOR DEPTH", SEVEN_BITS),
    ("RING MODULATOR SPEED", SEVEN_BITS),
    ("RING MODULATOR MODULATION", SEVEN_BITS),
    ("SUSTAIN I or II", SYNTH80_SWITCH),
    ("SUSTAIN", SEVEN_BITS),
    ("PORTAMENTO or GLISSANDO", SYNTH80_SWITCH),
    ("PORTAMENTO/GLISSANDO", SEVEN_BITS),
    ("TREMOLO/CHORUS ON", SYNTH80_SWITCH),
    ("CHORUS", SYNTH80_SWITCH),
    ("TREMOLO", SYNTH80_SWITCH),
    ("TREMOLO/CHORUS SPEED", SEVEN_BITS),
    ("TREMOLO/CHORUS DEPTH", SEVEN_BITS),
];

// Each table covers its patch: synth80's two tones and its other
// controls, and every other table's last field ending at the patch's end
// (the test `every_table_covers_its_patch_once` checks each byte once).
const _: () = assert!(SYNTH80_TONE.len() * 2 + SYNTH80_GLOBAL.len() == SYNTH80_PATCH_BYTES as usize);
const _: () = assert!(SYNTH106.len() == SYNTH106_PATCH_BYTES as usize);
const _: () = assert!(SYNTH53.len() == SYNTH53_PATCH_BYTES as usize);
const _: () = assert!(SYNTH8.len() == SYNTH8_PATCH_BYTES as usize);

/// Why a patch's bytes are not one `synth`'s reader takes, naming the byte,
/// its control and the range it broke; `bytes` is one whole patch.
pub(super) fn check(synth: Synth, bytes: &[u8]) -> Result<(), String> {
    match synth {
        Synth::Synth106 | Synth::Synth60 => fields(&SYNTH106, bytes),
        Synth::SynthD => fields(&SYNTHD, bytes),
        Synth::Synth53 | Synth::Synth52 | Synth::Synth51 => fields(&SYNTH53, bytes),
        Synth::Synth8 => fields(&SYNTH8, bytes),
        Synth::Synth80 => synth80(bytes),
    }
}

/// synth80's patch: channel I's tone, channel II's, then the rest.
fn synth80(bytes: &[u8]) -> Result<(), String> {
    let named = SYNTH80_TONE
        .iter()
        .map(|&(name, most)| (format!("channel I {name}"), most))
        .chain(SYNTH80_TONE.iter().map(|&(name, most)| (format!("channel II {name}"), most)))
        .chain(SYNTH80_GLOBAL.iter().map(|&(name, most)| (String::from(name), most)));
    for (at, ((name, most), &value)) in named.zip(bytes).enumerate() {
        if value > most {
            return Err(format!("byte {at} ({name}) is {value}: at most {most}"));
        }
    }
    Ok(())
}

/// Each field of `table` within its range in `bytes`.
fn fields(table: &[Field], bytes: &[u8]) -> Result<(), String> {
    for &(at, name, range) in table {
        let start = usize::from(at);
        let Some(field) = bytes.get(start..start.saturating_add(range.bytes())) else {
            return Err(format!("byte {at} ({name}) is missing"));
        };
        if let Some(why) = field_fault(field, range) {
            return Err(format!("byte {at} ({name}) {why}"));
        }
    }
    Ok(())
}

/// Why one field's bytes are outside `range`, if they are.
fn field_fault(field: &[u8], range: Range) -> Option<String> {
    match (range, field) {
        (Range::Byte(most), &[value]) => (value > most).then(|| format!("is {value}: at most {most}")),
        (Range::Bits(mask), &[value]) => (value & !mask != 0).then(|| format!("is {value:#04X}: no bit outside {mask:#04X}")),
        (Range::Synth106Feet, &[value]) => {
            let range_bits = value & SYNTH106_RANGE_MASK;
            (value > SEVEN_BITS || !SYNTH106_RANGE_BITS.contains(&range_bits))
                .then(|| format!("is {value:#04X}: at most 0x7F, with exactly one of the range bits 0x01 (16'), 0x02 (8') and 0x04 (4')"))
        }
        (Range::Signed(most), &[low, high]) => {
            let value = i16::from_le_bytes([low, high]);
            (value.unsigned_abs() > most.unsigned_abs()).then(|| format!("is {value}: -{most} to {most}, the low byte first"))
        }
        (Range::Word(most), &[low, high]) => {
            let value = u16::from_le_bytes([low, high]);
            (value > most).then(|| format!("is {value}: at most {most}, the low byte first"))
        }
        _ => Some(String::from("is not the field's length")),
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{Field, SYNTH8, SYNTH53, SYNTH106, SYNTHD};
    use crate::protocol::Synth;

    /// Every byte of the patch is in exactly one field, in order.
    #[test]
    fn every_table_covers_its_patch_once() {
        for (synth, table) in [(Synth::Synth106, &SYNTH106[..]), (Synth::SynthD, &SYNTHD[..]), (Synth::Synth53, &SYNTH53[..]), (Synth::Synth8, &SYNTH8[..])] {
            let covered: Vec<usize> = table.iter().flat_map(|&(at, _, range): &Field| usize::from(at)..usize::from(at) + range.bytes()).collect();
            assert_eq!(covered, (0..usize::from(synth.patch_bytes())).collect::<Vec<usize>>(), "{synth:?}");
        }
    }
}
