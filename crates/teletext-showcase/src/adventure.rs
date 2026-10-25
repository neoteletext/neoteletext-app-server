//! Adventure games (pages 400 to 499): the menu (400), Station Zero, a
//! typed text adventure that is each viewer's own (401), and The Lost
//! Crown, a story told on the coloured keys, each viewer's place their own
//! (402).

mod art;
pub mod crown;
pub mod station;
mod text;

#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Application, Context, Owner, PageEntry};

use crate::channels::Picture;
use crate::common;
use crate::index::{INDEX, Section};

/// The adventures' menu.
pub const MENU: u16 = 0x400;
/// The menu's title band colour.
const BAND: Colour = Colour::Magenta;
/// The first row of the menu's picture.
const PICTURE_ROW: usize = 3;
/// The menu's first entry's row.
const FIRST_ENTRY_ROW: usize = 10;
/// Rows between entries.
const ENTRY_STEP: usize = 5;
/// The column of an entry's name, after its page number.
const NAME_COLUMN: usize = 5;
/// The picture's first column: centred.
const PICTURE_COLUMN: usize = 10;
/// The column of the lines about an entry: their colour under the
/// name's colour, so the text lines up with the name.
const ABOUT_COLUMN: usize = NAME_COLUMN + 1;
/// Rows a double-height line covers: its own and the next (EN 300 706
/// clause 12.3).
const DOUBLE_HEIGHT_ROWS: usize = 2;
/// The row of the note at the foot.
const NOTE_ROW: usize = 21;

/// An entry of the menu: its page, its name and two lines about it.
struct Entry {
    page: u16,
    name: &'static str,
    about: [&'static str; 2],
}

/// The menu's entries.
const ENTRIES: [Entry; 2] = [
    Entry { page: station::PAGE, name: "Station Zero", about: ["A typed adventure in space:", "fourteen rooms, one way home"] },
    Entry { page: crown::PAGE, name: "The Lost Crown", about: ["A Fastext quest for a crown:", "eight endings, coloured keys"] },
];

/// This area's applications.
#[must_use]
pub fn applications(_pictures: &[Picture]) -> Vec<Box<dyn Application>> {
    alloc::vec![Box::new(Menu), Box::new(station::Station::new()), Box::new(crown::Crown::default())]
}

/// This area's entries in the index.
#[must_use]
pub fn sections() -> Vec<Section> {
    alloc::vec![Section { page: MENU, name: "Adventures", about: "a typed adventure and a Fastext quest" }]
}

/// Page 400: the adventures, published once.
#[derive(Default)]
pub struct Menu;

/// The menu's page.
#[must_use]
pub fn menu() -> Screen {
    let mut screen = common::page("Adventures", BAND, [("Station", Some(station::PAGE)), ("Crown", Some(crown::PAGE)), ("", None), ("Index", Some(INDEX))]);
    crown::pictures::CASTLE.draw(&mut screen, PICTURE_ROW, PICTURE_COLUMN);
    let mut row = FIRST_ENTRY_ROW;
    for entry in &ENTRIES {
        screen.coloured(row, 0, Colour::Yellow, &alloc::format!("{:03X}", entry.page));
        screen.double_height(row, NAME_COLUMN, Colour::White, entry.name);
        for (offset, line) in entry.about.iter().enumerate() {
            screen.coloured(row.saturating_add(DOUBLE_HEIGHT_ROWS).saturating_add(offset), ABOUT_COLUMN, Colour::Cyan, line);
        }
        row = row.saturating_add(ENTRY_STEP);
    }
    screen.centred(NOTE_ROW, Colour::Green, "Station Zero takes the keyboard");
    screen
}

impl Application for Menu {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(MENU, "Adventures")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        context.publish(Owner::Channel, MENU, &menu());
    }
}
