//! Page 504, the drum machine: a step sequencer of sixteen steps over six
//! of the library's sounds, each viewer's own pattern, played in a loop to
//! that viewer alone. Every viewer's steps fall on one clock, so the
//! channel needs one wake for all of them.

use alloc::format;

use teletext_runtime::screen::{Colour, Screen};

use super::draw::badge;
use crate::sound::Effect;

/// Steps in the loop: a bar of sixteenth notes.
pub(crate) const STEPS: usize = 16;
/// The tracks: their labels, sounds and colours, top to bottom.
pub(crate) const TRACKS: [(&str, Effect, Colour); 6] = [
    ("Bell", Effect::Bell, Colour::Magenta),
    ("C'", Effect::Note(12), Colour::Cyan),
    ("G", Effect::Note(7), Colour::Cyan),
    ("E", Effect::Note(4), Colour::Cyan),
    ("Hit", Effect::Hit, Colour::Yellow),
    ("Tick", Effect::Click, Colour::White),
];
/// The tempo: 120 beats a minute of four steps, so a step is 125 ms.
pub(crate) const STEP_MS: u64 = 60_000 / 120 / 4;
/// A pattern to start from, a step a bit, so the machine plays something
/// at once: a rising arpeggio over a backbeat and running ticks.
const START: [u16; TRACKS.len()] = [0x0001, 0x4100, 0x0840, 0x8008, 0x1010, 0x5555];

/// One viewer's machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Machine {
    /// Each track's steps, a bit a step.
    pub steps: [u16; TRACKS.len()],
    /// The cursor: track and step.
    pub cursor: (usize, usize),
    /// Whether it plays.
    pub playing: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self { steps: START, cursor: (0, 0), playing: false }
    }
}

impl Machine {
    /// Whether `track` sounds at `step`.
    pub(crate) fn on(&self, track: usize, step: usize) -> bool {
        self.steps.get(track).is_some_and(|bits| step < STEPS && bits & (1 << step) != 0)
    }

    /// Turns the step under the cursor on or off.
    pub(crate) fn toggle(&mut self) {
        let (track, step) = self.cursor;
        if let Some(bits) = self.steps.get_mut(track)
            && step < STEPS
        {
            *bits ^= 1 << step;
        }
    }

    /// Moves the cursor, staying on the grid.
    pub(crate) fn go(&mut self, down: isize, across: isize) {
        let (track, step) = self.cursor;
        self.cursor = (track.saturating_add_signed(down).min(TRACKS.len().saturating_sub(1)), step.saturating_add_signed(across).min(STEPS.saturating_sub(1)));
    }

    /// Clears every step.
    pub(crate) fn clear(&mut self) {
        self.steps = [0; TRACKS.len()];
    }
}

/// The step the clock is at, at `now`.
pub(crate) fn step_at(now: u64) -> usize {
    let steps = u64::try_from(STEPS).unwrap_or(1);
    usize::try_from((now / STEP_MS).checked_rem(steps).unwrap_or(0)).unwrap_or(0)
}

/// The grid's top row.
const GRID_ROW: usize = 5;
/// The first step's colour cell.
const GRID_COLUMN: usize = 6;
/// Columns a step takes: its colour and its mosaic.
const STEP_CELLS: usize = 2;
/// A step that is on: every sixel (EN 300 706 Table 47).
const ON: u8 = 0x7F;
/// A step that is off: the middle sixel row, a dash.
const OFF: u8 = 0x2C;
/// The steps a beat.
const BEAT: usize = 4;

/// The cell of `step`'s mosaic.
const fn step_column(step: usize) -> usize {
    GRID_COLUMN.saturating_add(step.saturating_mul(STEP_CELLS)).saturating_add(1)
}

/// Draws the machine; `playhead` is the step playing, when it plays.
pub(crate) fn draw(screen: &mut Screen, machine: &Machine, playhead: Option<usize>) {
    screen.coloured(3, 1, Colour::Cyan, "Sixteen steps, six sounds, your own");
    for beat in 0..STEPS / BEAT {
        screen.coloured(
            GRID_ROW.saturating_sub(1),
            step_column(beat.saturating_mul(BEAT)).saturating_sub(1),
            Colour::Yellow,
            &format!("{}", beat.saturating_add(1)),
        );
    }
    for (track, &(label, _, colour)) in TRACKS.iter().enumerate() {
        let row = GRID_ROW.saturating_add(track);
        screen.coloured(row, 0, colour, label);
        for step in 0..STEPS {
            let colour = if machine.cursor == (track, step) {
                Colour::Yellow
            } else if playhead == Some(step) {
                Colour::White
            } else if machine.on(track, step) {
                Colour::Green
            } else if step % BEAT == 0 {
                Colour::Cyan
            } else {
                Colour::Blue
            };
            let mosaic = if machine.on(track, step) { ON } else { OFF };
            screen.codes(row, step_column(step).saturating_sub(1), &[colour.mosaic(), mosaic]);
        }
    }
    let marker = GRID_ROW.saturating_add(TRACKS.len());
    if let Some(step) = playhead {
        screen.coloured(marker, step_column(step).saturating_sub(1), Colour::White, "^");
    }
    if machine.playing {
        badge(screen, 13, 0, Colour::Green, Colour::Blue, " PLAYING ");
        screen.coloured(13, 13, Colour::White, "120 bpm, in a loop");
    } else {
        badge(screen, 13, 0, Colour::Blue, Colour::White, " STOPPED ");
        screen.coloured(13, 13, Colour::White, "Enter to play");
    }
    screen.coloured(15, 1, Colour::Green, "Arrows");
    screen.coloured(15, 12, Colour::White, "move the cursor");
    screen.coloured(16, 1, Colour::Green, "Space");
    screen.coloured(16, 12, Colour::White, "turn a step on or off");
    screen.coloured(17, 1, Colour::Green, "Enter");
    screen.coloured(17, 12, Colour::White, "play or stop");
    screen.coloured(18, 1, Colour::Green, "C");
    screen.coloured(18, 12, Colour::White, "clear the pattern");
    screen.coloured(20, 1, Colour::Cyan, "A click on a step turns it on or off.");
}

/// The track and step at a cell, for the mouse.
pub(crate) fn step_at_cell(column: usize, row: usize) -> Option<(usize, usize)> {
    let track = row.checked_sub(GRID_ROW).filter(|&track| track < TRACKS.len())?;
    let step = column.checked_sub(GRID_COLUMN)? / STEP_CELLS;
    (step < STEPS).then_some((track, step))
}
