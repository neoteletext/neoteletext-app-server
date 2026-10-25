//! Pages 410 to 438, The Lost Crown: a choose-your-path story in the
//! old teletext way. Every node is a shared page, published once at the
//! start (decision 7: no work for any viewer); its choices are the
//! coloured Fastext links (EN 300 706 clause 9.6), which the receiver
//! follows itself, so the story needs no input.

pub mod pictures;
pub mod story;

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::COLUMNS;
use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Application, Context, Owner, PageEntry};

use super::text::{self, Block, Style, TEXT_COLUMN};
use crate::common;
use story::{Ending, NODES, Node};

/// The story's first page.
pub const START: u16 = story::CROSSROADS;
/// The story's name, in each page's title band.
const TITLE: &str = "The Lost Crown";
/// The row of the node's title, in double height.
const NODE_TITLE_ROW: usize = 3;
/// The picture's first row.
const PICTURE_ROW: usize = 5;
/// The picture's first column: centred.
const PICTURE_COLUMN: usize = (COLUMNS - PICTURE_CELLS) / 2;
/// The picture's width in cells.
const PICTURE_CELLS: usize = 20;
/// The story text's first row.
pub const TEXT_ROW: usize = PICTURE_ROW + pictures::ROWS + 1;
/// Rows the story text may take.
pub const TEXT_ROWS: usize = 7;
/// The width of the story text: after the colour attribute in column 0,
/// less a cell's margin at the right.
pub const TEXT_WIDTH: usize = COLUMNS - 2;
/// The first choice's row.
const CHOICE_ROW: usize = TEXT_ROW + TEXT_ROWS;
/// The row of the words at the end.
const THE_END_ROW: usize = CHOICE_ROW;
/// The first choice's row at an ending, below the words.
const ENDING_CHOICE_ROW: usize = THE_END_ROW + 2;
/// The hint's row.
const HINT_ROW: usize = 23;
/// The cells before a double-height title: Double Height and its colour.
const TITLE_ATTRIBUTES: usize = 2;
/// A mosaic with every sixel set (EN 300 706 Table 47): the key's colour.
const BLOCK: u8 = 0x7F;
/// The longest title a page entry may have (the engine's
/// `contents.txt` rule).
const ENTRY_TITLE: usize = 32;
/// The Fastext colours, in the order of a node's choices.
const KEY_COLOURS: [Colour; 4] = [Colour::Red, Colour::Green, Colour::Yellow, Colour::Cyan];

const _: () = assert!(CHOICE_ROW + 4 <= HINT_ROW, "four choices fit above the hint");
const _: () = assert!(PICTURE_COLUMN >= 4, "a picture's background and attributes fit before it");

/// The Lost Crown: its pages, published once.
#[derive(Default)]
pub struct Crown;

/// The band's colour: how the node ends, if it does.
const fn band(ending: Ending) -> Colour {
    match ending {
        Ending::Not => Colour::Magenta,
        Ending::Won => Colour::Green,
        Ending::Lost => Colour::Red,
        Ending::Other => Colour::Blue,
    }
}

/// The page of `node`.
#[must_use]
pub fn draw(node: &Node) -> Screen {
    let mut labels: [(&str, Option<u16>); 4] = [("", None); 4];
    for (slot, choice) in labels.iter_mut().zip(node.choices) {
        *slot = (choice.label, Some(choice.to));
    }
    let mut screen = common::page(TITLE, band(node.ending), labels);
    let title_width = node.title.chars().count().saturating_add(TITLE_ATTRIBUTES);
    screen.double_height(NODE_TITLE_ROW, COLUMNS.saturating_sub(title_width) / 2, Colour::Yellow, node.title);
    node.picture.draw(&mut screen, PICTURE_ROW, PICTURE_COLUMN);
    let block = Block { first_row: TEXT_ROW, rows: TEXT_ROWS, column: TEXT_COLUMN, narrow_rows: 0, narrow_width: 0, width: TEXT_WIDTH };
    text::draw(&mut screen, node.text, block, Style { plain: Colour::White, highlight: Colour::Yellow });

    let first = if node.ending == Ending::Not {
        CHOICE_ROW
    } else {
        let (words, colour) = match node.ending {
            Ending::Won => ("You have won", Colour::Green),
            Ending::Lost => ("The End", Colour::Red),
            Ending::Not | Ending::Other => ("The End", Colour::Cyan),
        };
        let width = words.chars().count().saturating_add(TITLE_ATTRIBUTES);
        screen.double_height(THE_END_ROW, COLUMNS.saturating_sub(width) / 2, colour, words);
        ENDING_CHOICE_ROW
    };
    for (offset, (choice, colour)) in node.choices.iter().zip(KEY_COLOURS).enumerate() {
        let row = first.saturating_add(offset);
        let marker = [colour.mosaic(), BLOCK, Colour::White.text()];
        screen.codes(row, TEXT_COLUMN, &marker);
        screen.text(row, TEXT_COLUMN.saturating_add(marker.len()), choice.text);
    }
    screen.centred(HINT_ROW, Colour::Cyan, "Choose with the coloured keys");
    screen
}

/// A node's entry in the channel's contents.
fn entry(node: &Node) -> PageEntry {
    let title: String = alloc::format!("Lost Crown: {}", node.title).chars().take(ENTRY_TITLE).collect();
    PageEntry::new(node.page, &title)
}

impl Application for Crown {
    fn pages(&self) -> Vec<PageEntry> {
        NODES.iter().map(entry).collect()
    }

    fn start(&mut self, context: &mut Context<'_>) {
        for node in NODES {
            context.publish(Owner::Channel, node.page, &draw(node));
        }
    }
}
