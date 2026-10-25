//! The radio's oscillators, noise and envelopes, in integers only: the
//! showcase is `no_std` with no floating-point library, and every voice is
//! a pure function of its sample offset, so any moment of the stream can
//! be computed without replaying what came before it.

/// The radio's sample rate. 16 kHz is the profile's wideband rate (the
/// engine's `audio.md` section 3.2) and, in 512-byte IMA ADPCM blocks,
/// 64 kbit/s: the stream `SERVER.md` section 9 sizes. 8 kHz images its band
/// above 4 kHz as fizz on tellyboy (`audio.md` section 11).
pub(crate) const RATE: u32 = 16_000;
/// Milliseconds in a second.
pub(crate) const MILLISECONDS: u64 = 1_000;

/// Full scale of a wave: −32 768 to 32 767, a 16-bit sample.
const WAVE_SHIFT: u32 = 15;
/// An envelope's levels run from 0 to this: Q12 fixed point.
pub(crate) const ENVELOPE_ONE: i32 = 1 << ENVELOPE_SHIFT;
/// The envelope's fixed-point shift.
const ENVELOPE_SHIFT: u32 = 12;
/// The middle of a 16-bit unsigned range, which centres a wave on zero.
const MIDDLE: i32 = 0x8000;

/// A waveform of the tracker's voices. Every wave starts at zero, rising,
/// so a note or a drum starts without a click; the bright ones are cut at
/// the fifth harmonic, below the 8 kHz the rate carries, so they do not
/// alias, and IMA ADPCM, which follows a slope but not a step, codes them
/// cleanly (a stepped pulse measured 7 to 14 dB through it, these as the
/// sine-like bass does, about 40).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Wave {
    /// A sine: pure, for the kick's thump and bells.
    Sine,
    /// A triangle: soft, for the bass.
    Triangle,
    /// A square's odd harmonics: hollow.
    Square,
    /// A sawtooth's harmonics: bright and buzzy.
    Saw,
    /// A quarter-cycle pulse's harmonics: the nasal chip lead.
    Pulse,
}

/// A quarter of the 2³² cycle.
const QUARTER: u32 = 1 << 30;
/// Q15's one.
const Q15: i64 = 1 << 15;

/// Each wave's harmonics, first to fifth, in Q15: its Fourier series
/// (square 1/n for odd n; sawtooth 1/n; 25 % pulse sin(nπ/4)/n) cut at the
/// fifth and scaled so the sum's peak is full scale, the peaks of the cut
/// series being 0.933, 1.527 and 1.285 of the first harmonic (computed
/// over 20 000 points of a cycle).
const SQUARE_HARMONICS: [i64; 5] = [35_108, 0, 11_703, 0, 7_022];
/// The sawtooth's.
const SAW_HARMONICS: [i64; 5] = [21_454, 10_727, 7_151, 5_364, 0];
/// The pulse's.
const PULSE_HARMONICS: [i64; 5] = [18_025, 12_746, 6_008, 0, -3_605];

/// A sine at `phase`, full scale, without floating point: the parabola
/// 4t(1 − |t|) over the cycle t from −1 to 1, then the parabola's own
/// correction y + 0.225 (y|y| − y), which brings its error under 0.1 %.
pub(crate) fn sine_at(phase: u32) -> i32 {
    let t = i64::from(i32::from_ne_bytes(phase.to_ne_bytes()) >> 16);
    let parabola = t.saturating_mul(Q15.saturating_sub(t.abs())) >> 13;
    // The correction's 0.225, in thousandths.
    let bent = (parabola.saturating_mul(parabola.abs()) >> 15).saturating_sub(parabola).saturating_mul(225) / 1_000;
    i32::try_from(parabola.saturating_add(bent).clamp(i64::from(i16::MIN), i64::from(i16::MAX))).unwrap_or(0)
}

/// One sample of `wave` at `phase`, where 2³² is one cycle, at full scale.
pub(crate) fn wave_at(wave: Wave, phase: u32) -> i32 {
    let harmonics = |weights: &[i64; 5]| {
        let sum = (1_u32..)
            .zip(weights)
            .filter(|&(_, &weight)| weight != 0)
            .fold(0_i64, |sum, (harmonic, &weight)| sum.saturating_add(i64::from(sine_at(phase.wrapping_mul(harmonic))).saturating_mul(weight) >> 15));
        i32::try_from(sum.clamp(i64::from(i16::MIN), i64::from(i16::MAX))).unwrap_or(0)
    };
    match wave {
        Wave::Sine => sine_at(phase),
        // Up from zero over the first quarter, down over the middle half,
        // up again over the last quarter.
        Wave::Triangle => {
            let top = i32::try_from(phase.wrapping_add(QUARTER) >> 16).unwrap_or(0);
            let rising = if top < MIDDLE { top } else { 0xFFFF_i32.saturating_sub(top) };
            rising.saturating_mul(2).saturating_sub(MIDDLE)
        }
        Wave::Square => harmonics(&SQUARE_HARMONICS),
        Wave::Saw => harmonics(&SAW_HARMONICS),
        Wave::Pulse => harmonics(&PULSE_HARMONICS),
    }
}

/// `value` (a full-scale wave) at `peak`, shaped by `envelope` (0 to
/// [`ENVELOPE_ONE`]).
pub(crate) fn scaled(value: i32, peak: i32, envelope: i32) -> i32 {
    let product = i64::from(value).saturating_mul(i64::from(peak)).saturating_mul(i64::from(envelope));
    i32::try_from(product >> (WAVE_SHIFT.saturating_add(ENVELOPE_SHIFT))).unwrap_or(0)
}

/// MIDI's note numbers, 0 to 127.
pub(crate) const MIDI_NOTES: usize = 128;
/// MIDI's number for A above middle C, the tuning reference.
const MIDI_A4: usize = 69;
/// A4 in millihertz (ISO 16).
const A4_MILLIHERTZ: u64 = 440_000;
/// The equal-tempered semitone, 2^(1/12), in Q32: round(2^(1/12) × 2³²).
const SEMITONE_Q32: u64 = 4_550_359_342;
/// Q32's one.
const Q32_ONE: u64 = 1 << 32;

/// Each MIDI note's phase step per sample at [`RATE`]: the cycle is 2³²,
/// so the step is the frequency × 2³² ÷ the rate. The frequencies are
/// derived from A4 a semitone at a time, so no table of them is kept.
pub(crate) fn note_steps() -> [u32; MIDI_NOTES] {
    let mut millihertz = [0_u64; MIDI_NOTES];
    if let Some(slot) = millihertz.get_mut(MIDI_A4) {
        *slot = A4_MILLIHERTZ;
    }
    let half = Q32_ONE >> 1;
    for note in MIDI_A4.saturating_add(1)..MIDI_NOTES {
        let below = millihertz.get(note.saturating_sub(1)).copied().unwrap_or(0);
        if let Some(slot) = millihertz.get_mut(note) {
            *slot = below.saturating_mul(SEMITONE_Q32).saturating_add(half) >> 32;
        }
    }
    for note in (0..MIDI_A4).rev() {
        let above = millihertz.get(note.saturating_add(1)).copied().unwrap_or(0);
        if let Some(slot) = millihertz.get_mut(note) {
            *slot = (above << 32).saturating_add(SEMITONE_Q32 >> 1).checked_div(SEMITONE_Q32).unwrap_or(0);
        }
    }
    let per_cycle = u64::from(RATE).saturating_mul(MILLISECONDS);
    millihertz.map(|frequency| u32::try_from(frequency.saturating_mul(Q32_ONE).checked_div(per_cycle).unwrap_or(0)).unwrap_or(u32::MAX))
}

/// The phase step of `millihertz` at [`RATE`].
pub(crate) fn step_of(millihertz: u64) -> u64 {
    millihertz.saturating_mul(Q32_ONE).checked_div(u64::from(RATE).saturating_mul(MILLISECONDS)).unwrap_or(0)
}

/// White noise at sample `index`, full scale: the finaliser of `SplitMix64`
/// (Steele, Lea and Flood, "Fast splittable pseudorandom number
/// generators", OOPSLA 2014), a hash of the index, so noise needs no state
/// carried from one sample to the next.
pub(crate) fn noise_at(index: u64) -> i32 {
    // SplitMix64's increment (the golden ratio's 2⁶⁴ fraction) and its two
    // mixing multipliers, from the paper.
    let mut mixed = index.wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^= mixed >> 31;
    i32::try_from(mixed >> 48).unwrap_or(0).saturating_sub(MIDDLE)
}

/// An attack, decay, sustain and release envelope over a note, in samples.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shape {
    /// Samples to rise to the full level.
    pub attack: u32,
    /// Samples to fall from the full level to the sustain.
    pub decay: u32,
    /// The held level, 0 to [`ENVELOPE_ONE`].
    pub sustain: i32,
    /// Samples to fall to silence before the note's end.
    pub release: u32,
}

/// Samples in `milliseconds` at [`RATE`].
pub(crate) const fn samples_in(milliseconds: u32) -> u32 {
    RATE.saturating_mul(milliseconds) / 1_000
}

impl Shape {
    /// The level `offset` samples into a note `length` samples long.
    pub(crate) fn level(self, offset: u32, length: u32) -> i32 {
        if offset >= length {
            return 0;
        }
        let ramp = |done: u32, of: u32| {
            i32::try_from(i64::from(ENVELOPE_ONE).saturating_mul(i64::from(done)).checked_div(i64::from(of.max(1))).unwrap_or(0)).unwrap_or(0)
        };
        let mut level = if offset < self.attack {
            ramp(offset, self.attack)
        } else if offset < self.attack.saturating_add(self.decay) {
            let fallen = ramp(offset.saturating_sub(self.attack), self.decay);
            let span = ENVELOPE_ONE.saturating_sub(self.sustain);
            ENVELOPE_ONE.saturating_sub(scaled_level(span, fallen))
        } else {
            self.sustain
        };
        let left = length.saturating_sub(offset);
        if left < self.release {
            level = scaled_level(level, ramp(left, self.release));
        }
        level
    }
}

/// `level` × `by`, both 0 to [`ENVELOPE_ONE`].
pub(crate) fn scaled_level(level: i32, by: i32) -> i32 {
    level.saturating_mul(by) >> ENVELOPE_SHIFT
}

/// A percussive fall from the full level to silence over `length`
/// samples, quadratic so it dies away as a struck sound does.
pub(crate) fn fall(offset: u32, length: u32) -> i32 {
    if offset >= length {
        return 0;
    }
    let left = i64::from(length.saturating_sub(offset));
    let whole = i64::from(length.max(1));
    let level = i64::from(ENVELOPE_ONE).saturating_mul(left).saturating_mul(left).checked_div(whole.saturating_mul(whole)).unwrap_or(0);
    i32::try_from(level).unwrap_or(0)
}

/// The phase of a sweep from `from` to `to` (phase steps) over `length`
/// samples, `offset` samples in, then steady at `to`: the integral of a
/// falling frequency, in closed form, `from·k − (from − to)·k² ÷ 2L`.
pub(crate) fn sweep_phase(from: u64, to: u64, offset: u32, length: u32) -> u32 {
    let within = u64::from(offset.min(length));
    let span = from.saturating_sub(to);
    let swept = from
        .wrapping_mul(within)
        .wrapping_sub(span.wrapping_mul(within).wrapping_mul(within).checked_div(u64::from(length.max(1)).saturating_mul(2)).unwrap_or(0));
    let after = to.wrapping_mul(u64::from(offset.saturating_sub(length)));
    // The phase is the low 32 bits: a cycle is 2³².
    u32::try_from(swept.wrapping_add(after) & u64::from(u32::MAX)).unwrap_or(0)
}
