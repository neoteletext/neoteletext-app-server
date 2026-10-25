//! What the service pages draw alike: word-wrapped text, mosaic pictures
//! from text art, large mosaic digits, the demonstration-data banner, and
//! publishing with the Fastext row shown.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::COLUMNS;
use teletext_runtime::screen::{Colour, ROWS, Screen, Sixels};
use teletext_runtime::{Context, Owner, PageFile};

/// The last row of a page, which carries the Fastext labels.
pub const FASTEXT_ROW: usize = ROWS - 1;
/// The row above it, where the demonstration-data banner goes.
pub const BANNER_ROW: usize = FASTEXT_ROW - 1;
/// The first row below the title band (rows 1 and 2).
pub const FIRST_ROW: usize = 3;
/// The column a body line's colour attribute takes, leaving column 0 as
/// a margin.
pub const BODY_COLUMN: usize = 1;
/// The characters of a body line after its colour attribute.
pub const BODY_WIDTH: usize = 37;
/// The title band's row, where the sub-page counter goes.
const TITLE_ROW: usize = 1;

/// `text` broken at spaces into lines of at most `width` characters; a
/// word longer than a line is cut.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let needed = line.chars().count().saturating_add(word.chars().count()).saturating_add(usize::from(!line.is_empty()));
        if needed > width && !line.is_empty() {
            lines.push(core::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.extend(word.chars().take(width));
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Writes `text` wrapped to the body's width from `row` in `colour`, and
/// answers the row after it.
pub fn paragraph(screen: &mut Screen, row: usize, colour: Colour, text: &str) -> usize {
    let mut row = row;
    for line in wrap(text, BODY_WIDTH) {
        screen.coloured(row, BODY_COLUMN, colour, &line);
        row = row.saturating_add(1);
    }
    row
}

/// A band across `row`: `text` in `colour` on `background`.
pub fn band(screen: &mut Screen, row: usize, background: Colour, colour: Colour, text: &str) {
    screen.background(row, 0, background);
    screen.coloured(row, 2, colour, text);
}

/// The banner every simulated page carries: its figures are made up.
pub fn demonstration(screen: &mut Screen) {
    band(screen, BANNER_ROW, Colour::Red, Colour::Yellow, "DEMONSTRATION DATA: all simulated");
}

/// The band above the Fastext row naming what the carousel shows next.
pub fn next(screen: &mut Screen, what: &str) {
    band(screen, BANNER_ROW.saturating_sub(1), Colour::Blue, Colour::Yellow, &alloc::format!("Next: {what}"));
}

/// Writes `text` in `colour` so that it ends in the last column.
pub fn right(screen: &mut Screen, row: usize, colour: Colour, text: &str) {
    // The colour attribute takes the cell before the text.
    let width = text.chars().count().saturating_add(1);
    screen.coloured(row, COLUMNS.saturating_sub(width), colour, text);
}

/// The sub-page counter, "2/7", at the right of the title band.
pub fn counter(screen: &mut Screen, number: usize, of: usize) {
    right(screen, TITLE_ROW, Colour::White, &alloc::format!("{number}/{of}"));
}

/// Sets the sixels of text art at `x`, `y`: `#` is set, anything else
/// clear.
pub fn art(canvas: &mut Sixels, x: usize, y: usize, lines: &[&str]) {
    for (down, line) in lines.iter().enumerate() {
        for (across, character) in line.chars().enumerate() {
            if character == '#' {
                canvas.set(x.saturating_add(across), y.saturating_add(down), true);
            }
        }
    }
}

/// A large digit's width in font pixels.
pub const DIGIT_WIDTH: usize = 5;
/// Its height in font pixels.
pub const DIGIT_HEIGHT: usize = 7;
/// The pixels between characters.
const DIGIT_GAP: usize = 1;
/// The digits 0 to 9, five pixels by seven, each row's leftmost pixel in
/// bit 4: the dot-matrix shapes of character displays.
const DIGITS: [[u8; DIGIT_HEIGHT]; 10] = [
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
];
/// The colon, one pixel wide.
const COLON: [u8; DIGIT_HEIGHT] = [0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00];
/// The colon's width in font pixels.
const COLON_WIDTH: usize = 1;

/// Draws `text` (digits and colons) in large digits on `canvas` at `x`,
/// `y`, each font pixel `scale` sixels square; answers the width drawn in
/// sixels.
pub fn digits(canvas: &mut Sixels, x: usize, y: usize, text: &str, scale: usize) -> usize {
    let mut left = x;
    for character in text.chars() {
        let (rows, width) = match character.to_digit(10) {
            Some(digit) => (DIGITS.get(usize::try_from(digit).unwrap_or(0)).copied().unwrap_or_default(), DIGIT_WIDTH),
            None => (COLON, COLON_WIDTH),
        };
        for (down, bits) in rows.iter().enumerate() {
            for across in 0..width {
                let shift = u32::try_from(width.saturating_sub(1).saturating_sub(across)).unwrap_or(0);
                if bits.checked_shr(shift).unwrap_or(0) & 1 == 1 {
                    square(canvas, left.saturating_add(across.saturating_mul(scale)), y.saturating_add(down.saturating_mul(scale)), scale);
                }
            }
        }
        left = left.saturating_add(width.saturating_add(DIGIT_GAP).saturating_mul(scale));
    }
    left.saturating_sub(x)
}

/// Sets a square of `size` sixels.
fn square(canvas: &mut Sixels, x: usize, y: usize, size: usize) {
    for down in 0..size {
        for across in 0..size {
            canvas.set(x.saturating_add(across), y.saturating_add(down), true);
        }
    }
}

/// The page `screen` is, as sub-page `subpage`, with its Fastext row
/// shown: the runtime's `Screen::page` leaves X/27/0's "display row 24"
/// flag clear, and a receiver then hides row 24 (EN 300 706 clause
/// 9.6.2).
#[must_use]
pub fn with_fastext_row(screen: &Screen, number: u16, subpage: u8) -> PageFile {
    let mut page = screen.page(number, subpage);
    page.fastext_row_24 = true;
    page
}

/// Publishes `screen` as the channel's page `number`.
pub fn publish(context: &mut Context<'_>, number: u16, screen: &Screen) {
    context.publish_subpages(Owner::Channel, number, alloc::vec![with_fastext_row(screen, number, 0)]);
}

/// Publishes `screens` as the channel's page `number`, sub-pages 1 on,
/// for the receiver to show in turn.
pub fn publish_carousel(context: &mut Context<'_>, number: u16, screens: &[Screen]) {
    let subpages = screens.iter().zip(1_u8..).map(|(screen, subpage)| with_fastext_row(screen, number, subpage)).collect();
    context.publish_subpages(Owner::Channel, number, subpages);
}
