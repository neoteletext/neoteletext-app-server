//! Page 503, the sound board: the showcase's effects on the number keys 1
//! to 9, or a click on their buttons, each viewer's page lighting the last
//! one played.

use alloc::format;

use teletext_runtime::screen::{Colour, Screen};

use super::draw::badge;
use crate::sound::Effect;

/// The board's buttons: each effect of the library and its label, keys 1
/// to 9 in order.
pub(crate) const BUTTONS: [(Effect, &str); 9] = [
    (Effect::Laser, "Laser"),
    (Effect::Explosion, "Boom"),
    (Effect::Hit, "Hit"),
    (Effect::Coin, "Coin"),
    (Effect::Jump, "Jump"),
    (Effect::Win, "Win"),
    (Effect::Lose, "Lose"),
    (Effect::Click, "Click"),
    (Effect::Bell, "Bell"),
];
/// Buttons in a row of the grid.
const ACROSS: usize = 3;
/// Columns from one button to the next: three attributes, the label's
/// nine cells, and Black Background.
const BUTTON_STEP: usize = 13;
/// A label's width, padded.
const LABEL_CELLS: usize = 9;
/// The first row of buttons.
const FIRST_ROW: usize = 5;
/// Rows from one row of buttons to the next: three rows of button and a
/// gap.
const ROW_STEP: usize = 4;
/// A button's rows.
const BUTTON_ROWS: usize = 3;

/// The button for key `character`, `1` to `9`.
pub(crate) fn button_of(character: char) -> Option<usize> {
    let digit = usize::try_from(character.to_digit(10)?).ok()?;
    digit.checked_sub(1).filter(|&index| index < BUTTONS.len())
}

/// The button at a cell, for the mouse.
pub(crate) fn button_at(column: usize, row: usize) -> Option<usize> {
    let down = row.checked_sub(FIRST_ROW)?;
    if down % ROW_STEP >= BUTTON_ROWS {
        return None;
    }
    let across = column / BUTTON_STEP;
    if across >= ACROSS {
        return None;
    }
    let index = (down / ROW_STEP).saturating_mul(ACROSS).saturating_add(across);
    (index < BUTTONS.len()).then_some(index)
}

/// Draws the board, with `lit` the button last played.
pub(crate) fn draw(screen: &mut Screen, lit: Option<usize>) {
    screen.coloured(3, 1, Colour::Cyan, "Press 1 to 9, or click a button");
    for (index, (_, label)) in BUTTONS.iter().enumerate() {
        let column = (index % ACROSS).saturating_mul(BUTTON_STEP);
        let top = FIRST_ROW.saturating_add((index / ACROSS).saturating_mul(ROW_STEP));
        let (background, foreground) = if lit == Some(index) { (Colour::Yellow, Colour::Blue) } else { (Colour::Blue, Colour::White) };
        let text = format!("{} {label}", index.saturating_add(1));
        for row in top..top.saturating_add(BUTTON_ROWS) {
            let shown = if row == top.saturating_add(1) { text.as_str() } else { "" };
            badge(screen, row, column, background, foreground, &format!("{shown:<LABEL_CELLS$}"));
        }
    }
    let played = lit.and_then(|index| BUTTONS.get(index)).map_or("nothing yet", |(_, label)| label);
    screen.coloured(18, 1, Colour::White, &format!("Last played: {played}"));
    screen.coloured(20, 1, Colour::Cyan, "Each effect is sent to you once, then");
    screen.coloured(21, 1, Colour::Cyan, "each press is one 42-byte command.");
}
