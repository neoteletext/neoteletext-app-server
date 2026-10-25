//! A page built as a terminal program builds its screen: text and colour
//! at a row and column, mosaic graphics from a sixel canvas, Fastext links
//! and a cursor, into the engine's [`PageFile`].

use alloc::string::String;
use neoteletext_core::consts::COLUMNS;
use neoteletext_core::types::{Magazine, PageAddress, PageNumber, SubCode};
use neoteletext_formats::page::{CursorLine, FastextLink, ModesLine, PageFile};

/// Rows of a page: the header row 0 and rows 1 to 24 (EN 300 706 clause
/// 9.3).
pub const ROWS: usize = 25;
/// The first display column of the header, after the receiver's own page
/// number (EN 300 706 clause 9.3.1).
pub const HEADER_TEXT_COLUMN: usize = 8;
/// A mosaic cell's sixels across (EN 300 706 clause 15.7.1).
pub const SIXELS_ACROSS: usize = 2;
/// A mosaic cell's sixels down.
pub const SIXELS_DOWN: usize = 3;

/// The eight colours of Level 1 (EN 300 706 Table 26, spacing attributes).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(missing_docs, reason = "the colours name themselves")]
pub enum Colour {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl Colour {
    /// The colour's number, 0 to 7.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// The spacing attribute for alphanumeric text in this colour: 0x00
    /// to 0x07 (Table 26).
    #[must_use]
    pub const fn text(self) -> u8 {
        self.index()
    }

    /// The spacing attribute for mosaic graphics in this colour: 0x10 to
    /// 0x17 (Table 26).
    #[must_use]
    pub const fn mosaic(self) -> u8 {
        0x10 | self.index()
    }
}

/// Spacing attributes of EN 300 706 Table 26.
pub mod attribute {
    /// Flash.
    pub const FLASH: u8 = 0x08;
    /// Steady.
    pub const STEADY: u8 = 0x09;
    /// Normal height.
    pub const NORMAL_HEIGHT: u8 = 0x0C;
    /// Double height.
    pub const DOUBLE_HEIGHT: u8 = 0x0D;
    /// Conceal.
    pub const CONCEAL: u8 = 0x18;
    /// Contiguous mosaic graphics.
    pub const CONTIGUOUS: u8 = 0x19;
    /// Separated mosaic graphics.
    pub const SEPARATED: u8 = 0x1A;
    /// Black background.
    pub const BLACK_BACKGROUND: u8 = 0x1C;
    /// New background: the foreground colour becomes the background.
    pub const NEW_BACKGROUND: u8 = 0x1D;
    /// Hold mosaics.
    pub const HOLD_MOSAICS: u8 = 0x1E;
    /// Release mosaics.
    pub const RELEASE_MOSAICS: u8 = 0x1F;
}

/// The code of a space.
pub const SPACE: u8 = 0x20;
/// The base of a mosaic character: bit 5 always set (Table 47).
const MOSAIC_BASE: u8 = 0x20;
/// The bit of the bottom-right sixel, which sits above bit 5 in the code.
const BOTTOM_RIGHT_SIXEL: u8 = 0x40;

/// The English national option set's codes for the characters that differ
/// from ASCII (EN 300 706 Table 36): `£` at 0x23 and `#` at 0x5F.
fn english_code(character: char) -> u8 {
    match character {
        '£' => 0x23,
        '#' => 0x5F,
        '\u{2190}' => 0x5B,
        '\u{2192}' => 0x5D,
        '\u{2191}' => 0x5E,
        // Printable ASCII the set shares; anything else is a space.
        ' '..='~' => u8::try_from(character).unwrap_or(SPACE),
        _ => SPACE,
    }
}

/// A screen being drawn: 25 rows of 40 seven-bit codes, its links, cursor
/// and input modes.
#[derive(Clone, Debug)]
pub struct Screen {
    rows: [[u8; COLUMNS]; ROWS],
    fastext: [Option<u16>; 4],
    index: Option<u16>,
    cursor: Option<CursorLine>,
    modes: u32,
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen {
    /// A blank screen.
    #[must_use]
    pub const fn new() -> Self {
        Self { rows: [[SPACE; COLUMNS]; ROWS], fastext: [None; 4], index: None, cursor: None, modes: 0 }
    }

    /// Writes `codes` at `row`, `column`, cut at the row's end.
    pub fn codes(&mut self, row: usize, column: usize, codes: &[u8]) {
        let Some(line) = self.rows.get_mut(row) else { return };
        for (slot, &code) in line.iter_mut().skip(column).zip(codes) {
            *slot = code & 0x7F;
        }
    }

    /// Writes one code.
    pub fn code(&mut self, row: usize, column: usize, code: u8) {
        self.codes(row, column, &[code]);
    }

    /// Writes `text` at `row`, `column` in the English option set, cut at
    /// the row's end.
    pub fn text(&mut self, row: usize, column: usize, text: &str) {
        let Some(line) = self.rows.get_mut(row) else { return };
        for (slot, character) in line.iter_mut().skip(column).zip(text.chars()) {
            *slot = english_code(character);
        }
    }

    /// Writes `text` in `colour`: the colour attribute takes the cell
    /// before it.
    pub fn coloured(&mut self, row: usize, column: usize, colour: Colour, text: &str) {
        self.code(row, column, colour.text());
        self.text(row, column.saturating_add(1), text);
    }

    /// Writes `text` centred on `row` in `colour`.
    pub fn centred(&mut self, row: usize, colour: Colour, text: &str) {
        let width = text.chars().count().saturating_add(1);
        let column = COLUMNS.saturating_sub(width) / 2;
        self.coloured(row, column, colour, text);
    }

    /// Writes a double-height line: the text on `row`, which also covers
    /// `row + 1` (EN 300 706 clause 12.3).
    pub fn double_height(&mut self, row: usize, column: usize, colour: Colour, text: &str) {
        self.code(row, column, attribute::DOUBLE_HEIGHT);
        self.coloured(row, column.saturating_add(1), colour, text);
    }

    /// Sets a background colour for the rest of `row` from `column`: the
    /// colour and New Background take two cells.
    pub fn background(&mut self, row: usize, column: usize, colour: Colour) {
        self.codes(row, column, &[colour.text(), attribute::NEW_BACKGROUND]);
    }

    /// The header row's text, after the receiver's page number.
    pub fn header(&mut self, text: &str) {
        self.text(0, HEADER_TEXT_COLUMN, text);
    }

    /// Fills `columns` cells of `row` from `column` with `code`.
    pub fn fill(&mut self, row: usize, column: usize, count: usize, code: u8) {
        let Some(line) = self.rows.get_mut(row) else { return };
        for slot in line.iter_mut().skip(column).take(count) {
            *slot = code & 0x7F;
        }
    }

    /// The code at `row`, `column`.
    #[must_use]
    pub fn at(&self, row: usize, column: usize) -> u8 {
        self.rows.get(row).and_then(|line| line.get(column)).copied().unwrap_or(SPACE)
    }

    /// A row's codes.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<&[u8; COLUMNS]> {
        self.rows.get(row)
    }

    /// The Fastext links: red, green, yellow, cyan (EN 300 706 clause
    /// 9.6), each a page number such as `0x100`.
    pub fn fastext(&mut self, links: [Option<u16>; 4]) {
        self.fastext = links;
    }

    /// The Index link (the fifth Fastext link).
    pub fn index_link(&mut self, page: u16) {
        self.index = Some(page);
    }

    /// The cursor at `row`, `column`, with `width` cells the receiver may
    /// echo into and the cursor record's flags (INPUT.md section 7.1).
    pub fn cursor(&mut self, row: u8, column: u8, width: u8, flags: u8) {
        self.cursor = Some(CursorLine { row, column, width, flags, acked: 0, line: 0 });
    }

    /// The input modes the page asks for (INPUT.md section 6).
    pub fn modes(&mut self, modes: u32) {
        self.modes = modes;
    }

    /// The page this screen is, as the engine's page model.
    #[must_use]
    pub fn page(&self, number: u16, subpage: u8) -> PageFile {
        let mut page = PageFile::new();
        page.address = address(number);
        page.subpage = subpage;
        page.subcode = SubCode::new(decimal_subcode(subpage));
        for (row, codes) in self.rows.iter().enumerate() {
            page.set_row(row, codes);
        }
        let links = [self.fastext[0], self.fastext[1], self.fastext[2], self.fastext[3], self.index];
        if links.iter().any(Option::is_some) {
            // The link control byte's bit 4 shows row 24, where the
            // Fastext prompts are (EN 300 706 Table 14); without it the
            // receiver hides the row.
            page.fastext_row_24 = true;
            for (slot, link) in page.fastext.iter_mut().zip(links) {
                *slot = link.map(|number| FastextLink { address: address(number), subcode: SubCode::ANY });
            }
        }
        page.cursor = self.cursor;
        if self.modes != 0 {
            page.input_modes = Some(ModesLine { modes: self.modes, line: 0 });
        }
        page
    }

    /// The text of `row`, for tests: the codes as characters.
    #[must_use]
    pub fn row_text(&self, row: usize) -> String {
        self.rows.get(row).map(|line| line.iter().map(|&code| char::from(code)).collect()).unwrap_or_default()
    }
}

/// A sub-page's number as a sub-code: its decimal digits, one to a
/// hexadecimal digit, as page numbers are written (sub-page 10 is
/// `0010`, as TTI's `SC` line and receivers show it).
#[must_use]
pub const fn decimal_subcode(subpage: u8) -> u16 {
    let tens = (subpage / 10) % 10;
    let units = subpage % 10;
    ((tens as u16) << 4) | units as u16
}

/// The page every channel has: 100, where a number outside the channel
/// falls back to.
pub const FIRST_PAGE: u16 = 0x100;

/// A page number such as `0x100` as the engine's page address: magazine
/// `1` to `8` and page `00` to `FF`; anything else is page 100.
#[must_use]
pub fn address(number: u16) -> PageAddress {
    PageAddress::from_mpp(number).or(PageAddress::from_mpp(FIRST_PAGE)).unwrap_or(PageAddress { magazine: Magazine::from_bits(1), page: PageNumber(0) })
}

/// A canvas of sixels, drawn as mosaic graphics into a block of cells.
#[derive(Clone, Debug)]
pub struct Sixels {
    width: usize,
    height: usize,
    bits: alloc::vec::Vec<bool>,
}

impl Sixels {
    /// A clear canvas `width` by `height` sixels.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, bits: alloc::vec![false; width.saturating_mul(height)] }
    }

    /// Its size in sixels.
    #[must_use]
    pub const fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Sets the sixel at `x`, `y`; outside the canvas, nothing.
    pub fn set(&mut self, x: usize, y: usize, on: bool) {
        if x < self.width
            && let Some(slot) = self.bits.get_mut(y.saturating_mul(self.width).saturating_add(x))
        {
            *slot = on;
        }
    }

    /// Whether the sixel at `x`, `y` is set; outside the canvas, not.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> bool {
        x < self.width && self.bits.get(y.saturating_mul(self.width).saturating_add(x)).copied().unwrap_or(false)
    }

    /// Clears the canvas.
    pub fn clear(&mut self) {
        self.bits.iter_mut().for_each(|bit| *bit = false);
    }

    /// The mosaic character for the cell whose top-left sixel is `x`, `y`
    /// (EN 300 706 Table 47: bits 0 to 4 and 6, top-left first).
    #[must_use]
    pub fn cell(&self, x: usize, y: usize) -> u8 {
        let right = x.saturating_add(1);
        let sixels = [
            (x, y, 0x01),
            (right, y, 0x02),
            (x, y.saturating_add(1), 0x04),
            (right, y.saturating_add(1), 0x08),
            (x, y.saturating_add(2), 0x10),
            (right, y.saturating_add(2), BOTTOM_RIGHT_SIXEL),
        ];
        sixels.iter().filter(|&&(sx, sy, _)| self.get(sx, sy)).fold(MOSAIC_BASE, |code, &(_, _, bit)| code | bit)
    }

    /// Draws the canvas as mosaic cells on `screen` from `row`, `column`:
    /// each row of cells begins with the mosaic colour attribute, in the
    /// cell before `column`.
    pub fn draw(&self, screen: &mut Screen, row: usize, column: usize, colour: Colour) {
        let cells_across = self.width.div_ceil(SIXELS_ACROSS);
        let cells_down = self.height.div_ceil(SIXELS_DOWN);
        for cell_row in 0..cells_down {
            let screen_row = row.saturating_add(cell_row);
            if column > 0 {
                screen.code(screen_row, column.saturating_sub(1), colour.mosaic());
            }
            for cell_column in 0..cells_across {
                let code = self.cell(cell_column.saturating_mul(SIXELS_ACROSS), cell_row.saturating_mul(SIXELS_DOWN));
                screen.code(screen_row, column.saturating_add(cell_column), code);
            }
        }
    }
}

#[cfg(test)]
mod tests;
