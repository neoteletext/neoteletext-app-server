// Sounds made in the program: tones, noise and sequences of them, as 16-bit
// samples at 8 kHz, sent once to each audience as a stored sound object and
// played by a command after (docs/applications.md section 3.4; the
// engine's audio profile).

import type { SampleRate } from "../kit.ts";

/** The rate every sound here is made at: speech and beeps need no more. */
export const RATE: SampleRate = 8_000;

/** The loudest sample made: a quarter of full scale, so sounds mixed together do not clip. */
const PEAK = 8_000;

const MILLISECONDS = 1_000;

/** A sound: its name, unique in the channel, and its samples at {@link RATE}. */
export interface Sound {
  readonly name: string;
  readonly samples: Int16Array;
}

/** A wave's shape. */
export type Wave = "sine" | "square" | "triangle";

function length(milliseconds: number): number {
  return Math.max(1, Math.round((RATE * milliseconds) / MILLISECONDS));
}

/** The fraction of a sound faded in and out at its ends, so it does not click. */
const EDGE = 0.05;

function envelope(index: number, count: number): number {
  const edge = Math.max(1, Math.floor(count * EDGE));
  return Math.min(1, index / edge, (count - 1 - index) / edge);
}

/** A tone of `frequency` Hz for `milliseconds`, at `gain` of the peak. */
export function tone(frequency: number, milliseconds: number, wave: Wave = "sine", gain = 1): Int16Array {
  const count = length(milliseconds);
  const samples = new Int16Array(count);
  for (let index = 0; index < count; index += 1) {
    const phase = (index * frequency) / RATE;
    const fraction = phase - Math.floor(phase);
    const value =
      wave === "sine" ? Math.sin(2 * Math.PI * phase) : wave === "square" ? (fraction < 0.5 ? 1 : -1) : 4 * Math.abs(fraction - 0.5) - 1;
    samples[index] = Math.round(value * PEAK * gain * envelope(index, count));
  }
  return samples;
}

/** Noise for `milliseconds`, from a fixed seed, so the same sound is made each time. */
export function noise(milliseconds: number, gain = 1): Int16Array {
  const count = length(milliseconds);
  const samples = new Int16Array(count);
  let state = 0x2545;
  for (let index = 0; index < count; index += 1) {
    // A 16-bit Galois LFSR, taps 16, 14, 13, 11.
    state = (state >> 1) ^ (-(state & 1) & 0xb400);
    samples[index] = Math.round(((state / 0xffff) * 2 - 1) * PEAK * gain * envelope(index, count));
  }
  return samples;
}

/** Sounds one after another. */
export function sequence(...parts: readonly Int16Array[]): Int16Array {
  const out = new Int16Array(parts.reduce((sum, part) => sum + part.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

/** A sound named `name`. */
export function sound(name: string, samples: Int16Array): Sound {
  return { name, samples };
}
