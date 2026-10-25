//! Sound (pages 500 to 599): a streamed radio, a piano, a sound board and
//! a drum machine, in the engine's audio profile (decisions E.84 to E.86).
//!
//! - 500, the menu.
//! - 501, NeoTeletext Radio: one live stream of synthesized music for every
//!   listener (`radio.rs`).
//! - 502, the piano; 503, the sound board; 504, the drum machine: each
//!   viewer's own, playing the showcase's effects library (`studio.rs`).

use alloc::boxed::Box;
use alloc::vec::Vec;

use teletext_runtime::screen::Colour;
use teletext_runtime::{Application, Context, Owner, PageEntry};

use crate::channels::Picture;
use crate::common::page;
use crate::index::Section;

mod board;
mod draw;
mod drums;
mod piano;
mod radio;
mod studio;
mod synth;
mod tracker;
mod tunes;

/// The sound menu's page.
pub const MENU: u16 = 0x500;
/// NeoTeletext Radio's page.
pub const RADIO: u16 = 0x501;
/// The piano's page.
pub const PIANO: u16 = 0x502;
/// The sound board's page.
pub const BOARD: u16 = 0x503;
/// The drum machine's page.
pub const DRUMS: u16 = 0x504;

/// This area's applications.
#[must_use]
pub fn applications(_pictures: &[Picture]) -> Vec<Box<dyn Application>> {
    alloc::vec![Box::new(Menu), Box::new(radio::Radio::new()), Box::new(studio::Studio::new())]
}

/// This area's entries in the index.
#[must_use]
pub fn sections() -> Vec<Section> {
    alloc::vec![Section { page: MENU, name: "Sound", about: "radio, piano, sound board, drums" }]
}

/// The menu's entries: page, name and what it is, on two lines.
const ENTRIES: [(u16, &str, &str, &str); 4] = [
    (RADIO, "NeoTeletext Radio", "music made live, one stream", "for every listener"),
    (PIANO, "Piano", "an octave on your letter keys,", "on any keyboard layout"),
    (BOARD, "Sound board", "nine effects on the number keys", ""),
    (DRUMS, "Drum machine", "sixteen steps of your own,", "played in a loop"),
];
/// The menu's first entry's row.
const FIRST_ROW: usize = 4;
/// Rows from one entry to the next.
const ROW_STEP: usize = 4;

/// Page 500, shared: the menu.
struct Menu;

impl Application for Menu {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(MENU, "Sound")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        let mut screen = page("Sound", Colour::Red, [("Radio", Some(RADIO)), ("Piano", Some(PIANO)), ("Board", Some(BOARD)), ("Drums", Some(DRUMS))]);
        let mut row = FIRST_ROW;
        for (number, name, about, more) in ENTRIES {
            screen.coloured(row, 1, Colour::Yellow, &alloc::format!("{number:03X}"));
            screen.coloured(row, 5, Colour::White, name);
            screen.coloured(row.saturating_add(1), 5, Colour::Cyan, about);
            screen.coloured(row.saturating_add(2), 5, Colour::Cyan, more);
            row = row.saturating_add(ROW_STEP);
        }
        screen.centred(21, Colour::Green, "Turn the browser's sound on to hear");
        screen.centred(22, Colour::Green, "these pages.");
        context.publish(Owner::Channel, MENU, &screen);
    }
}

#[cfg(test)]
mod tests;
