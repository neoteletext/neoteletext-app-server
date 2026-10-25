//! The radio's tracker: the playlist of `tunes.rs` read once into rows,
//! and every sample of the endless stream computed from its index alone.
//! Six voices: a lead with a tempo-synced echo, a chord arpeggio, a bass,
//! and kick, snare and hi-hats made from a falling sweep and noise.

use alloc::vec::Vec;

use super::synth::{ENVELOPE_ONE, MIDI_NOTES, RATE, Shape, Wave, fall, noise_at, note_steps, samples_in, scaled, scaled_level, step_of, sweep_phase, wave_at};
use super::tunes::{PLAYLIST, Tune};

/// Rows in a bar: sixteenth notes.
pub(crate) const ROWS_PER_BAR: u32 = 16;
/// Rows in a beat.
const ROWS_PER_BEAT: u32 = 4;
/// Seconds in a minute, for the tempo.
const SECONDS_PER_MINUTE: u32 = 60;
/// Semitones in an octave.
const OCTAVE: u8 = 12;

/// The voices, in the order the page's bars show them: low to high, as a
/// spectrum is read.
pub(crate) const VOICES: usize = 6;
/// The bass.
pub(crate) const BASS: usize = 0;
/// The kick drum.
pub(crate) const KICK: usize = 1;
/// The lead.
pub(crate) const LEAD: usize = 2;
/// The arpeggio.
pub(crate) const ARPEGGIO: usize = 3;
/// The snare.
pub(crate) const SNARE: usize = 4;
/// The hi-hats.
pub(crate) const HATS: usize = 5;

// Each voice's loudest sample. Their sum, with the echo, is 30 350, under
// 16-bit full scale, so the mix never clips.
/// The lead.
const LEAD_PEAK: i32 = 5_200;
/// The echo, as a share of the lead: 3/8.
const ECHO_EIGHTHS: i32 = 3;
/// The arpeggio.
const ARPEGGIO_PEAK: i32 = 2_000;
/// The bass.
const BASS_PEAK: i32 = 6_500;
/// The kick.
const KICK_PEAK: i32 = 7_500;
/// The snare's noise.
const SNARE_NOISE_PEAK: i32 = 3_800;
/// The snare's drum tone.
const SNARE_TONE_PEAK: i32 = 1_800;
/// The hi-hats.
const HATS_PEAK: i32 = 1_600;
/// The loudest the mix can be: what the meters scale to.
pub(crate) const MIX_PEAK: i32 =
    LEAD_PEAK + LEAD_PEAK * ECHO_EIGHTHS / 8 + ARPEGGIO_PEAK + BASS_PEAK + KICK_PEAK + SNARE_NOISE_PEAK + SNARE_TONE_PEAK + HATS_PEAK;
const _: () = assert!(MIX_PEAK <= i16::MAX as i32, "the voices' peaks must not clip");

/// The echo's delay, in rows: a dotted eighth.
const ECHO_ROWS: u32 = 3;
/// The lead's envelope: a quick attack, a fall to 60 %, a short release.
const LEAD_SHAPE: Shape = Shape { attack: samples_in(4), decay: samples_in(120), sustain: ENVELOPE_ONE * 3 / 5, release: samples_in(30) };
/// The bass's envelope: plucked, then held at 70 %.
const BASS_SHAPE: Shape = Shape { attack: samples_in(3), decay: samples_in(80), sustain: ENVELOPE_ONE * 7 / 10, release: samples_in(20) };
/// The kick's length.
const KICK_SAMPLES: u32 = samples_in(160);
/// The kick's sweep, from 150 Hz to 45 Hz over its first 60 ms: the thump
/// of a drum machine's kick.
const KICK_FROM_MILLIHERTZ: u64 = 150_000;
/// Where the kick's sweep ends.
const KICK_TO_MILLIHERTZ: u64 = 45_000;
/// How long the kick sweeps.
const KICK_SWEEP_SAMPLES: u32 = samples_in(60);
/// The snare's noise.
const SNARE_SAMPLES: u32 = samples_in(140);
/// The snare's drum tone, and its pitch.
const SNARE_TONE_SAMPLES: u32 = samples_in(60);
/// The snare's drum head, 190 Hz.
const SNARE_TONE_MILLIHERTZ: u64 = 190_000;
/// A closed hi-hat.
const HAT_SAMPLES: u32 = samples_in(35);
/// An open hi-hat.
const OPEN_HAT_SAMPLES: u32 = samples_in(160);
/// Rows back a drum's sound may still ring: the longest sound over the
/// shortest row, the fastest tune's.
const DRUM_ROWS: u32 = 3;
/// The arpeggio's octave: chord tones from middle C up.
const ARPEGGIO_BASE: u8 = 60;
/// The bass's range: the root lies from G2 (MIDI 43) to F♯3 (54), low
/// enough to be bass and high enough for small speakers.
const BASS_LOWEST: u8 = 43;
/// The bass range's top.
const BASS_HIGHEST: u8 = 54;
/// The C three octaves below middle C: the bass root's base.
const BASS_C: u8 = 48;

/// A note held over rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Span {
    /// The row it starts.
    start: u32,
    /// Rows it lasts.
    rows: u32,
    /// Its MIDI note.
    note: u8,
}

/// A row of a part.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Cell {
    /// A new note.
    Note(u8),
    /// The note before, held.
    Hold,
    /// Silence.
    Rest,
}

/// A chord: its root's pitch class and its four tones above the root.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chord {
    /// Its name as written.
    pub name: &'static str,
    root: u8,
    tones: [u8; 4],
}

/// Pitch classes of the letters A to G.
const LETTERS: [(char, u8); 7] = [('c', 0), ('d', 2), ('e', 4), ('f', 5), ('g', 7), ('a', 9), ('b', 11)];

/// The pitch class of a note letter.
fn pitch_class(letter: char) -> Option<u8> {
    LETTERS.iter().find(|(name, _)| *name == letter.to_ascii_lowercase()).map(|&(_, class)| class)
}

impl Chord {
    /// A chord from its name; an unreadable name is C major.
    pub(crate) fn parse(name: &'static str) -> Self {
        let mut characters = name.chars().peekable();
        let mut root = characters.next().and_then(pitch_class).unwrap_or(0);
        match characters.peek() {
            Some('#') => {
                root = root.saturating_add(1) % OCTAVE;
                characters.next();
            }
            Some('b') => {
                root = root.saturating_add(OCTAVE - 1) % OCTAVE;
                characters.next();
            }
            _ => {}
        }
        let quality: alloc::string::String = characters.collect();
        // Root, third, fifth, and the seventh or the octave.
        let tones = match quality.as_str() {
            "m" => [0, 3, 7, 12],
            "7" => [0, 4, 7, 10],
            "m7" => [0, 3, 7, 10],
            _ => [0, 4, 7, 12],
        };
        Self { name, root, tones }
    }

    /// The `index`th tone (0 to 3) above `base`'s octave.
    fn tone(&self, base: u8, index: usize) -> u8 {
        base.saturating_add(self.root).saturating_add(self.tones.get(index).copied().unwrap_or(0))
    }

    /// The bass's root: in its range.
    fn bass_root(&self) -> u8 {
        let root = BASS_C.saturating_add(self.root);
        if root > BASS_HIGHEST { root.saturating_sub(OCTAVE) } else { root.max(BASS_LOWEST) }
    }
}

/// A melody token as a cell; anything unreadable is silence.
pub(crate) fn melody_cell(token: &str) -> Option<Cell> {
    match token {
        "-" => return Some(Cell::Hold),
        "." => return Some(Cell::Rest),
        _ => {}
    }
    let mut characters = token.chars();
    let mut class = characters.next().and_then(pitch_class)?;
    let mut rest = characters.as_str();
    if let Some(after) = rest.strip_prefix('#') {
        class = class.saturating_add(1);
        rest = after;
    }
    let octave: u8 = rest.parse().ok()?;
    // MIDI numbers C−1 as 0, so middle C, `c4`, is 60.
    let note = octave.checked_add(1)?.checked_mul(OCTAVE)?.checked_add(class)?;
    (usize::from(note) < MIDI_NOTES).then_some(Cell::Note(note))
}

/// A bass token over `chord` as a cell.
fn bass_cell(token: &str, chord: &Chord) -> Cell {
    let root = chord.bass_root();
    let tone = |index: usize| Cell::Note(root.saturating_add(chord.tones.get(index).copied().unwrap_or(0)));
    match token {
        "R" => Cell::Note(root),
        "T" => tone(1),
        "F" => tone(2),
        "S" => tone(3),
        "O" => Cell::Note(root.saturating_add(OCTAVE)),
        "-" => Cell::Hold,
        _ => Cell::Rest,
    }
}

/// Each row's span: the note sounding there, if any.
fn spans(cells: &[Cell]) -> Vec<Option<Span>> {
    let mut rows: Vec<Option<Span>> = Vec::with_capacity(cells.len());
    let mut current: Option<Span> = None;
    for (row, cell) in (0_u32..).zip(cells) {
        match *cell {
            Cell::Note(note) => current = Some(Span { start: row, rows: 1, note }),
            Cell::Hold => {
                if let Some(span) = current.as_mut() {
                    span.rows = span.rows.saturating_add(1);
                }
            }
            Cell::Rest => current = None,
        }
        rows.push(current);
    }
    // Each row of a span names the whole span.
    let mut whole = rows.clone();
    for (slot, row) in whole.iter_mut().zip(&rows) {
        if let Some(span) = row {
            *slot = rows.iter().filter_map(|other| *other).rfind(|other| other.start == span.start);
        }
    }
    whole
}

/// A drum bar as a bit set of its rows holding `hit`.
fn pattern(text: &str, hit: char) -> u16 {
    text.chars().take(16).enumerate().filter(|&(_, character)| character == hit).fold(0, |bits, (row, _)| bits | (1 << row))
}

/// A tune read into rows.
struct Arranged {
    tune: &'static Tune,
    /// Samples in a row, at the tune's tempo.
    row_samples: u32,
    /// Rows in one pass of the melody.
    pass_rows: u32,
    lead: Vec<Option<Span>>,
    bass: Vec<Option<Span>>,
    chords: Vec<Chord>,
    kick: u16,
    snare: u16,
    fill: u16,
    hats: u16,
    open_hats: u16,
    /// Where it starts in the playlist's cycle, in samples.
    start: u64,
    /// Its length in samples.
    length: u64,
}

impl Arranged {
    fn new(tune: &'static Tune, start: u64) -> Self {
        let row_samples = RATE.saturating_mul(SECONDS_PER_MINUTE).checked_div(tune.tempo.saturating_mul(ROWS_PER_BEAT)).unwrap_or(1).max(1);
        let chords: Vec<Chord> = tune.chords.iter().map(|name| Chord::parse(name)).collect();
        let lead: Vec<Cell> = tune.melody.iter().flat_map(|bar| bar.split_whitespace().map(|token| melody_cell(token).unwrap_or(Cell::Rest))).collect();
        let bass: Vec<Cell> = chords.iter().flat_map(|chord| tune.bass.split_whitespace().map(|token| bass_cell(token, chord))).collect();
        let pass_rows = u32::try_from(tune.melody.len()).unwrap_or(0).saturating_mul(ROWS_PER_BAR);
        let fill = if tune.fill.is_empty() { pattern(tune.snare, 'x') } else { pattern(tune.fill, 'x') };
        let length = u64::from(row_samples).saturating_mul(u64::from(pass_rows)).saturating_mul(u64::from(tune.passes));
        Self {
            tune,
            row_samples,
            pass_rows,
            lead: spans(&lead),
            bass: spans(&bass),
            chords,
            kick: pattern(tune.kick, 'x'),
            snare: pattern(tune.snare, 'x'),
            fill,
            hats: pattern(tune.hats, 'x'),
            open_hats: pattern(tune.hats, 'o'),
            start,
            length,
        }
    }

    /// The tune row of sample `offset` into the tune, and the sample
    /// within the row.
    fn row_of(&self, offset: u64) -> (u32, u32) {
        let samples = u64::from(self.row_samples);
        let row = offset.checked_div(samples).and_then(|row| u32::try_from(row).ok()).unwrap_or(u32::MAX);
        let within = offset.checked_rem(samples).and_then(|within| u32::try_from(within).ok()).unwrap_or(0);
        (row, within)
    }

    /// The row of the pass, and the pass, of tune row `row`.
    fn in_pass(&self, row: u32) -> (u32, u32) {
        (row.checked_rem(self.pass_rows).unwrap_or(0), row.checked_div(self.pass_rows).unwrap_or(0))
    }

    /// The span sounding at pass row `row`, `within` samples into the row,
    /// as (note, offset into the note, the note's length).
    fn sounding(&self, part: &[Option<Span>], row: u32, within: u32) -> Option<(u8, u32, u32)> {
        let span = part.get(usize::try_from(row).ok()?).copied().flatten()?;
        let offset = row.saturating_sub(span.start).saturating_mul(self.row_samples).saturating_add(within);
        Some((span.note, offset, span.rows.saturating_mul(self.row_samples)))
    }

    /// Whether the drum of `bits` hits at tune row `row`; the snare plays
    /// its fill in each pass's last bar.
    fn hits(&self, row: u32, drum: usize) -> bool {
        let (pass_row, _) = self.in_pass(row);
        let step = pass_row % ROWS_PER_BAR;
        let last_bar = pass_row / ROWS_PER_BAR == (self.pass_rows / ROWS_PER_BAR).saturating_sub(1);
        let bits = match drum {
            KICK => self.kick,
            SNARE if last_bar => self.fill,
            SNARE => self.snare,
            HATS => self.hats | self.open_hats,
            _ => 0,
        };
        bits & (1 << step) != 0
    }

    /// The latest hit of `drum` at or before tune row `row`, within the
    /// rows a drum rings, as (its row, samples since).
    fn latest_hit(&self, row: u32, within: u32, drum: usize) -> Option<(u32, u32)> {
        (0..=DRUM_ROWS.min(row)).find_map(|back| {
            let hit = row.saturating_sub(back);
            self.hits(hit, drum).then(|| (hit, back.saturating_mul(self.row_samples).saturating_add(within)))
        })
    }

    fn open_hat(&self, row: u32) -> bool {
        let (pass_row, _) = self.in_pass(row);
        self.open_hats & (1 << (pass_row % ROWS_PER_BAR)) != 0
    }
}

/// One voice at one sample: its value, and its envelope's level for the
/// page's meters.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Voice {
    /// The sample.
    pub value: i32,
    /// The envelope, 0 to [`ENVELOPE_ONE`].
    pub level: i32,
}

/// Where the stream is in the playlist.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Place {
    /// The tune playing.
    pub tune: &'static Tune,
    /// The next tune that is not the ident.
    pub next: &'static Tune,
    /// The bar playing, from 0, over every pass.
    pub bar: u32,
    /// Bars in the whole tune.
    pub bars: u32,
    /// The chord playing.
    pub chord: &'static str,
    /// The lead's note, if it is sounding.
    pub lead: Option<u8>,
    /// How far through the tune, in thousandths.
    pub thousandths: u32,
}

/// Thousandths in one.
const THOUSAND: u64 = 1_000;

/// The playlist, read once, and the notes' phase steps.
pub(crate) struct Tracker {
    tunes: Vec<Arranged>,
    /// The playlist's length in samples, after which it loops.
    cycle: u64,
    steps: [u32; MIDI_NOTES],
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    /// The playlist, read into rows.
    pub(crate) fn new() -> Self {
        let mut tunes = Vec::with_capacity(PLAYLIST.len());
        let mut start = 0_u64;
        for tune in PLAYLIST {
            let arranged = Arranged::new(tune, start);
            start = start.saturating_add(arranged.length);
            tunes.push(arranged);
        }
        Self { tunes, cycle: start, steps: note_steps() }
    }

    /// The playlist's length, in samples.
    #[cfg(test)]
    pub(crate) const fn cycle(&self) -> u64 {
        self.cycle
    }

    /// The tune at stream sample `index`, and the sample within it.
    fn locate(&self, index: u64) -> Option<(usize, &Arranged, u64)> {
        let offset = index.checked_rem(self.cycle)?;
        self.tunes
            .iter()
            .enumerate()
            .find(|(_, tune)| offset >= tune.start && offset < tune.start.saturating_add(tune.length))
            .map(|(position, tune)| (position, tune, offset.saturating_sub(tune.start)))
    }

    fn step(&self, note: u8) -> u32 {
        self.steps.get(usize::from(note)).copied().unwrap_or(0)
    }

    /// The lead `offset` samples into tune `tune`.
    fn lead(&self, tune: &Arranged, offset: u64) -> Voice {
        let (row, within) = tune.row_of(offset);
        let (pass_row, _) = tune.in_pass(row);
        let Some((note, at, length)) = tune.sounding(&tune.lead, pass_row, within) else { return Voice::default() };
        let level = LEAD_SHAPE.level(at, length);
        Voice { value: scaled(wave_at(tune.tune.lead, self.step(note).wrapping_mul(at)), LEAD_PEAK, level), level }
    }

    /// Every voice at stream sample `index`.
    pub(crate) fn voices(&self, index: u64) -> [Voice; VOICES] {
        let mut voices = [Voice::default(); VOICES];
        let Some((_, tune, offset)) = self.locate(index) else { return voices };
        let (row, within) = tune.row_of(offset);
        let (pass_row, pass) = tune.in_pass(row);
        let bar = usize::try_from(pass_row / ROWS_PER_BAR).unwrap_or(0);

        let mut lead = self.lead(tune, offset);
        let echo_samples = u64::from(tune.row_samples.saturating_mul(ECHO_ROWS));
        if let Some(earlier) = offset.checked_sub(echo_samples) {
            let echo = self.lead(tune, earlier);
            lead.value = lead.value.saturating_add(echo.value.saturating_mul(ECHO_EIGHTHS) / 8);
        }
        if let Some(slot) = voices.get_mut(LEAD) {
            *slot = lead;
        }

        if let Some((note, at, length)) = tune.sounding(&tune.bass, pass_row, within)
            && let Some(slot) = voices.get_mut(BASS)
        {
            let level = BASS_SHAPE.level(at, length);
            *slot = Voice { value: scaled(wave_at(Wave::Triangle, self.step(note).wrapping_mul(at)), BASS_PEAK, level), level };
        }

        if pass >= tune.tune.arpeggio_from
            && let Some(chord) = tune.chords.get(bar)
            && let Some(slot) = voices.get_mut(ARPEGGIO)
        {
            // Up the chord's four tones, one a row.
            let chord_tone = usize::try_from(pass_row % 4).unwrap_or(0);
            let note = chord.tone(ARPEGGIO_BASE, chord_tone);
            let level = fall(within, tune.row_samples);
            *slot = Voice { value: scaled(wave_at(Wave::Square, self.step(note).wrapping_mul(within)), ARPEGGIO_PEAK, level), level };
        }

        if let Some((_, since)) = tune.latest_hit(row, within, KICK)
            && let Some(slot) = voices.get_mut(KICK)
        {
            let phase = sweep_phase(step_of(KICK_FROM_MILLIHERTZ), step_of(KICK_TO_MILLIHERTZ), since, KICK_SWEEP_SAMPLES);
            let level = fall(since, KICK_SAMPLES);
            *slot = Voice { value: scaled(wave_at(Wave::Sine, phase), KICK_PEAK, level), level };
        }

        if let Some((_, since)) = tune.latest_hit(row, within, SNARE)
            && let Some(slot) = voices.get_mut(SNARE)
        {
            let level = fall(since, SNARE_SAMPLES);
            let tone_phase = u32::try_from(step_of(SNARE_TONE_MILLIHERTZ).wrapping_mul(u64::from(since)) & u64::from(u32::MAX)).unwrap_or(0);
            let head = scaled(wave_at(Wave::Sine, tone_phase), SNARE_TONE_PEAK, fall(since, SNARE_TONE_SAMPLES));
            *slot = Voice { value: scaled(noise_at(index), SNARE_NOISE_PEAK, level).saturating_add(head), level };
        }

        if let Some((hit, since)) = tune.latest_hit(row, within, HATS)
            && let Some(slot) = voices.get_mut(HATS)
        {
            let length = if tune.open_hat(hit) { OPEN_HAT_SAMPLES } else { HAT_SAMPLES };
            let level = fall(since, length);
            // Noise less the sample before: a first difference, which
            // keeps the hiss and drops the rumble, as a hi-hat sounds.
            let bright = noise_at(index).saturating_sub(noise_at(index.wrapping_sub(1))) / 2;
            *slot = Voice { value: scaled(bright, HATS_PEAK, level), level };
        }
        voices
    }

    /// Stream sample `index`.
    pub(crate) fn sample(&self, index: u64) -> i16 {
        let sum = self.voices(index).iter().fold(0_i32, |sum, voice| sum.saturating_add(voice.value));
        i16::try_from(sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX))).unwrap_or(0)
    }

    /// `count` samples from stream sample `first`.
    pub(crate) fn render(&self, first: u64, count: usize) -> Vec<i16> {
        (0..count).map(|offset| self.sample(first.saturating_add(u64::try_from(offset).unwrap_or(0)))).collect()
    }

    /// Where stream sample `index` is in the playlist.
    pub(crate) fn place(&self, index: u64) -> Option<Place> {
        let (position, tune, offset) = self.locate(index)?;
        let (row, within) = tune.row_of(offset);
        let (pass_row, _) = tune.in_pass(row);
        let bar_in_pass = usize::try_from(pass_row / ROWS_PER_BAR).unwrap_or(0);
        let next = (1..=self.tunes.len())
            .filter_map(|ahead| position.saturating_add(ahead).checked_rem(self.tunes.len()).and_then(|next| self.tunes.get(next)))
            .map(|arranged| arranged.tune)
            .find(|candidate| !candidate.ident)
            .unwrap_or(tune.tune);
        let bars = u32::try_from(tune.lead.len()).unwrap_or(0).saturating_mul(tune.tune.passes) / ROWS_PER_BAR;
        Some(Place {
            tune: tune.tune,
            next,
            bar: row / ROWS_PER_BAR,
            bars,
            chord: tune.chords.get(bar_in_pass).map_or("", |chord| chord.name),
            lead: tune.sounding(&tune.lead, pass_row, within).map(|(note, _, _)| note),
            thousandths: u32::try_from(offset.saturating_mul(THOUSAND).checked_div(tune.length).unwrap_or(0)).unwrap_or(0),
        })
    }
}

/// Note names of the octave, for the page.
const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// A MIDI note's name, such as `E5`.
pub(crate) fn note_name(note: u8) -> alloc::string::String {
    let name = NAMES.get(usize::from(note % OCTAVE)).copied().unwrap_or("");
    let octave = i32::from(note / OCTAVE).saturating_sub(1);
    alloc::format!("{name}{octave}")
}

/// `level` (0 to [`ENVELOPE_ONE`]) as a share of `of`.
pub(crate) fn share(level: i32, of: u32) -> u32 {
    u32::try_from(scaled_level(level.clamp(0, ENVELOPE_ONE), i32::try_from(of).unwrap_or(0))).unwrap_or(0)
}
