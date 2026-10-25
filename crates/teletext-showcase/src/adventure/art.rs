//! Mosaic pictures in colour, drawn as text: each character of a row is
//! one sixel, a colour's letter or `.` for none, and each row of cells
//! may have a background colour (EN 300 706 clause 15.7.1: a cell is two
//! sixels across and three down).
//!
//! A colour changes with a spacing attribute, which takes the cell
//! before the change. That cell shows a space, or in Hold Mosaics
//! (clause 12.2, Table 26) the mosaic before it; each row takes whichever
//! shows it better. A picture reads best when its colours meet across a
//! blank cell, or across a run of the same mosaic.

use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, SIXELS_ACROSS, SIXELS_DOWN, SPACE, Screen, attribute};

/// The character of a sixel that is off.
#[cfg(test)]
const OFF: char = '.';
/// The colours of Level 1 (Table 26), counted in a cell.
const COLOURS: usize = 8;
/// The base of a mosaic character: bit 5 always set (Table 47).
const MOSAIC_BASE: u8 = 0x20;
/// Each sixel's bit in a mosaic character, top-left first, row by row
/// (Table 47: bits 0 to 4, then bit 6 for the bottom right).
const SIXEL_BITS: [[u8; SIXELS_ACROSS]; SIXELS_DOWN] = [[0x01, 0x02], [0x04, 0x08], [0x10, 0x40]];

/// A picture: its sixel rows and its cell rows' backgrounds.
#[derive(Clone, Copy, Debug)]
pub struct Art {
    /// Rows of sixels, each character one sixel.
    pub sixels: &'static [&'static str],
    /// One character a cell row: a colour's letter for its background,
    /// or `.` for black.
    pub backgrounds: &'static str,
}

/// The colour a letter names.
#[must_use]
pub const fn colour_of(letter: char) -> Option<Colour> {
    match letter {
        'r' => Some(Colour::Red),
        'g' => Some(Colour::Green),
        'y' => Some(Colour::Yellow),
        'b' => Some(Colour::Blue),
        'm' => Some(Colour::Magenta),
        'c' => Some(Colour::Cyan),
        'w' => Some(Colour::White),
        _ => None,
    }
}

/// Whether a picture's character is one the format knows.
#[cfg(test)]
#[must_use]
pub const fn known(letter: char) -> bool {
    letter == OFF || colour_of(letter).is_some()
}

impl Art {
    /// Its size in sixels: the widest row, and the rows.
    #[must_use]
    pub fn size(&self) -> (usize, usize) {
        (self.sixels.iter().map(|row| row.chars().count()).max().unwrap_or(0), self.sixels.len())
    }

    /// Its size in cells.
    #[must_use]
    pub fn cells(&self) -> (usize, usize) {
        let (across, down) = self.size();
        (across.div_ceil(SIXELS_ACROSS), down.div_ceil(SIXELS_DOWN))
    }

    /// The colour of the sixel at `x`, `y`, if it is on.
    fn sixel(&self, x: usize, y: usize) -> Option<Colour> {
        self.sixels.get(y).and_then(|row| row.chars().nth(x)).and_then(colour_of)
    }

    /// The cell whose top-left sixel is `x`, `y`: its mosaic character and
    /// the colour most of its sixels have, or none when it is empty.
    fn cell(&self, x: usize, y: usize) -> (u8, Option<Colour>) {
        let mut code = MOSAIC_BASE;
        let mut counts = [0_u8; COLOURS];
        for (down, bits) in SIXEL_BITS.iter().enumerate() {
            for (across, &bit) in bits.iter().enumerate() {
                if let Some(colour) = self.sixel(x.saturating_add(across), y.saturating_add(down)) {
                    code |= bit;
                    if let Some(count) = counts.get_mut(usize::from(colour.index())) {
                        *count = count.saturating_add(1);
                    }
                }
            }
        }
        let mut best: Option<(Colour, u8)> = None;
        for colour in [Colour::White, Colour::Yellow, Colour::Cyan, Colour::Green, Colour::Magenta, Colour::Red, Colour::Blue] {
            let count = counts.get(usize::from(colour.index())).copied().unwrap_or(0);
            if count > 0 && best.is_none_or(|(_, most)| count > most) {
                best = Some((colour, count));
            }
        }
        (code, best.map(|(colour, _)| colour))
    }

    /// Draws the picture on `screen` with its top-left cell at `row`,
    /// `column`. Each row's attributes go in the cells before `column`:
    /// the first colour, Hold Mosaics where it serves the row better, and
    /// a background at the row's start, so a background is a band across
    /// the page. `column` must leave them room: four cells with a
    /// background, two without.
    pub fn draw(&self, screen: &mut Screen, row: usize, column: usize) {
        let (across, down) = self.cells();
        let mut backgrounds = self.backgrounds.chars();
        for cell_row in 0..down {
            let screen_row = row.saturating_add(cell_row);
            if let Some(background) = backgrounds.next().and_then(colour_of) {
                screen.codes(screen_row, 0, &[background.text(), attribute::NEW_BACKGROUND]);
            }
            let y = cell_row.saturating_mul(SIXELS_DOWN);
            let cells: Vec<(u8, Option<Colour>)> = (0..across).map(|cell| self.cell(cell.saturating_mul(SIXELS_ACROSS), y)).collect();
            let wanted: Vec<u8> = cells.iter().map(|&(code, _)| code).collect();
            let codes = row_codes(&cells);
            if mismatches(&codes, &wanted, true) < mismatches(&codes, &wanted, false) {
                screen.code(screen_row, column.saturating_sub(2), attribute::HOLD_MOSAICS);
            }
            screen.codes(screen_row, column.saturating_sub(1), &codes);
        }
    }
}

/// A row's codes from the cell before the picture: the first colour,
/// then each cell, a change of colour taking the cell before it.
fn row_codes(cells: &[(u8, Option<Colour>)]) -> Vec<u8> {
    let first = cells.iter().find_map(|&(_, colour)| colour).unwrap_or(Colour::White);
    let mut codes = alloc::vec![first.mosaic()];
    let mut current = first;
    for &(code, colour) in cells {
        if let Some(colour) = colour
            && colour != current
            && let Some(before) = codes.last_mut()
        {
            *before = colour.mosaic();
            current = colour;
        }
        codes.push(code);
    }
    codes
}

/// How many cells of a row show other than `wanted` (which starts a cell
/// after `codes`), with Hold Mosaics or without: an attribute's cell
/// shows the last mosaic held, or a space (EN 300 706 clause 12.2).
fn mismatches(codes: &[u8], wanted: &[u8], hold: bool) -> usize {
    let mut last_mosaic = SPACE;
    let mut count = 0_usize;
    for (&code, &want) in codes.iter().skip(1).zip(wanted) {
        let shown = if code < SPACE {
            if hold { last_mosaic } else { SPACE }
        } else {
            last_mosaic = code;
            code
        };
        if shown != want {
            count = count.saturating_add(1);
        }
    }
    count
}
