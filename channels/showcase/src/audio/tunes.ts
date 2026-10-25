// The radio's tunes, written for the showcase, as a tracker writes them:
// one token a row, sixteen rows a bar
// (crates/teletext-showcase/src/audio/tunes.rs).
//
// - A melody token is a note (`c5`, `f#4`, `a#5`: the letter, a sharp,
//   the octave, middle C being `c4`), `-` to hold the note before, or `.`
//   for silence.
// - A bass token names a tone of the bar's chord: `R` its root, `T` its
//   third, `F` its fifth, `S` its seventh (or octave), `O` the root an
//   octave up; `-` and `.` as in a melody.
// - A drum row is `x` for a hit, `o` for an open hi-hat, `.` for none.
// - A chord is its root (`C`, `F#`, `Bb`) and `m`, `7` or `m7`.

import { Wave } from "./synth.ts";

/** A tune as written. */
export interface Tune {
  /** Its title, for the page. */
  readonly title: string;
  /** What it is, in a few words. */
  readonly style: string;
  /** Beats a minute; four rows a beat. */
  readonly tempo: number;
  /** The lead's waveform. */
  readonly lead: Wave;
  /** How many times the melody plays. */
  readonly passes: number;
  /** The first pass with the arpeggio; at or past `passes`, none. */
  readonly arpeggioFrom: number;
  /** One chord a bar. */
  readonly chords: readonly string[];
  /** One string a bar, sixteen tokens each. */
  readonly melody: readonly string[];
  /** The bass's bar, played over each bar's chord. */
  readonly bass: string;
  /** The kick drum's bar. */
  readonly kick: string;
  /** The snare's bar. */
  readonly snare: string;
  /** The snare's bar at the end of each pass; empty for the usual one. */
  readonly fill: string;
  /** The hi-hats' bar. */
  readonly hats: string;
  /** Whether it is the station's ident, which the page never names as the next tune. */
  readonly ident: boolean;
}

/** A bright tune in C major. */
const SUNRISE: Tune = {
  title: "Ceefax Sunrise",
  style: "chip pop in C major",
  tempo: 124,
  lead: Wave.Pulse,
  passes: 2,
  arpeggioFrom: 1,
  chords: ["C", "G", "Am", "F", "C", "G", "F", "C"],
  melody: [
    "c5 - e5 - g5 - - - e5 - g5 - c6 - - -",
    "b5 - - - a5 - g5 - d5 - - - g5 - - -",
    "a5 - - - c6 - b5 - a5 - e5 - - - . .",
    "f5 - a5 - c6 - a5 - g5 - f5 - e5 - d5 -",
    "e5 - - - g5 - e5 - c5 - e5 - g5 - - -",
    "d5 - - - g5 - b5 - d6 - - - b5 - - -",
    "c6 - a5 - f5 - a5 - g5 - f5 - e5 - d5 -",
    "c5 - - - - - - - . . . . g4 - b4 -",
  ],
  bass: "R - . R F - . R O - . R F - R -",
  kick: "x...x...x...x...",
  snare: "....x.......x...",
  fill: "....x.......x.xx",
  hats: "x.x.x.x.x.x.x.x.",
  ident: false,
};

/** A slow tune in A minor: page 888 is where the subtitles were. */
const PAGE_888: Tune = {
  title: "Page 888",
  style: "slow minor, late at night",
  tempo: 96,
  lead: Wave.Square,
  passes: 2,
  arpeggioFrom: 0,
  chords: ["Am", "F", "C", "G", "Am", "F", "E", "E7"],
  melody: [
    "a4 - - - - - c5 - e5 - - - d5 - c5 -",
    "c5 - - - a4 - - - - - - - . . . .",
    "g4 - - - c5 - e5 - g5 - - - f5 - e5 -",
    "d5 - - - - - - - b4 - - - g4 - - -",
    "a4 - - - - - c5 - e5 - - - a5 - g5 -",
    "f5 - - - e5 - c5 - a4 - - - c5 - - -",
    "b4 - - - g#4 - - - b4 - - - e5 - d5 -",
    "e5 - - - - - - - - - - - . . . .",
  ],
  bass: "R - - - - - R - F - - - R - - -",
  kick: "x.........x.....",
  snare: "........x.......",
  fill: "........x.....x.",
  hats: "x.x.x.x.x.x.x.o.",
  ident: false,
};

/** A syncopated tune in D Dorian. */
const GROOVE: Tune = {
  title: "Mosaic Groove",
  style: "funk in D Dorian",
  tempo: 104,
  lead: Wave.Saw,
  passes: 2,
  arpeggioFrom: 1,
  chords: ["Dm7", "G7", "Dm7", "G7", "Bb", "C7", "Dm7", "A7"],
  melody: [
    "d5 . f5 . a5 - . c6 - . a5 . g5 - f5 .",
    ". . b4 . d5 - . f5 - - - . e5 . d5 .",
    "a4 . . d5 . . f5 . e5 - d5 - c5 - a4 -",
    "b4 - - - - - . . g4 . a4 . b4 . d5 .",
    "d5 - f5 - a#5 - - - a5 - f5 - d5 - - -",
    "e5 - g5 - c6 - - - a#5 - g5 - e5 - - -",
    "f5 . e5 . d5 . c5 . d5 - - - a4 . c5 .",
    "c#5 - - - e5 - - - g5 - - - a5 - . .",
  ],
  bass: "R . O . R R . O . R . O R . F .",
  kick: "x.....x...x.....",
  snare: "....x.......x..x",
  fill: "....x.......xxxx",
  hats: "x.x.x.x.x.x.x.o.",
  ident: false,
};

/** A fast tune in E minor. */
const FASTEXT: Tune = {
  title: "Fastext Run",
  style: "chase music in E minor",
  tempo: 150,
  lead: Wave.Pulse,
  passes: 3,
  arpeggioFrom: 1,
  chords: ["Em", "C", "D", "B7", "Em", "C", "Am", "B7"],
  melody: [
    "e5 g5 b5 g5 e5 g5 b5 e6 d6 - b5 - g5 - e5 -",
    "e5 g5 c6 g5 e5 g5 c6 e6 d6 - c6 - g5 - e5 -",
    "f#5 a5 d6 a5 f#5 a5 d6 f#6 e6 - d6 - a5 - f#5 -",
    "b4 d#5 f#5 a5 b5 - a5 - f#5 - d#5 - b4 - - -",
    "g5 - - - f#5 - e5 - b5 - - - a5 - g5 -",
    "e5 - - - g5 - c6 - b5 - - - g5 - e5 -",
    "a5 - - - c6 - e6 - d6 - c6 - b5 - a5 -",
    "b5 - - - - - - - f#5 - d#5 - b4 - - -",
  ],
  bass: "R - R - R - R - O - R - F - R -",
  kick: "x...x...x...x.x.",
  snare: "....x.......x...",
  fill: "....x...x.x.xxxx",
  hats: "x.x.x.x.x.x.x.x.",
  ident: false,
};

/** The station's ident between tunes: three bell notes over a chord. */
const IDENT: Tune = {
  title: "NeoTeletext Radio ident",
  style: "the station's call sign",
  tempo: 100,
  lead: Wave.Sine,
  passes: 1,
  arpeggioFrom: 0,
  chords: ["C"],
  melody: ["g5 - - - e5 - - - c6 - - - - - - -"],
  bass: "R - - - - - - - - - - - - - - -",
  kick: "................",
  snare: "................",
  fill: "",
  hats: "................",
  ident: true,
};

/** The playlist, in order, looping: each tune with the station's ident after it. */
export const PLAYLIST: readonly Tune[] = [SUNRISE, IDENT, PAGE_888, IDENT, GROOVE, IDENT, FASTEXT, IDENT];
