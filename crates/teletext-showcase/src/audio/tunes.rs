//! The radio's tunes, written for the showcase, as a tracker writes them:
//! one token a row, sixteen rows a bar.
//!
//! - A melody token is a note (`c5`, `f#4`, `a#5`: the letter, a sharp,
//!   the octave, middle C being `c4`), `-` to hold the note before, or `.`
//!   for silence.
//! - A bass token names a tone of the bar's chord: `R` its root, `T` its
//!   third, `F` its fifth, `S` its seventh (or octave), `O` the root an
//!   octave up; `-` and `.` as in a melody.
//! - A drum row is `x` for a hit, `o` for an open hi-hat, `.` for none.
//! - A chord is its root (`C`, `F#`, `Bb`) and `m`, `7` or `m7`.

use super::synth::Wave;

/// A tune as written.
#[derive(Debug)]
pub(crate) struct Tune {
    /// Its title, for the page.
    pub title: &'static str,
    /// What it is, in a few words.
    pub style: &'static str,
    /// Beats a minute; four rows a beat.
    pub tempo: u32,
    /// The lead's waveform.
    pub lead: Wave,
    /// How many times the melody plays.
    pub passes: u32,
    /// The first pass with the arpeggio; at or past `passes`, none.
    pub arpeggio_from: u32,
    /// One chord a bar.
    pub chords: &'static [&'static str],
    /// One string a bar, sixteen tokens each.
    pub melody: &'static [&'static str],
    /// The bass's bar, played over each bar's chord.
    pub bass: &'static str,
    /// The kick drum's bar.
    pub kick: &'static str,
    /// The snare's bar.
    pub snare: &'static str,
    /// The snare's bar at the end of each pass; empty for the usual one.
    pub fill: &'static str,
    /// The hi-hats' bar.
    pub hats: &'static str,
    /// Whether it is the station's ident, which the page never names as
    /// the next tune.
    pub ident: bool,
}

/// The playlist, in order, looping: each tune with the station's ident
/// after it.
pub(crate) const PLAYLIST: [&Tune; 8] = [&SUNRISE, &IDENT, &PAGE_888, &IDENT, &GROOVE, &IDENT, &FASTEXT, &IDENT];

/// A bright tune in C major.
const SUNRISE: Tune = Tune {
    title: "Ceefax Sunrise",
    style: "chip pop in C major",
    tempo: 124,
    lead: Wave::Pulse,
    passes: 2,
    arpeggio_from: 1,
    chords: &["C", "G", "Am", "F", "C", "G", "F", "C"],
    melody: &[
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

/// A slow tune in A minor: page 888 is where the subtitles were.
const PAGE_888: Tune = Tune {
    title: "Page 888",
    style: "slow minor, late at night",
    tempo: 96,
    lead: Wave::Square,
    passes: 2,
    arpeggio_from: 0,
    chords: &["Am", "F", "C", "G", "Am", "F", "E", "E7"],
    melody: &[
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

/// A syncopated tune in D Dorian.
const GROOVE: Tune = Tune {
    title: "Mosaic Groove",
    style: "funk in D Dorian",
    tempo: 104,
    lead: Wave::Saw,
    passes: 2,
    arpeggio_from: 1,
    chords: &["Dm7", "G7", "Dm7", "G7", "Bb", "C7", "Dm7", "A7"],
    melody: &[
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

/// A fast tune in E minor.
const FASTEXT: Tune = Tune {
    title: "Fastext Run",
    style: "chase music in E minor",
    tempo: 150,
    lead: Wave::Pulse,
    passes: 3,
    arpeggio_from: 1,
    chords: &["Em", "C", "D", "B7", "Em", "C", "Am", "B7"],
    melody: &[
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

/// The station's ident between tunes: three bell notes over a chord.
const IDENT: Tune = Tune {
    title: "NeoTeletext Radio ident",
    style: "the station's call sign",
    tempo: 100,
    lead: Wave::Sine,
    passes: 1,
    arpeggio_from: 0,
    chords: &["C"],
    melody: &["g5 - - - e5 - - - c6 - - - - - - -"],
    bass: "R - - - - - - - - - - - - - - -",
    kick: "................",
    snare: "................",
    fill: "",
    hats: "................",
    ident: true,
};
