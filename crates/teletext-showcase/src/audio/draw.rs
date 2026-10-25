//! Drawing the sound pages' parts: labels on a coloured background, and
//! mosaic bars, in Level 1 attributes (EN 300 706 Table 26).

use teletext_runtime::screen::{Colour, SIXELS_ACROSS, SIXELS_DOWN, Screen, Sixels, attribute};

/// A label on a coloured background from `column`: the background colour,
/// New Background, the text's colour, the text, then Black Background, so
/// the label takes its text's length and four cells.
pub(crate) fn badge(screen: &mut Screen, row: usize, column: usize, background: Colour, foreground: Colour, text: &str) {
    screen.codes(row, column, &[background.text(), attribute::NEW_BACKGROUND, foreground.text()]);
    let after = column.saturating_add(3);
    screen.text(row, after, text);
    screen.code(row, after.saturating_add(text.chars().count()), attribute::BLACK_BACKGROUND);
}

/// The cells a bar of `cells` cells and `rows` rows has, in sixels.
pub(crate) const fn sixels(cells: usize, rows: usize) -> (usize, usize) {
    (cells.saturating_mul(SIXELS_ACROSS), rows.saturating_mul(SIXELS_DOWN))
}

/// A vertical bar `cells` wide and `rows` tall at `row`, `column`, filled
/// from the bottom to `height` sixels, its colour in the cell before.
pub(crate) fn column_bar(screen: &mut Screen, row: usize, column: usize, size: (usize, usize), height: usize, colour: Colour) {
    let (width, tall) = sixels(size.0, size.1);
    let mut canvas = Sixels::new(width, tall);
    for y in tall.saturating_sub(height.min(tall))..tall {
        for x in 0..width {
            canvas.set(x, y, true);
        }
    }
    canvas.draw(screen, row, column, colour);
}

/// A horizontal bar `cells` wide at `row`, `column`, filled from the left
/// to `filled` sixels, the rest drawn as a thin track.
pub(crate) fn track_bar(screen: &mut Screen, row: usize, column: usize, cells: usize, filled: usize, colour: Colour) {
    let (width, tall) = sixels(cells, 1);
    let mut canvas = Sixels::new(width, tall);
    for x in 0..width {
        if x < filled {
            for y in 0..tall {
                canvas.set(x, y, true);
            }
        } else {
            // The middle sixel row: a line where the bar has yet to reach.
            canvas.set(x, 1, true);
        }
    }
    canvas.draw(screen, row, column, colour);
}
