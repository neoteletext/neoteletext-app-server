// The showcase's sounds, synthesized: effects for the games and a
// chromatic octave for the piano, in the engine's audio profile
// (crates/teletext-showcase/src/sound.rs). A session is sent the library
// once, as sound objects; each sound after that is a command.
//
// The synthesis is integer arithmetic only (square, triangle and noise
// waves with linear envelopes), as the Rust showcase's is, so the samples
// are the same.
//
// The server numbers each audio channel's commands itself
// (docs/applications.md section 3.4), so the one count the Rust showcase
// kept across its applications is not needed here.

import type { Context, SampleRate } from "./kit.ts";

/** The library's sample rate: 8 kHz is enough for chip-style effects. */
export const RATE: SampleRate = 8_000;
/** Commands rotate over the profile's channels 0 to 2; channel 3 is the radio's. */
const EFFECT_CHANNELS = 3;
/** How loud effects play, 0 to 100. */
const GAIN = 80;
/** The fade at each end of a sound, in milliseconds, against clicks. */
const FADE_MS = 5;
/** The loudest sample a sound reaches: a quarter of full scale, so mixed sounds do not clip. */
export const PEAK = 8_000;
/** Every sound of the library is generation 1 of its handle. */
const GENERATION = 1;
/** Milliseconds in a second. */
const SECOND_MS = 1_000;

/** The sounds of the library besides the notes. */
export const Effect = {
  /** A falling zap. */
  Laser: "laser",
  /** A burst of noise, fading. */
  Explosion: "explosion",
  /** A short blip. */
  Hit: "hit",
  /** Two rising notes. */
  Coin: "coin",
  /** A rising sweep. */
  Jump: "jump",
  /** A rising arpeggio. */
  Win: "win",
  /** A falling slide. */
  Lose: "lose",
  /** A tick, for keys. */
  Click: "click",
  /** A bell-like tone. */
  Bell: "bell",
} as const;
/** A note of the chromatic octave from middle C: 0 to 12. */
export interface Note {
  readonly note: number;
}
/** A sound of the library. */
export type Effect = (typeof Effect)[keyof typeof Effect] | Note;

/** The notes of the octave. */
export const NOTES = 13;
/** The first note's handle; the twelve after it follow. */
const NOTE_HANDLE = 1;
/** Middle C to the C above, in hundredths of a hertz (equal temperament, A4 = 440 Hz). */
const NOTE_CENTIHERTZ = [26_163, 27_718, 29_366, 31_113, 32_963, 34_923, 36_999, 39_200, 41_530, 44_000, 46_616, 49_388, 52_325] as const;
/** Each effect's handle. */
const HANDLES: Readonly<Record<(typeof Effect)[keyof typeof Effect], number>> = {
  laser: 20,
  explosion: 21,
  hit: 22,
  coin: 23,
  jump: 24,
  win: 25,
  lose: 26,
  click: 27,
  bell: 28,
};
/** The effects, in the library's order. */
const ALL = [Effect.Laser, Effect.Explosion, Effect.Hit, Effect.Coin, Effect.Jump, Effect.Win, Effect.Lose, Effect.Click, Effect.Bell] as const;

/** A note's place in the octave, held to it. */
function noteIndex(note: Note): number {
  return Math.min(Math.max(0, note.note), NOTES - 1);
}

/** The handle `effect` is stored at. */
export function handle(effect: Effect): number {
  return typeof effect === "string" ? HANDLES[effect] : NOTE_HANDLE + noteIndex(effect);
}

const Wave = { Square: "square", Triangle: "triangle" } as const;
type Wave = (typeof Wave)[keyof typeof Wave];

/** A cycle of phase: 65,536 steps. */
const CYCLE = 0x1_0000;
/** Half a cycle. */
const HALF_CYCLE = 0x8000;
/** The phase's 16 bits. */
const PHASE_MASK = 0xffff;
/** Centihertz in a hertz. */
const CENTIHERTZ = 100;

/** Samples in `milliseconds`. */
export function samplesIn(milliseconds: number): number {
  return Math.floor((RATE * milliseconds) / SECOND_MS);
}

/** One sample of `wave` at `phase`, 0 to 65,535 over a cycle. */
function waveAt(wave: Wave, phase: number): number {
  const at = phase & PHASE_MASK;
  if (wave === Wave.Square) {
    return at < HALF_CYCLE ? PEAK : -PEAK;
  }
  // Up from -peak to +peak over the first half, down over the second.
  const rising = at < HALF_CYCLE ? at : PHASE_MASK - at;
  return Math.floor((rising * PEAK * 2) / HALF_CYCLE) - PEAK;
}

/** A linear fade out over the sound's length: sample `index` of `total`. */
function envelope(sample: number, index: number, total: number): number {
  return Math.trunc((sample * (total - index)) / Math.max(total, 1));
}

/** A tone of `centihertz` for `milliseconds`. */
function tone(centihertz: number, milliseconds: number, wave: Wave): number[] {
  const total = samplesIn(milliseconds);
  // The phase step per sample, in 1/65,536ths of a cycle.
  const step = Math.floor((centihertz * CYCLE) / (RATE * CENTIHERTZ));
  let phase = 0;
  return Array.from({ length: total }, (_, index) => {
    const sample = envelope(waveAt(wave, phase), index, total);
    phase = (phase + step) >>> 0;
    return sample;
  });
}

/** A sweep from `from` to `to` hertz over `milliseconds`. */
function sweep(from: number, to: number, milliseconds: number, wave: Wave): number[] {
  const total = samplesIn(milliseconds);
  let phase = 0;
  return Array.from({ length: total }, (_, index) => {
    const hertz = from + Math.trunc(((to - from) * index) / Math.max(total, 1));
    const step = Math.floor((Math.max(hertz, 1) * CYCLE) / RATE);
    const sample = envelope(waveAt(wave, phase), index, total);
    phase = (phase + step) >>> 0;
    return sample;
  });
}

/** The 16-bit linear feedback shift register's first state. */
const NOISE_SEED = 0xace1;
/** Its bits. */
const REGISTER_MASK = 0xffff;
/** Where the feedback bit goes in. */
const FEEDBACK_BIT = 15;

/** Noise for `milliseconds`, from a 16-bit linear feedback shift register (taps 16, 14, 13, 11: maximal length). */
function noise(milliseconds: number): number[] {
  const total = samplesIn(milliseconds);
  let register = NOISE_SEED;
  return Array.from({ length: total }, (_, index) => {
    const bit = (register ^ (register >> 2) ^ (register >> 3) ^ (register >> 5)) & 1;
    register = ((register >> 1) | (bit << FEEDBACK_BIT)) & REGISTER_MASK;
    return envelope((register & 1) === 1 ? PEAK : -PEAK, index, total);
  });
}

/** The arpeggio of the win: C5, E5, G5 and C6, in centihertz. */
const WIN_NOTES = [52_325, 65_926, 78_399, 104_650] as const;

/**
 * The samples of `effect`: each sweep's start and end in hertz, each
 * tone's pitch in centihertz, and each length in milliseconds, as the Rust
 * showcase tuned them.
 */
export function samples(effect: Effect): Int16Array {
  if (typeof effect !== "string") {
    return Int16Array.from(tone(NOTE_CENTIHERTZ[noteIndex(effect)] ?? NOTE_CENTIHERTZ[0], 400, Wave.Triangle));
  }
  switch (effect) {
    case Effect.Laser:
      return Int16Array.from(sweep(1_800, 300, 160, Wave.Square));
    case Effect.Explosion:
      return Int16Array.from(noise(450));
    case Effect.Hit:
      return Int16Array.from(sweep(900, 600, 60, Wave.Square));
    case Effect.Coin:
      return Int16Array.from([...tone(98_800, 70, Wave.Square), ...tone(131_800, 140, Wave.Square)]);
    case Effect.Jump:
      return Int16Array.from(sweep(300, 900, 150, Wave.Triangle));
    case Effect.Win:
      return Int16Array.from(WIN_NOTES.flatMap((note) => tone(note, 110, Wave.Square)));
    case Effect.Lose:
      return Int16Array.from(sweep(500, 120, 500, Wave.Triangle));
    case Effect.Click:
      return Int16Array.from(tone(200_000, 12, Wave.Square));
    case Effect.Bell:
      return Int16Array.from(tone(88_000, 500, Wave.Triangle));
  }
}

/** Every sound of the library, in order: the effects, then the notes. */
export function library(): readonly Effect[] {
  return [...ALL, ...Array.from({ length: NOTES }, (_, note) => ({ note }))];
}

/** Which sessions hold the library, and the channel the next effect uses. */
export class Sounds {
  readonly #library: readonly { readonly handle: number; readonly samples: Int16Array }[];
  readonly #loaded = new Set<number>();
  #channel = 0;

  /** The library, synthesized once. */
  constructor() {
    this.#library = library().map((effect) => ({ handle: handle(effect), samples: samples(effect) }));
  }

  /** Sends the library to `session` if it does not hold it yet: once a session, before its first sound. */
  load(session: number, context: Context): void {
    if (this.#loaded.has(session)) {
      return;
    }
    this.#loaded.add(session);
    for (const sound of this.#library) {
      context.sound({ session }, { object: { handle: sound.handle, generation: GENERATION, rate: RATE, samples: sound.samples } });
    }
  }

  /** Plays `effect` for `session`, sending the library first if needed. */
  play(session: number, effect: Effect, context: Context): void {
    this.load(session, context);
    this.#channel = (this.#channel + 1) % EFFECT_CHANNELS;
    context.sound({ session }, { play: { channel: this.#channel, handle: handle(effect), generation: GENERATION, gain: GAIN, fadeMs: FADE_MS } });
  }

  /** Forgets `session`, which ended. */
  forget(session: number): void {
    this.#loaded.delete(session);
  }
}
