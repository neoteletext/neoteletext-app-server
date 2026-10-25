//! Page 110, the long read: an article far longer than a page's 25 rows,
//! scrolled within the page. Each viewer's place is their own page
//! (`SERVER.md` section 2), published as they scroll; a viewer who has not
//! scrolled sees the shared start.
//!
//! - **Keys:** Page Down (or Space) and Page Up a screen, keeping one line
//!   for context; Down and Up a line; Home and End.
//! - **The wheel:** three lines a notch.
//! - **The scroll bar,** in the last column: a white block of whole cells,
//!   the part shown in proportion to the whole, on a thin blue track of
//!   mosaic sixels. A cell has one foreground colour (EN 300 706 clause
//!   12.2), so a block that ended part-way through a cell would paint that
//!   cell's track white too; the block moves by whole cells. A click above
//!   or below the block moves a screen.
//! - **Fastext:** the services' links. The page takes the mouse, so the
//!   receiver sends a click on a prompt here and the page follows it.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::COLUMNS;
use neoteletext_input::modes::MOUSE_BUTTONS;
use teletext_runtime::screen::{Colour, SIXELS_ACROSS, SIXELS_DOWN, Screen, Sixels};
use teletext_runtime::{Application, Context, Input, Named, Owner, PageEntry, SessionId};

use super::draw::{BODY_COLUMN, FIRST_ROW, right, wrap};
use super::{LONG_READ, navigation};
use crate::common::{self, FASTEXT_ROW};

/// The page's title.
const TITLE: &str = "The long read";
/// The title band's colour.
const BAND: Colour = Colour::Blue;
/// The first row of the article.
pub const VIEW_FIRST_ROW: usize = FIRST_ROW;
/// Rows of the article shown at once.
pub const VIEW_ROWS: usize = 19;
/// The row saying which lines are shown.
pub const POSITION_ROW: usize = VIEW_FIRST_ROW + VIEW_ROWS;
/// The row naming the keys.
const HINT_ROW: usize = POSITION_ROW + 1;
/// The scroll bar's column: the last.
pub const BAR_COLUMN: usize = COLUMNS - 1;
/// The cell before it, which holds its mosaic colour.
pub const BAR_ATTRIBUTE_COLUMN: usize = BAR_COLUMN - 1;
/// The characters of an article line after its colour attribute, ending
/// a cell short of the scroll bar's colour.
pub const TEXT_WIDTH: usize = BAR_ATTRIBUTE_COLUMN - BODY_COLUMN - 2;
/// The bar's height in cells.
pub const TRACK_CELLS: usize = VIEW_ROWS;
/// The shortest block, one cell, so it can always be seen and hit.
const SHORTEST_THUMB: usize = 1;
/// The sixel column of the bar's thin track: the left of the cell's two.
const TRACK_SIXEL: usize = 0;
/// Lines a wheel notch scrolls, as desktop browsers do.
const WHEEL_LINES: i64 = 3;
/// A wheel notch in the protocol's units (INPUT.md section 5.4: 1/120ths
/// of a line).
const NOTCH: i64 = 120;
/// The mouse's left button (INPUT.md section 5.4).
const LEFT_BUTTON: u8 = 0;

const _: () = assert!(HINT_ROW < FASTEXT_ROW, "the hint is above the Fastext row");
const _: () = assert!(BODY_COLUMN + 1 + TEXT_WIDTH < BAR_ATTRIBUTE_COLUMN, "a gap between the text and the bar");

/// A part of the article.
enum Part {
    Heading(&'static str),
    Paragraph(&'static str),
}

/// The article. Its facts are kept to what is well established about
/// teletext and its standard, ETSI EN 300 706.
const ARTICLE: &[Part] = &[
    Part::Heading("Longer than a page"),
    Part::Paragraph(
        "This page is longer than any teletext page could be. A page has 25 rows of 40 characters, and this article runs to well over a hundred lines. The server keeps all of it and sends you the part you are reading, as a page of your own.",
    ),
    Part::Paragraph(
        "Page Down and Page Up move a screen at a time, the arrow keys a line, and Home and End go to the start and the end. The wheel scrolls too. The bar on the right shows where you are: click above or below its block to move a screen.",
    ),
    Part::Heading("The page"),
    Part::Paragraph(
        "Row 0 is the header: the page number, the service's name, the date and a clock that keeps running while you wait. Rows 1 to 23 are the page itself. Row 24 shows the Fastext prompts, one for each coloured key.",
    ),
    Part::Heading("Packets"),
    Part::Paragraph(
        "Every row travels as a packet of 42 bytes. The first two give the magazine and the row, coded so that a receiver can correct one wrong bit in each. The other 40 are the row's characters: seven bits each, with an eighth for odd parity, so a damaged character can at least be spotted.",
    ),
    Part::Heading("Magazines and page numbers"),
    Part::Paragraph(
        "Pages are numbered from 100 to 899. The first digit is the magazine, one of eight. The other two are hexadecimal, so pages such as 1A0 exist, though a remote control with only digits cannot reach them.",
    ),
    Part::Heading("The carousel"),
    Part::Paragraph(
        "A broadcaster sends its pages one after another in an endless loop, the carousel. A receiver that wants a page waits for it to come round. On a big service that could take many seconds, which is why later sets kept pages in memory.",
    ),
    Part::Heading("Hidden in the picture"),
    Part::Paragraph(
        "The packets ride in the lines at the top of each television frame that a set does not show: the vertical blanking interval. A few of those lines in every field were enough for a whole service.",
    ),
    Part::Heading("Control bits"),
    Part::Paragraph(
        "The header carries switches for the page: erase the old copy before showing the new, a newsflash or a subtitle to be boxed into the picture, an update, and others. Every page this server sends asks the receiver to erase the old copy first.",
    ),
    Part::Heading("Spacing attributes"),
    Part::Paragraph(
        "At Level 1 every change of colour or style is a control character, and each one takes a cell of the row. Red text, a mosaic, a new background, double height or flashing each cost a space. That is why teletext text so often has a gap before a change of colour.",
    ),
    Part::Heading("Mosaics"),
    Part::Paragraph(
        "A mosaic character splits its cell into six blocks, two across and three down, called sixels. Contiguous mosaics fill the cell; separated mosaics leave a thin gap round each block. Pictures, maps and the bar on the right of this page are all drawn with them.",
    ),
    Part::Paragraph("Hold mosaics shows the last mosaic again in the cells that attributes take, so a picture need not have holes where its colours change."),
    Part::Heading("Fastext"),
    Part::Paragraph(
        "Fastext added a packet that links a page to four others, one for each coloured key, and to an index. A receiver that had stored those pages could show them at once, and the coloured prompts on row 24 told you where each key led.",
    ),
    Part::Heading("Higher levels"),
    Part::Paragraph(
        "Levels 2.5 and 3.5 added packets that place characters and colours without spending a cell, a palette of 32 colours, and characters a service defines itself. A Level 1 set ignores what it does not understand and still shows the page.",
    ),
    Part::Heading("Ceefax"),
    Part::Paragraph("The BBC's Ceefax service began in 1974 and closed in 2012."),
    Part::Heading("NeoTeletext"),
    Part::Paragraph(
        "NeoTeletext carries the same pages over the web instead of the air. A channel sends a page when it changes, and the browser shows it as a television would, at whichever level the page was made for.",
    ),
    Part::Paragraph(
        "Scrolling is new. On the air every viewer saw the same page; here the server can send each viewer their own, so one article can be as long as it needs to be.",
    ),
    Part::Heading("The end"),
    Part::Paragraph("You have reached the end. Home takes you back to the start, and the Fastext keys lead to the other services."),
];

/// One line of the article as shown: its colour and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Its colour.
    pub colour: Colour,
    /// Its text, at most [`TEXT_WIDTH`] characters.
    pub text: String,
}

/// The article laid out: each heading in yellow, each paragraph in white
/// wrapped to [`TEXT_WIDTH`], a blank line after each paragraph.
#[must_use]
pub fn lines() -> Vec<Line> {
    let mut lines = Vec::new();
    for part in ARTICLE {
        match part {
            Part::Heading(text) => lines.push(Line { colour: Colour::Yellow, text: String::from(*text) }),
            Part::Paragraph(text) => {
                lines.extend(wrap(text, TEXT_WIDTH).into_iter().map(|text| Line { colour: Colour::White, text }));
                lines.push(Line { colour: Colour::White, text: String::new() });
            }
        }
    }
    lines
}

/// `numerator` / `denominator`, rounded to the nearest whole number; 0
/// for a denominator of 0.
fn rounded(numerator: usize, denominator: usize) -> usize {
    numerator.saturating_mul(2).saturating_add(denominator).checked_div(denominator.saturating_mul(2)).unwrap_or(0)
}

/// Where the scroll bar's block lies, in cells from the top of the track:
/// its first cell and its length. The block's length is the share of the
/// article shown; it reaches the track's top exactly at the first line and
/// its bottom exactly at the last.
#[must_use]
pub fn thumb(top: usize, total: usize) -> (usize, usize) {
    let shown = VIEW_ROWS.min(total);
    let length = rounded(TRACK_CELLS.saturating_mul(shown), total).clamp(SHORTEST_THUMB, TRACK_CELLS);
    let last_top = total.saturating_sub(VIEW_ROWS);
    let room = TRACK_CELLS.saturating_sub(length);
    (rounded(room.saturating_mul(top.min(last_top)), last_top).min(room), length)
}

/// Draws the scroll bar for the view from line `top` of `total`.
fn draw_bar(screen: &mut Screen, top: usize, total: usize) {
    let (start, length) = thumb(top, total);
    let block = start..start.saturating_add(length);
    // One cell of each: the block fills its cell, the track is a line.
    let mut cells = Sixels::new(SIXELS_ACROSS.saturating_mul(2), SIXELS_DOWN);
    for y in 0..SIXELS_DOWN {
        for x in 0..SIXELS_ACROSS {
            cells.set(x, y, true);
        }
        cells.set(SIXELS_ACROSS.saturating_add(TRACK_SIXEL), y, true);
    }
    let (full, track) = (cells.cell(0, 0), cells.cell(SIXELS_ACROSS, 0));
    for cell_row in 0..TRACK_CELLS {
        let (colour, code) = if block.contains(&cell_row) { (Colour::White, full) } else { (Colour::Blue, track) };
        let row = VIEW_FIRST_ROW.saturating_add(cell_row);
        screen.code(row, BAR_ATTRIBUTE_COLUMN, colour.mosaic());
        screen.code(row, BAR_COLUMN, code);
    }
}

/// The page with the article from line `top`.
#[must_use]
pub fn draw(lines: &[Line], top: usize) -> Screen {
    let mut screen = common::page(TITLE, BAND, navigation(LONG_READ));
    screen.modes(MOUSE_BUTTONS);
    let top = top.min(last_top(lines.len()));
    for (offset, line) in lines.iter().skip(top).take(VIEW_ROWS).enumerate() {
        if !line.text.is_empty() {
            screen.coloured(VIEW_FIRST_ROW.saturating_add(offset), BODY_COLUMN, line.colour, &line.text);
        }
    }
    draw_bar(&mut screen, top, lines.len());
    let last = top.saturating_add(VIEW_ROWS).min(lines.len());
    screen.coloured(POSITION_ROW, BODY_COLUMN, Colour::Cyan, &format!("Lines {}-{last} of {}", top.saturating_add(1), lines.len()));
    let percent = last.saturating_mul(100).checked_div(lines.len()).unwrap_or(100);
    right(&mut screen, POSITION_ROW, Colour::Cyan, &format!("{percent}%"));
    screen.coloured(HINT_ROW, BODY_COLUMN, Colour::Green, "PgDn PgUp, arrows, Home, End, wheel");
    screen
}

/// The first line shown when the view is at the end.
const fn last_top(total: usize) -> usize {
    total.saturating_sub(VIEW_ROWS)
}

/// One viewer: the first line they see, and the wheel's turn not yet a
/// whole line.
#[derive(Default)]
struct Reader {
    top: usize,
    wheel: i64,
}

/// What an input asks of the view.
enum Move {
    /// Lines down, or up when negative.
    By(i64),
    /// To the start.
    Start,
    /// To the end.
    End,
    /// Another page.
    Leave(u16),
}

/// The page 110 application.
pub struct LongRead {
    lines: Vec<Line>,
    readers: BTreeMap<SessionId, Reader>,
}

impl Default for LongRead {
    fn default() -> Self {
        Self { lines: lines(), readers: BTreeMap::new() }
    }
}

/// A screen's move: one line short of the view, kept for context.
fn screenful() -> i64 {
    i64::try_from(VIEW_ROWS.saturating_sub(1)).unwrap_or(1)
}

impl LongRead {
    /// What `input` asks, from the reader at `top` with `wheel` held over.
    fn wanted(&self, input: &Input<'_>, top: usize, wheel: &mut i64) -> Option<Move> {
        if input.pressed(Named::PageDown) || input.character() == Some(' ') {
            return Some(Move::By(screenful()));
        }
        if input.pressed(Named::PageUp) {
            return Some(Move::By(screenful().saturating_neg()));
        }
        if input.pressed(Named::Down) {
            return Some(Move::By(1));
        }
        if input.pressed(Named::Up) {
            return Some(Move::By(-1));
        }
        if input.pressed(Named::Home) {
            return Some(Move::Start);
        }
        if input.pressed(Named::End) {
            return Some(Move::End);
        }
        match *input {
            Input::Wheel { dy, .. } => {
                // Whole lines; what is left of a notch waits for the next
                // turn, so a trackpad's small steps add up.
                let turned = wheel.saturating_add(i64::from(dy)).saturating_mul(WHEEL_LINES);
                let lines = turned / NOTCH;
                *wheel = (turned % NOTCH) / WHEEL_LINES;
                (lines != 0).then_some(Move::By(lines))
            }
            Input::Mouse { pressed: true, button: LEFT_BUTTON, column, row } => {
                let (column, row) = (usize::try_from(column).ok()?, usize::try_from(row).ok()?);
                if row == FASTEXT_ROW {
                    let links = navigation(LONG_READ);
                    let place = common::label_at(links.map(|(label, _)| label), column)?;
                    return links.get(place).and_then(|&(_, page)| page).map(Move::Leave);
                }
                let cell_row = row.checked_sub(VIEW_FIRST_ROW).filter(|&cell_row| cell_row < VIEW_ROWS)?;
                if column < BAR_ATTRIBUTE_COLUMN {
                    return None;
                }
                let (start, length) = thumb(top, self.lines.len());
                if cell_row < start {
                    Some(Move::By(screenful().saturating_neg()))
                } else if cell_row >= start.saturating_add(length) {
                    Some(Move::By(screenful()))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Application for LongRead {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(LONG_READ, "The long read")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        context.publish(Owner::Channel, LONG_READ, &draw(&self.lines, 0));
    }

    fn input(&mut self, session: SessionId, _page: u16, input: &Input<'_>, context: &mut Context<'_>) {
        let reader = self.readers.entry(session).or_default();
        let (top, mut wheel) = (reader.top, reader.wheel);
        let wanted = self.wanted(input, top, &mut wheel);
        let last = last_top(self.lines.len());
        let reader = self.readers.entry(session).or_default();
        reader.wheel = wheel;
        let next = match wanted {
            Some(Move::By(lines)) => {
                let moved = i64::try_from(top).unwrap_or(i64::MAX).saturating_add(lines).max(0);
                usize::try_from(moved).unwrap_or(last).min(last)
            }
            Some(Move::Start) => 0,
            Some(Move::End) => last,
            Some(Move::Leave(page)) => {
                context.goto(session, page);
                return;
            }
            None => return,
        };
        if next != top {
            reader.top = next;
            context.publish(Owner::Session(session), LONG_READ, &draw(&self.lines, next));
        }
    }

    fn ended(&mut self, session: SessionId, _context: &mut Context<'_>) {
        self.readers.remove(&session);
    }
}
