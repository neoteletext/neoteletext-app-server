// The radio's oscillators, noise and envelopes, in integers only
// (crates/teletext-showcase/src/audio/synth.rs): every voice is a pure
// function of its sample offset, so any moment of the stream can be
// computed without replaying what came before it, and the samples are the
// Rust showcase's exactly.
//
// Every value here stays well inside 2^53, so `number` holds it exactly;
// a shift of a signed value is a division by a power of two, floored, as
// Rust's `>>` on a signed integer floors. The 64-bit arithmetic of the
// tuning and the noise is BigInt.

import type { SampleRate } from "../kit.ts";

/**
 * The radio's sample rate. 16 kHz is the profile's wideband rate (the
 * engine's `audio.md` section 3.2) and, in 512-byte IMA ADPCM blocks,
 * 64 kbit/s: the stream `SERVER.md` section 9 sizes. 8 kHz images its band
 * above 4 kHz as fizz on tellyboy (`audio.md` section 11).
 */
export const RATE: SampleRate = 16_000;
/** Milliseconds in a second. */
export const MILLISECONDS = 1_000;

/** Full scale of a wave: -32,768 to 32,767, a 16-bit sample. */
const WAVE_SHIFT = 15;
/** The envelope's fixed-point shift. */
const ENVELOPE_SHIFT = 12;
/** An envelope's levels run from 0 to this: Q12 fixed point. */
export const ENVELOPE_ONE = 2 ** ENVELOPE_SHIFT;
/** The middle of a 16-bit unsigned range, which centres a wave on zero. */
const MIDDLE = 0x8000;
/** The lowest 16-bit sample. */
export const SAMPLE_LOWEST = -0x8000;
/** The highest 16-bit sample. */
export const SAMPLE_HIGHEST = 0x7fff;
/** The highest 16-bit unsigned value. */
const UNSIGNED_16_HIGHEST = 0xffff;

/**
 * A waveform of the tracker's voices. Every wave starts at zero, rising,
 * so a note or a drum starts without a click; the bright ones are cut at
 * the fifth harmonic, below the 8 kHz the rate carries, so they do not
 * alias, and IMA ADPCM, which follows a slope but not a step, codes them
 * cleanly (a stepped pulse measured 7 to 14 dB through it, these as the
 * sine-like bass does, about 40).
 */
export const Wave = {
  /** A sine: pure, for the kick's thump and bells. */
  Sine: "sine",
  /** A triangle: soft, for the bass. */
  Triangle: "triangle",
  /** A square's odd harmonics: hollow. */
  Square: "square",
  /** A sawtooth's harmonics: bright and buzzy. */
  Saw: "saw",
  /** A quarter-cycle pulse's harmonics: the nasal chip lead. */
  Pulse: "pulse",
} as const;
/** A waveform. */
export type Wave = (typeof Wave)[keyof typeof Wave];

/** A quarter of the 2^32 cycle. */
const QUARTER = 2 ** 30;
/** Q15's one. */
const Q15 = 2 ** 15;
/** A phase is 32 bits: one cycle. */
const CYCLE = 2 ** 32;
/** The parabola's product is Q28; this brings it to Q15. */
const PARABOLA_SHIFT = 13;
/** The upper half of a 32-bit phase, as a signed 16-bit cycle position. */
const PHASE_TOP_SHIFT = 16;

/** Each wave's harmonics, first to fifth, in Q15: its Fourier series
 * (square 1/n for odd n; sawtooth 1/n; 25 % pulse sin(nπ/4)/n) cut at the
 * fifth and scaled so the sum's peak is full scale, the peaks of the cut
 * series being 0.933, 1.527 and 1.285 of the first harmonic (computed
 * over 20 000 points of a cycle). */
const SQUARE_HARMONICS = [35_108, 0, 11_703, 0, 7_022] as const;
/** The sawtooth's. */
const SAW_HARMONICS = [21_454, 10_727, 7_151, 5_364, 0] as const;
/** The pulse's. */
const PULSE_HARMONICS = [18_025, 12_746, 6_008, 0, -3_605] as const;

/** The sine's correction factor, 0.225, in thousandths. */
const CORRECTION_THOUSANDTHS = 225;
/** Thousandths in one. */
const THOUSAND = 1_000;

/** `value` held to a 16-bit sample. */
export function clampSample(value: number): number {
  return Math.min(Math.max(value, SAMPLE_LOWEST), SAMPLE_HIGHEST);
}

/** `value` shifted right by `bits`, floored, as Rust's `>>` on a signed integer. */
function shiftDown(value: number, bits: number): number {
  return Math.floor(value / 2 ** bits);
}

/**
 * A sine at `phase`, full scale, without floating point: the parabola
 * 4t(1 - |t|) over the cycle t from -1 to 1, then the parabola's own
 * correction y + 0.225 (y|y| - y), which brings its error under 0.1 %.
 */
export function sineAt(phase: number): number {
  // The phase's 32 bits as a signed number, its top 16 bits kept.
  const t = (phase | 0) >> PHASE_TOP_SHIFT;
  const parabola = shiftDown(t * (Q15 - Math.abs(t)), PARABOLA_SHIFT);
  const bent = Math.trunc(((shiftDown(parabola * Math.abs(parabola), WAVE_SHIFT) - parabola) * CORRECTION_THOUSANDTHS) / THOUSAND);
  return clampSample(parabola + bent);
}

/** The sum of the first five harmonics of `phase` with `weights`, in Q15. */
function harmonics(phase: number, weights: readonly number[]): number {
  let sum = 0;
  weights.forEach((weight, index) => {
    if (weight !== 0) {
      const harmonic = index + 1;
      sum += shiftDown(sineAt(Math.imul(phase, harmonic) >>> 0) * weight, WAVE_SHIFT);
    }
  });
  return clampSample(sum);
}

/** One sample of `wave` at `phase`, where 2^32 is one cycle, at full scale. */
export function waveAt(wave: Wave, phase: number): number {
  switch (wave) {
    case Wave.Sine:
      return sineAt(phase);
    case Wave.Triangle: {
      // Up from zero over the first quarter, down over the middle half,
      // up again over the last quarter.
      const top = ((phase + QUARTER) % CYCLE) >>> PHASE_TOP_SHIFT;
      const rising = top < MIDDLE ? top : UNSIGNED_16_HIGHEST - top;
      return rising * 2 - MIDDLE;
    }
    case Wave.Square:
      return harmonics(phase, SQUARE_HARMONICS);
    case Wave.Saw:
      return harmonics(phase, SAW_HARMONICS);
    case Wave.Pulse:
      return harmonics(phase, PULSE_HARMONICS);
  }
}

/** `value` (a full-scale wave) at `peak`, shaped by `envelope` (0 to {@link ENVELOPE_ONE}). */
export function scaled(value: number, peak: number, envelope: number): number {
  return shiftDown(value * peak * envelope, WAVE_SHIFT + ENVELOPE_SHIFT);
}

/** MIDI's note numbers, 0 to 127. */
export const MIDI_NOTES = 128;
/** MIDI's number for A above middle C, the tuning reference. */
const MIDI_A4 = 69;
/** A4 in millihertz (ISO 16). */
const A4_MILLIHERTZ = 440_000n;
/** The equal-tempered semitone, 2^(1/12), in Q32: round(2^(1/12) × 2^32). */
const SEMITONE_Q32 = 4_550_359_342n;
/** Q32's one. */
const Q32_ONE = 1n << 32n;
/** The largest 32-bit unsigned value. */
const U32_MAX = 0xffff_ffff;

/**
 * Each MIDI note's phase step per sample at {@link RATE}: the cycle is
 * 2^32, so the step is the frequency × 2^32 ÷ the rate. The frequencies
 * are derived from A4 a semitone at a time, so no table of them is kept.
 */
export function noteSteps(): number[] {
  const millihertz = new Array<bigint>(MIDI_NOTES).fill(0n);
  millihertz[MIDI_A4] = A4_MILLIHERTZ;
  const half = Q32_ONE >> 1n;
  for (let note = MIDI_A4 + 1; note < MIDI_NOTES; note += 1) {
    millihertz[note] = ((millihertz[note - 1] ?? 0n) * SEMITONE_Q32 + half) >> 32n;
  }
  for (let note = MIDI_A4 - 1; note >= 0; note -= 1) {
    millihertz[note] = (((millihertz[note + 1] ?? 0n) << 32n) + (SEMITONE_Q32 >> 1n)) / SEMITONE_Q32;
  }
  const perCycle = BigInt(RATE) * BigInt(MILLISECONDS);
  return millihertz.map((frequency) => {
    const step = (frequency * Q32_ONE) / perCycle;
    return step > BigInt(U32_MAX) ? U32_MAX : Number(step);
  });
}

/** The phase step of `millihertz` at {@link RATE}. */
export function stepOf(millihertz: number): number {
  return Number((BigInt(millihertz) * Q32_ONE) / (BigInt(RATE) * BigInt(MILLISECONDS)));
}

/** SplitMix64's increment: the golden ratio's 2^64 fraction (Steele, Lea and Flood, OOPSLA 2014). */
const SPLITMIX_INCREMENT = 0x9e37_79b9_7f4a_7c15n;
/** Its first mixing multiplier, from the paper. */
const SPLITMIX_FIRST = 0xbf58_476d_1ce4_e5b9n;
/** Its second. */
const SPLITMIX_SECOND = 0x94d0_49bb_1331_11ebn;
/** Its three shifts. */
const SPLITMIX_SHIFT_A = 30n;
const SPLITMIX_SHIFT_B = 27n;
const SPLITMIX_SHIFT_C = 31n;
/** The hash's top 16 bits are the sample. */
const NOISE_SHIFT = 48n;
/** The bits of a u64. */
const U64_BITS = 64;

/**
 * White noise at sample `index`, full scale: the finaliser of SplitMix64
 * (Steele, Lea and Flood, "Fast splittable pseudorandom number
 * generators", OOPSLA 2014), a hash of the index, so noise needs no state
 * carried from one sample to the next. `index` -1 is the u64's last, as
 * Rust's wrapping subtraction gives.
 */
export function noiseAt(index: number): number {
  let mixed = BigInt.asUintN(U64_BITS, BigInt(index) + SPLITMIX_INCREMENT);
  mixed = BigInt.asUintN(U64_BITS, (mixed ^ (mixed >> SPLITMIX_SHIFT_A)) * SPLITMIX_FIRST);
  mixed = BigInt.asUintN(U64_BITS, (mixed ^ (mixed >> SPLITMIX_SHIFT_B)) * SPLITMIX_SECOND);
  mixed ^= mixed >> SPLITMIX_SHIFT_C;
  return Number(mixed >> NOISE_SHIFT) - MIDDLE;
}

/** An attack, decay, sustain and release envelope over a note, in samples. */
export interface Shape {
  /** Samples to rise to the full level. */
  readonly attack: number;
  /** Samples to fall from the full level to the sustain. */
  readonly decay: number;
  /** The held level, 0 to {@link ENVELOPE_ONE}. */
  readonly sustain: number;
  /** Samples to fall to silence before the note's end. */
  readonly release: number;
}

/** Samples in `milliseconds` at {@link RATE}. */
export function samplesIn(milliseconds: number): number {
  return Math.floor((RATE * milliseconds) / MILLISECONDS);
}

/** `done` of `of` as an envelope level. */
function ramp(done: number, of: number): number {
  return Math.floor((ENVELOPE_ONE * done) / Math.max(of, 1));
}

/** The level of `shape` `offset` samples into a note `length` samples long. */
export function shapeLevel(shape: Shape, offset: number, length: number): number {
  if (offset >= length) {
    return 0;
  }
  let level: number;
  if (offset < shape.attack) {
    level = ramp(offset, shape.attack);
  } else if (offset < shape.attack + shape.decay) {
    const fallen = ramp(offset - shape.attack, shape.decay);
    const span = ENVELOPE_ONE - shape.sustain;
    level = ENVELOPE_ONE - scaledLevel(span, fallen);
  } else {
    level = shape.sustain;
  }
  const left = length - offset;
  if (left < shape.release) {
    level = scaledLevel(level, ramp(left, shape.release));
  }
  return level;
}

/** `level` × `by`, both 0 to {@link ENVELOPE_ONE}. */
export function scaledLevel(level: number, by: number): number {
  return shiftDown(level * by, ENVELOPE_SHIFT);
}

/**
 * A percussive fall from the full level to silence over `length`
 * samples, quadratic so it dies away as a struck sound does.
 */
export function fall(offset: number, length: number): number {
  if (offset >= length) {
    return 0;
  }
  const left = length - offset;
  const whole = Math.max(length, 1);
  return Math.floor((ENVELOPE_ONE * left * left) / (whole * whole));
}

/**
 * The phase of a sweep from `from` to `to` (phase steps) over `length`
 * samples, `offset` samples in, then steady at `to`: the integral of a
 * falling frequency, in closed form, `from·k - (from - to)·k² ÷ 2L`. The
 * terms stay under 2^53 for the drums' sweeps (a step under 2^26, a sweep
 * under 2^10 samples) and the sum is never negative, so no 64-bit wrap is
 * reached.
 */
export function sweepPhase(from: number, to: number, offset: number, length: number): number {
  const within = Math.min(offset, length);
  const span = Math.max(0, from - to);
  const swept = from * within - Math.floor((span * within * within) / (Math.max(length, 1) * 2));
  const after = to * Math.max(0, offset - length);
  // The phase is the low 32 bits: a cycle is 2^32.
  return (swept + after) % CYCLE;
}
