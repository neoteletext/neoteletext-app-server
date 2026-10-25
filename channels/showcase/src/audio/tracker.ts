// The radio's tracker (crates/teletext-showcase/src/audio/tracker.rs): the
// playlist of tunes.ts read once into rows, and every sample of the
// endless stream computed from its index alone. Six voices: a lead with a
// tempo-synced echo, a chord arpeggio, a bass, and kick, snare and
// hi-hats made from a falling sweep and noise.

import {
  ENVELOPE_ONE,
  MIDI_NOTES,
  RATE,
  type Shape,
  Wave,
  clampSample,
  fall,
  noiseAt,
  noteSteps,
  samplesIn,
  scaled,
  scaledLevel,
  shapeLevel,
  stepOf,
  sweepPhase,
  waveAt,
} from "./synth.ts";
import { PLAYLIST, type Tune } from "./tunes.ts";

/** Rows in a bar: sixteenth notes. */
export const ROWS_PER_BAR = 16;
/** Rows in a beat. */
const ROWS_PER_BEAT = 4;
/** Seconds in a minute, for the tempo. */
const SECONDS_PER_MINUTE = 60;
/** Semitones in an octave. */
const OCTAVE = 12;

/** The voices, in the order the page's bars show them: low to high, as a spectrum is read. */
export const VOICES = 6;
/** The bass. */
export const BASS = 0;
/** The kick drum. */
export const KICK = 1;
/** The lead. */
export const LEAD = 2;
/** The arpeggio. */
export const ARPEGGIO = 3;
/** The snare. */
export const SNARE = 4;
/** The hi-hats. */
export const HATS = 5;

// Each voice's loudest sample. Their sum, with the echo, is 30 350, under
// 16-bit full scale, so the mix never clips.
/** The lead. */
const LEAD_PEAK = 5_200;
/** The echo, as a share of the lead: 3/8. */
const ECHO_EIGHTHS = 3;
/** Eighths in one. */
const EIGHTHS = 8;
/** The arpeggio. */
const ARPEGGIO_PEAK = 2_000;
/** The bass. */
const BASS_PEAK = 6_500;
/** The kick. */
const KICK_PEAK = 7_500;
/** The snare's noise. */
const SNARE_NOISE_PEAK = 3_800;
/** The snare's drum tone. */
const SNARE_TONE_PEAK = 1_800;
/** The hi-hats. */
const HATS_PEAK = 1_600;
/** The loudest the mix can be: what the meters scale to. */
export const MIX_PEAK =
  LEAD_PEAK + Math.trunc((LEAD_PEAK * ECHO_EIGHTHS) / EIGHTHS) + ARPEGGIO_PEAK + BASS_PEAK + KICK_PEAK + SNARE_NOISE_PEAK + SNARE_TONE_PEAK + HATS_PEAK;
if (MIX_PEAK > 0x7fff) {
  throw new Error("the voices' peaks must not clip");
}

/** The echo's delay, in rows: a dotted eighth. */
const ECHO_ROWS = 3;
/** The lead's envelope: a quick attack, a fall to 60 %, a short release. */
const LEAD_SHAPE: Shape = { attack: samplesIn(4), decay: samplesIn(120), sustain: Math.trunc((ENVELOPE_ONE * 3) / 5), release: samplesIn(30) };
/** The bass's envelope: plucked, then held at 70 %. */
const BASS_SHAPE: Shape = { attack: samplesIn(3), decay: samplesIn(80), sustain: Math.trunc((ENVELOPE_ONE * 7) / 10), release: samplesIn(20) };
/** The kick's length. */
const KICK_SAMPLES = samplesIn(160);
/** The kick's sweep, from 150 Hz to 45 Hz over its first 60 ms: the thump of a drum machine's kick. */
const KICK_FROM_MILLIHERTZ = 150_000;
/** Where the kick's sweep ends. */
const KICK_TO_MILLIHERTZ = 45_000;
/** How long the kick sweeps. */
const KICK_SWEEP_SAMPLES = samplesIn(60);
/** The snare's noise. */
const SNARE_SAMPLES = samplesIn(140);
/** The snare's drum tone, and its pitch. */
const SNARE_TONE_SAMPLES = samplesIn(60);
/** The snare's drum head, 190 Hz. */
const SNARE_TONE_MILLIHERTZ = 190_000;
/** A closed hi-hat. */
const HAT_SAMPLES = samplesIn(35);
/** An open hi-hat. */
const OPEN_HAT_SAMPLES = samplesIn(160);
/** Rows back a drum's sound may still ring: the longest sound over the shortest row, the fastest tune's. */
const DRUM_ROWS = 3;
/** The arpeggio's octave: chord tones from middle C up. */
const ARPEGGIO_BASE = 60;
/** The bass's range: the root lies from G2 (MIDI 43) to F♯3 (54), low enough to be bass and high enough for small speakers. */
const BASS_LOWEST = 43;
/** The bass range's top. */
const BASS_HIGHEST = 54;
/** The C three octaves below middle C: the bass root's base. */
const BASS_C = 48;
/** Chord tones the arpeggio climbs, one a row. */
const CHORD_TONES = 4;

/** The kick's sweep, in phase steps, computed once. */
const KICK_FROM_STEP = stepOf(KICK_FROM_MILLIHERTZ);
const KICK_TO_STEP = stepOf(KICK_TO_MILLIHERTZ);
/** The snare head's phase step. */
const SNARE_TONE_STEP = stepOf(SNARE_TONE_MILLIHERTZ);
/** A phase is 32 bits: one cycle. */
const CYCLE = 2 ** 32;

/** A note held over rows. */
interface Span {
  /** The row it starts. */
  readonly start: number;
  /** Rows it lasts. */
  rows: number;
  /** Its MIDI note. */
  readonly note: number;
}

/** A row of a part: a new note, the note before held, or silence. */
export type Cell = { readonly note: number } | "hold" | "rest";

/** A chord: its name as written, its root's pitch class and its four tones above the root. */
export interface Chord {
  readonly name: string;
  readonly root: number;
  readonly tones: readonly number[];
}

/** Pitch classes of the letters A to G. */
const LETTERS: ReadonlyMap<string, number> = new Map([
  ["c", 0],
  ["d", 2],
  ["e", 4],
  ["f", 5],
  ["g", 7],
  ["a", 9],
  ["b", 11],
]);

/** ASCII's upper-case letters, which Rust's `to_ascii_lowercase` folds and nothing else. */
const ASCII_UPPER = /^[A-Z]$/u;

/** `character` in lower case if it is an ASCII letter, as Rust's `char::to_ascii_lowercase`. */
export function asciiLower(character: string): string {
  return ASCII_UPPER.test(character) ? character.toLowerCase() : character;
}

/** The pitch class of a note letter. */
function pitchClass(letter: string | undefined): number | undefined {
  return letter === undefined ? undefined : LETTERS.get(asciiLower(letter));
}

/** A chord's tones above its root: root, third, fifth, and the seventh or the octave. */
const QUALITIES: ReadonlyMap<string, readonly number[]> = new Map([
  ["m", [0, 3, 7, 12]],
  ["7", [0, 4, 7, 10]],
  ["m7", [0, 3, 7, 10]],
]);
/** A major chord's: anything unreadable is one. */
const MAJOR = [0, 4, 7, 12] as const;

/** A chord from its name; an unreadable name is C major. */
export function parseChord(name: string): Chord {
  const characters = [...name];
  let root = pitchClass(characters[0]) ?? 0;
  let rest = 1;
  if (characters[1] === "#") {
    root = (root + 1) % OCTAVE;
    rest += 1;
  } else if (characters[1] === "b") {
    root = (root + OCTAVE - 1) % OCTAVE;
    rest += 1;
  }
  const quality = characters.slice(rest).join("");
  return { name, root, tones: QUALITIES.get(quality) ?? MAJOR };
}

/** The `index`th tone (0 to 3) of `chord` above `base`'s octave. */
function chordTone(chord: Chord, base: number, index: number): number {
  return base + chord.root + (chord.tones[index] ?? 0);
}

/** The bass's root: in its range. */
function bassRoot(chord: Chord): number {
  const root = BASS_C + chord.root;
  return root > BASS_HIGHEST ? root - OCTAVE : Math.max(root, BASS_LOWEST);
}

/** The largest u8, the type Rust reads an octave and a note into. */
const U8_MAX = 0xff;
/** An octave as Rust's `str::parse::<u8>` reads it: an optional plus and decimal digits. */
const OCTAVE_DIGITS = /^\+?[0-9]+$/u;

/** A melody token as a cell; anything unreadable is undefined. */
export function melodyCell(token: string): Cell | undefined {
  if (token === "-") {
    return "hold";
  }
  if (token === ".") {
    return "rest";
  }
  const characters = [...token];
  let pitch = pitchClass(characters[0]);
  if (pitch === undefined) {
    return undefined;
  }
  let rest = characters.slice(1).join("");
  if (rest.startsWith("#")) {
    pitch += 1;
    rest = rest.slice(1);
  }
  if (!OCTAVE_DIGITS.test(rest)) {
    return undefined;
  }
  const octave = Number.parseInt(rest, 10);
  // MIDI numbers C-1 as 0, so middle C, `c4`, is 60; each step is checked
  // as a u8, as the Rust's checked arithmetic is.
  const above = octave + 1;
  const octaveStart = above * OCTAVE;
  const note = octaveStart + pitch;
  if (octave > U8_MAX || above > U8_MAX || octaveStart > U8_MAX || note > U8_MAX || note >= MIDI_NOTES) {
    return undefined;
  }
  return { note };
}

/** A bass token over `chord` as a cell. */
function bassCell(token: string, chord: Chord): Cell {
  const root = bassRoot(chord);
  const tone = (index: number): Cell => ({ note: root + (chord.tones[index] ?? 0) });
  switch (token) {
    case "R":
      return { note: root };
    case "T":
      return tone(1);
    case "F":
      return tone(2);
    case "S":
      return tone(3);
    case "O":
      return { note: root + OCTAVE };
    case "-":
      return "hold";
    default:
      return "rest";
  }
}

/** Each row's span: the note sounding there, if any, each row of a span naming the whole span. */
function spans(cells: readonly Cell[]): (Span | null)[] {
  const rows: (Span | null)[] = [];
  let current: Span | null = null;
  cells.forEach((cell, row) => {
    if (cell === "hold") {
      if (current !== null) {
        current.rows += 1;
      }
    } else if (cell === "rest") {
      current = null;
    } else {
      current = { start: row, rows: 1, note: cell.note };
    }
    rows.push(current);
  });
  // Every row of a span holds the one object, so each names the whole
  // span once it is read.
  return rows;
}

/** The bits of a drum bar: one a row. */
const BAR_BITS = 16;

/** A drum bar as a bit set of its rows holding `hit`. */
function pattern(text: string, hit: string): number {
  return [...text].slice(0, BAR_BITS).reduce((bits, character, row) => (character === hit ? bits | (1 << row) : bits), 0);
}

/** Whitespace-separated tokens, as Rust's `split_whitespace`. */
function tokens(text: string): string[] {
  return text.split(/\s+/u).filter((piece) => piece.length > 0);
}

/** A tune read into rows. */
class Arranged {
  readonly tune: Tune;
  /** Samples in a row, at the tune's tempo. */
  readonly rowSamples: number;
  /** Rows in one pass of the melody. */
  readonly passRows: number;
  readonly lead: readonly (Span | null)[];
  readonly bass: readonly (Span | null)[];
  readonly chords: readonly Chord[];
  readonly kick: number;
  readonly snare: number;
  readonly fill: number;
  readonly hats: number;
  readonly openHats: number;
  /** Where it starts in the playlist's cycle, in samples. */
  readonly start: number;
  /** Its length in samples. */
  readonly length: number;

  constructor(tune: Tune, start: number) {
    this.tune = tune;
    const perRow = tune.tempo * ROWS_PER_BEAT;
    this.rowSamples = Math.max(perRow === 0 ? 1 : Math.floor((RATE * SECONDS_PER_MINUTE) / perRow), 1);
    this.chords = tune.chords.map(parseChord);
    const lead = tune.melody.flatMap((bar) => tokens(bar).map((token) => melodyCell(token) ?? "rest"));
    const bass = this.chords.flatMap((chord) => tokens(tune.bass).map((token) => bassCell(token, chord)));
    this.passRows = tune.melody.length * ROWS_PER_BAR;
    this.lead = spans(lead);
    this.bass = spans(bass);
    this.kick = pattern(tune.kick, "x");
    this.snare = pattern(tune.snare, "x");
    this.fill = tune.fill.length === 0 ? this.snare : pattern(tune.fill, "x");
    this.hats = pattern(tune.hats, "x");
    this.openHats = pattern(tune.hats, "o");
    this.start = start;
    this.length = this.rowSamples * this.passRows * tune.passes;
  }

  /** The tune row of sample `offset` into the tune, and the sample within the row. */
  rowOf(offset: number): [row: number, within: number] {
    return [Math.floor(offset / this.rowSamples), offset % this.rowSamples];
  }

  /** The row of the pass, and the pass, of tune row `row`. */
  inPass(row: number): [passRow: number, pass: number] {
    return this.passRows === 0 ? [0, 0] : [row % this.passRows, Math.floor(row / this.passRows)];
  }

  /** The span sounding at pass row `row`, `within` samples into the row, as (note, offset into the note, the note's length). */
  sounding(part: readonly (Span | null)[], row: number, within: number): [note: number, offset: number, length: number] | undefined {
    const span = part[row];
    if (span === null || span === undefined) {
      return undefined;
    }
    return [span.note, (row - span.start) * this.rowSamples + within, span.rows * this.rowSamples];
  }

  /** Whether `drum` hits at tune row `row`; the snare plays its fill in each pass's last bar. */
  hits(row: number, drum: number): boolean {
    const [passRow] = this.inPass(row);
    const step = passRow % ROWS_PER_BAR;
    const lastBar = Math.floor(passRow / ROWS_PER_BAR) === Math.max(0, Math.floor(this.passRows / ROWS_PER_BAR) - 1);
    let bits = 0;
    if (drum === KICK) {
      bits = this.kick;
    } else if (drum === SNARE) {
      bits = lastBar ? this.fill : this.snare;
    } else if (drum === HATS) {
      bits = this.hats | this.openHats;
    }
    return (bits & (1 << step)) !== 0;
  }

  /** The latest hit of `drum` at or before tune row `row`, within the rows a drum rings, as (its row, samples since). */
  latestHit(row: number, within: number, drum: number): [hit: number, since: number] | undefined {
    for (let back = 0; back <= Math.min(DRUM_ROWS, row); back += 1) {
      const hit = row - back;
      if (this.hits(hit, drum)) {
        return [hit, back * this.rowSamples + within];
      }
    }
    return undefined;
  }

  openHat(row: number): boolean {
    const [passRow] = this.inPass(row);
    return (this.openHats & (1 << passRow % ROWS_PER_BAR)) !== 0;
  }
}

/** One voice at one sample: its value, and its envelope's level (0 to {@link ENVELOPE_ONE}) for the page's meters. */
export interface Voice {
  readonly value: number;
  readonly level: number;
}

/** A silent voice. */
const SILENT: Voice = { value: 0, level: 0 };

/** Where the stream is in the playlist. */
export interface Place {
  /** The tune playing. */
  readonly tune: Tune;
  /** The next tune that is not the ident. */
  readonly next: Tune;
  /** The bar playing, from 0, over every pass. */
  readonly bar: number;
  /** Bars in the whole tune. */
  readonly bars: number;
  /** The chord playing. */
  readonly chord: string;
  /** The lead's note, if it is sounding. */
  readonly lead: number | undefined;
  /** How far through the tune, in thousandths. */
  readonly thousandths: number;
}

/** Thousandths in one. */
const THOUSAND = 1_000;

/** The playlist, read once, and the notes' phase steps. */
export class Tracker {
  readonly #tunes: readonly Arranged[];
  /** The playlist's length in samples, after which it loops. */
  readonly #cycle: number;
  readonly #steps: readonly number[] = noteSteps();

  /** The playlist, read into rows. */
  constructor() {
    const tunes: Arranged[] = [];
    let start = 0;
    for (const tune of PLAYLIST) {
      const arranged = new Arranged(tune, start);
      start += arranged.length;
      tunes.push(arranged);
    }
    this.#tunes = tunes;
    this.#cycle = start;
  }

  /** The playlist's length, in samples. */
  cycle(): number {
    return this.#cycle;
  }

  /** The tune at stream sample `index`, its place in the playlist, and the sample within it. */
  #locate(index: number): [position: number, tune: Arranged, offset: number] | undefined {
    if (this.#cycle === 0) {
      return undefined;
    }
    const offset = index % this.#cycle;
    const position = this.#tunes.findIndex((tune) => offset >= tune.start && offset < tune.start + tune.length);
    const tune = this.#tunes[position];
    return tune === undefined ? undefined : [position, tune, offset - tune.start];
  }

  #step(note: number): number {
    return this.#steps[note] ?? 0;
  }

  /** The lead `offset` samples into tune `tune`. */
  #lead(tune: Arranged, offset: number): Voice {
    const [row, within] = tune.rowOf(offset);
    const [passRow] = tune.inPass(row);
    const sounding = tune.sounding(tune.lead, passRow, within);
    if (sounding === undefined) {
      return SILENT;
    }
    const [note, at, length] = sounding;
    const level = shapeLevel(LEAD_SHAPE, at, length);
    return { value: scaled(waveAt(tune.tune.lead, Math.imul(this.#step(note), at) >>> 0), LEAD_PEAK, level), level };
  }

  /** Every voice at stream sample `index`. */
  voices(index: number): Voice[] {
    const voices: Voice[] = new Array<Voice>(VOICES).fill(SILENT);
    const located = this.#locate(index);
    if (located === undefined) {
      return voices;
    }
    const [, tune, offset] = located;
    const [row, within] = tune.rowOf(offset);
    const [passRow, pass] = tune.inPass(row);
    const bar = Math.floor(passRow / ROWS_PER_BAR);

    const lead = this.#lead(tune, offset);
    const echoSamples = tune.rowSamples * ECHO_ROWS;
    let leadValue = lead.value;
    if (offset >= echoSamples) {
      const echo = this.#lead(tune, offset - echoSamples);
      leadValue += Math.trunc((echo.value * ECHO_EIGHTHS) / EIGHTHS);
    }
    voices[LEAD] = { value: leadValue, level: lead.level };

    const bass = tune.sounding(tune.bass, passRow, within);
    if (bass !== undefined) {
      const [note, at, length] = bass;
      const level = shapeLevel(BASS_SHAPE, at, length);
      voices[BASS] = { value: scaled(waveAt(Wave.Triangle, Math.imul(this.#step(note), at) >>> 0), BASS_PEAK, level), level };
    }

    const chord = tune.chords[bar];
    if (pass >= tune.tune.arpeggioFrom && chord !== undefined) {
      // Up the chord's four tones, one a row.
      const note = chordTone(chord, ARPEGGIO_BASE, passRow % CHORD_TONES);
      const level = fall(within, tune.rowSamples);
      voices[ARPEGGIO] = { value: scaled(waveAt(Wave.Square, Math.imul(this.#step(note), within) >>> 0), ARPEGGIO_PEAK, level), level };
    }

    const kick = tune.latestHit(row, within, KICK);
    if (kick !== undefined) {
      const [, since] = kick;
      const phase = sweepPhase(KICK_FROM_STEP, KICK_TO_STEP, since, KICK_SWEEP_SAMPLES);
      const level = fall(since, KICK_SAMPLES);
      voices[KICK] = { value: scaled(waveAt(Wave.Sine, phase), KICK_PEAK, level), level };
    }

    const snare = tune.latestHit(row, within, SNARE);
    if (snare !== undefined) {
      const [, since] = snare;
      const level = fall(since, SNARE_SAMPLES);
      const tonePhase = (SNARE_TONE_STEP * since) % CYCLE;
      const head = scaled(waveAt(Wave.Sine, tonePhase), SNARE_TONE_PEAK, fall(since, SNARE_TONE_SAMPLES));
      // Silent noise is skipped: scaled by a level of 0 it is 0 anyway.
      const noise = level === 0 ? 0 : scaled(noiseAt(index), SNARE_NOISE_PEAK, level);
      voices[SNARE] = { value: noise + head, level };
    }

    const hats = tune.latestHit(row, within, HATS);
    if (hats !== undefined) {
      const [hit, since] = hats;
      const length = tune.openHat(hit) ? OPEN_HAT_SAMPLES : HAT_SAMPLES;
      const level = fall(since, length);
      // Noise less the sample before: a first difference, which keeps the
      // hiss and drops the rumble, as a hi-hat sounds. Skipped when
      // silent, as the snare's is.
      const bright = level === 0 ? 0 : Math.trunc((noiseAt(index) - noiseAt(index - 1)) / 2);
      voices[HATS] = { value: scaled(bright, HATS_PEAK, level), level };
    }
    return voices;
  }

  /** Stream sample `index`. */
  sample(index: number): number {
    return clampSample(this.voices(index).reduce((sum, voice) => sum + voice.value, 0));
  }

  /** `count` samples from stream sample `first`. */
  render(first: number, count: number): Int16Array {
    const samples = new Int16Array(count);
    for (let offset = 0; offset < count; offset += 1) {
      samples[offset] = this.sample(first + offset);
    }
    return samples;
  }

  /** Where stream sample `index` is in the playlist. */
  place(index: number): Place | undefined {
    const located = this.#locate(index);
    if (located === undefined) {
      return undefined;
    }
    const [position, tune, offset] = located;
    const [row, within] = tune.rowOf(offset);
    const [passRow] = tune.inPass(row);
    const barInPass = Math.floor(passRow / ROWS_PER_BAR);
    let next = tune.tune;
    for (let ahead = 1; ahead <= this.#tunes.length; ahead += 1) {
      const candidate = this.#tunes[(position + ahead) % this.#tunes.length]?.tune;
      if (candidate !== undefined && !candidate.ident) {
        next = candidate;
        break;
      }
    }
    return {
      tune: tune.tune,
      next,
      bar: Math.floor(row / ROWS_PER_BAR),
      bars: Math.floor((tune.lead.length * tune.tune.passes) / ROWS_PER_BAR),
      chord: tune.chords[barInPass]?.name ?? "",
      lead: tune.sounding(tune.lead, passRow, within)?.[0],
      thousandths: tune.length === 0 ? 0 : Math.floor((offset * THOUSAND) / tune.length),
    };
  }
}

/** Note names of the octave, for the page. */
const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"] as const;

/** A MIDI note's name, such as `E5`. */
export function noteName(note: number): string {
  return `${NAMES[note % OCTAVE] ?? ""}${Math.floor(note / OCTAVE) - 1}`;
}

/** `level` (0 to {@link ENVELOPE_ONE}) as a share of `of`. */
export function share(level: number, of: number): number {
  return Math.max(0, scaledLevel(Math.min(Math.max(level, 0), ENVELOPE_ONE), of));
}
