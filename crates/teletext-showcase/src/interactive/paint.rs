//! Page 206: a paint box for the mouse, each viewer's own canvas. The
//! page asks for `MOUSE_BUTTONS` (INPUT.md section 6.4), so a click on it
//! comes to the channel as a cell; a click toggles that block of the
//! canvas in the colour chosen on the palette row, and C clears.
//!
//! Each block of the canvas has its own colour although a colour is a
//! spacing attribute that takes a cell (EN 300 706 clause 12.2): a row
//! alternates colour attributes and blocks under Hold Mosaics, so each
//! attribute's cell shows the block before it again, in that block's
//! colour (colours are Set-After, Hold Mosaics Set-At, Table 26). A block
//! is two cells wide, the second the held copy. The canvas is blue, set by
//! New Background before the blocks, so blue is not on the palette.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;

use neoteletext_input::modes::MOUSE_BUTTONS;
use teletext_runtime::screen::{Colour, Screen, attribute};
use teletext_runtime::{Context, Input, Owner, SessionId};

use super::{CALCULATOR, MENU, PAINT};
use crate::common::page;
use crate::index::INDEX;
use crate::sound::{Effect, Sounds};

/// Blocks across: from column 4 to 39, two cells each.
pub(super) const ACROSS: usize = 18;
/// Rows of blocks.
pub(super) const DOWN: usize = 17;
/// The canvas's top row.
pub(super) const TOP: usize = 3;
/// The first block's column, after the canvas's colour, New Background,
/// Hold Mosaics and the block's colour attribute.
pub(super) const FIRST_COLUMN: usize = 4;
/// The canvas's colour.
const CANVAS: Colour = Colour::Blue;
/// Cells each block takes: itself and its held copy.
const BLOCK_CELLS: usize = 2;
/// The palette's row.
pub(super) const PALETTE_ROW: usize = 21;
/// The row that marks the colour chosen.
const MARK_ROW: usize = 22;
/// The palette's first column: the palette centred.
pub(super) const PALETTE_COLUMN: usize = 5;
/// Cells each colour of the palette takes: its attribute, three blocks and
/// a gap.
pub(super) const SWATCH_CELLS: usize = 5;
/// Blocks in each swatch.
const SWATCH_BLOCKS: usize = 3;
/// The colours to paint with: all but the canvas's, and black, which
/// Level 1 has no mosaic attribute for (Table 26).
pub(super) const PALETTE: [Colour; 6] = [Colour::Red, Colour::Green, Colour::Yellow, Colour::Magenta, Colour::Cyan, Colour::White];
/// A mosaic cell with all six sixels set (EN 300 706 Table 47).
pub(super) const SOLID: u8 = 0x7F;
/// A mosaic cell with none set.
const EMPTY: u8 = 0x20;
/// The left mouse button, which paints (INPUT.md section 5.4).
const LEFT_BUTTON: u8 = 0;
/// The right mouse button, which rubs out.
const RIGHT_BUTTON: u8 = 2;

/// One viewer's canvas and colour.
struct Canvas {
    blocks: [[Option<Colour>; ACROSS]; DOWN],
    colour: Colour,
}

impl Canvas {
    const fn new() -> Self {
        Self { blocks: [[None; ACROSS]; DOWN], colour: Colour::Yellow }
    }

    /// A press of `button` at `column`, `row`: the sound, if it did
    /// anything.
    fn press(&mut self, button: u8, column: usize, row: usize) -> Option<Effect> {
        if row == PALETTE_ROW {
            let swatch = column.checked_sub(PALETTE_COLUMN)? / SWATCH_CELLS;
            self.colour = *PALETTE.get(swatch)?;
            return Some(Effect::Coin);
        }
        // A block's colour attribute's cell shows the block before it, so
        // it is that block's.
        let across = column.checked_sub(FIRST_COLUMN)? / BLOCK_CELLS;
        let block = self.blocks.get_mut(row.checked_sub(TOP)?)?.get_mut(across)?;
        *block = match button {
            LEFT_BUTTON if *block != Some(self.colour) => Some(self.colour),
            LEFT_BUTTON | RIGHT_BUTTON => None,
            _ => return None,
        };
        Some(Effect::Click)
    }

    fn clear(&mut self) {
        self.blocks = [[None; ACROSS]; DOWN];
    }

    fn screen(&self) -> Screen {
        let mut screen = page("Mouse paint", Colour::Green, [("Input", Some(MENU)), ("Sums", Some(CALCULATOR)), ("", None), ("Index", Some(INDEX))]);
        screen.modes(MOUSE_BUTTONS);
        for (row, blocks) in (TOP..).zip(self.blocks.iter()) {
            screen.codes(row, 0, &[CANVAS.mosaic(), attribute::NEW_BACKGROUND, attribute::HOLD_MOSAICS]);
            // A colour attribute is needed before every block; an empty
            // block keeps the colour before it, which nothing shows.
            let mut colour = Colour::White;
            let mut column = FIRST_COLUMN.saturating_sub(1);
            for block in blocks {
                colour = block.unwrap_or(colour);
                screen.code(row, column, colour.mosaic());
                screen.code(row, column.saturating_add(1), if block.is_some() { SOLID } else { EMPTY });
                column = column.saturating_add(BLOCK_CELLS);
            }
            // The last block's held copy.
            screen.code(row, column, colour.mosaic());
        }
        let mut column = PALETTE_COLUMN;
        for colour in PALETTE {
            screen.code(PALETTE_ROW, column, colour.mosaic());
            screen.fill(PALETTE_ROW, column.saturating_add(1), SWATCH_BLOCKS, SOLID);
            if colour == self.colour {
                screen.coloured(MARK_ROW, column, Colour::White, "\u{2191}\u{2191}\u{2191}");
            }
            column = column.saturating_add(SWATCH_CELLS);
        }
        screen.coloured(23, 1, Colour::White, "Left paints, right rubs out, C clears");
        screen
    }
}

/// Every viewer's canvas.
#[derive(Default)]
pub(super) struct Paint {
    canvases: BTreeMap<SessionId, Box<Canvas>>,
}

impl Paint {
    /// The page as a viewer first sees it, which is also the channel's
    /// copy, for a receiver that shows the page before saying it opened it.
    pub(super) fn first_screen() -> Screen {
        Canvas::new().screen()
    }

    fn publish(&self, session: SessionId, context: &mut Context<'_>) {
        if let Some(canvas) = self.canvases.get(&session) {
            context.publish(Owner::Session(session), PAINT, &canvas.screen());
        }
    }

    pub(super) fn opened(&mut self, session: SessionId, context: &mut Context<'_>) {
        self.canvases.entry(session).or_insert_with(|| Box::new(Canvas::new()));
        self.publish(session, context);
    }

    pub(super) fn input(&mut self, session: SessionId, input: &Input<'_>, sounds: &mut Sounds, context: &mut Context<'_>) {
        let canvas = self.canvases.entry(session).or_insert_with(|| Box::new(Canvas::new()));
        let effect = match *input {
            Input::Mouse { pressed: true, button, column, row } => {
                let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else { return };
                canvas.press(button, column, row)
            }
            _ if matches!(input.character(), Some('c' | 'C')) => {
                canvas.clear();
                Some(Effect::Explosion)
            }
            _ => None,
        };
        if let Some(effect) = effect {
            self.publish(session, context);
            sounds.play(session, effect, context);
        }
    }

    pub(super) fn ended(&mut self, session: SessionId) {
        self.canvases.remove(&session);
    }
}
