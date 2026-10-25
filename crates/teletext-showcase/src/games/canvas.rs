//! A playfield of coloured sixels, drawn as Level 1 mosaics.
//!
//! At Level 1 a colour is a spacing attribute: it takes a cell of its own,
//! and applies from the next cell to the end of the row or the next
//! attribute (EN 300 706 clause 12.2, Table 26). The canvas puts each
//! colour change in the empty cell before the first cell of the new
//! colour; where there is none, that cell takes the colour already in
//! force. Games keep their sprites a cell apart where colour matters.

use alloc::vec::Vec;

use neoteletext_core::consts::{MOSAIC_BIT5, MOSAIC_BIT6};
use teletext_runtime::screen::{Colour, SIXELS_ACROSS, SIXELS_DOWN, SPACE, Screen};

use super::sprites::Sprite;

/// Each sixel's bit in a mosaic character, top-left first, row by row
/// (EN 300 706 Table 47: bits 0 to 4, then bit 6; bit 5 is always set).
pub const SIXEL_BITS: [u8; SIXELS_ACROSS * SIXELS_DOWN] = [0x01, 0x02, 0x04, 0x08, 0x10, MOSAIC_BIT6];

/// The colours, by number (Table 26).
const COLOURS: [Colour; 8] = [Colour::Black, Colour::Red, Colour::Green, Colour::Yellow, Colour::Blue, Colour::Magenta, Colour::Cyan, Colour::White];

/// The colour numbered `index`.
fn colour_of(index: u8) -> Colour {
    COLOURS.get(usize::from(index)).copied().unwrap_or(Colour::White)
}

/// A canvas of sixels, each unlit or lit in a colour.
#[derive(Clone, Debug)]
pub struct Canvas {
    width: usize,
    height: usize,
    /// Each sixel: 0 unlit, else its colour's number plus one.
    pixels: Vec<u8>,
}

impl Canvas {
    /// A clear canvas `width` by `height` sixels.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, pixels: alloc::vec![0; width.saturating_mul(height)] }
    }

    fn slot(&self, x: i32, y: i32) -> Option<usize> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        (x < self.width && y < self.height).then(|| y.saturating_mul(self.width).saturating_add(x))
    }

    /// Lights the sixel at `x`, `y` in `colour`; outside, nothing.
    pub fn plot(&mut self, x: i32, y: i32, colour: Colour) {
        if let Some(slot) = self.slot(x, y).and_then(|slot| self.pixels.get_mut(slot)) {
            *slot = colour.index().saturating_add(1);
        }
    }

    /// Lights a rectangle.
    pub fn rectangle(&mut self, x: i32, y: i32, width: i32, height: i32, colour: Colour) {
        for row in y..y.saturating_add(height) {
            for column in x..x.saturating_add(width) {
                self.plot(column, row, colour);
            }
        }
    }

    /// Draws `sprite` with its top-left at `x`, `y` in `colour`.
    pub fn sprite(&mut self, sprite: &Sprite, x: i32, y: i32, colour: Colour) {
        for row in 0..sprite.height {
            for column in 0..sprite.width {
                if sprite.lit(column, row) {
                    let (dx, dy) = (i32::try_from(column).unwrap_or(0), i32::try_from(row).unwrap_or(0));
                    self.plot(x.saturating_add(dx), y.saturating_add(dy), colour);
                }
            }
        }
    }

    /// The mosaic code of the cell at `cell_x`, `cell_y` and the colour
    /// most of its lit sixels have; `None` for an empty cell.
    fn cell(&self, cell_x: usize, cell_y: usize) -> Option<(u8, u8)> {
        let (x, y) = (cell_x.saturating_mul(SIXELS_ACROSS), cell_y.saturating_mul(SIXELS_DOWN));
        let mut code = MOSAIC_BIT5;
        let mut counts = [0_u8; 8];
        for (index, bit) in SIXEL_BITS.iter().enumerate() {
            let across = index.checked_rem(SIXELS_ACROSS).unwrap_or(0);
            let down = index.checked_div(SIXELS_ACROSS).unwrap_or(0);
            let (sx, sy) = (x.saturating_add(across), y.saturating_add(down));
            let pixel = if sx < self.width { self.pixels.get(sy.saturating_mul(self.width).saturating_add(sx)).copied().unwrap_or(0) } else { 0 };
            if pixel != 0 {
                code |= bit;
                if let Some(count) = counts.get_mut(usize::from(pixel.saturating_sub(1))) {
                    *count = count.saturating_add(1);
                }
            }
        }
        let (colour, count) =
            counts.iter().enumerate().fold((0_u8, 0_u8), |best, (index, &count)| if count > best.1 { (u8::try_from(index).unwrap_or(0), count) } else { best });
        (count > 0).then_some((code, colour))
    }

    /// Draws the canvas on `screen` from `row`, `column`, which must be at
    /// least 1: the cell before each row is where its first colour goes.
    /// The rows' cells it covers are overwritten, empty ones as spaces.
    pub fn draw(&self, screen: &mut Screen, row: usize, column: usize) {
        let cells_across = self.width.div_ceil(SIXELS_ACROSS);
        let cells_down = self.height.div_ceil(SIXELS_DOWN);
        for cell_y in 0..cells_down {
            let screen_row = row.saturating_add(cell_y);
            screen.code(screen_row, column.saturating_sub(1), SPACE);
            let mut current: Option<u8> = None;
            let mut previous_empty = true;
            for cell_x in 0..cells_across {
                let screen_column = column.saturating_add(cell_x);
                let Some((code, colour)) = self.cell(cell_x, cell_y) else {
                    screen.code(screen_row, screen_column, SPACE);
                    previous_empty = true;
                    continue;
                };
                // A row starts in alphanumerics, so its first lit cell
                // always has an empty cell (or the row's first) before it.
                if current != Some(colour) && previous_empty {
                    screen.code(screen_row, screen_column.saturating_sub(1), colour_of(colour).mosaic());
                    current = Some(colour);
                }
                screen.code(screen_row, screen_column, code);
                previous_empty = false;
            }
        }
    }
}
