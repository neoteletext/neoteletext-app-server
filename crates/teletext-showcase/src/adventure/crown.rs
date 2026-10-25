//! Page 402, The Lost Crown: a choose-your-path story in the old teletext
//! way, on one page. Each viewer's place in the story is their own
//! (`SERVER.md` section 2: the page is the session's); the coloured keys
//! choose (F1 to F4, the remote's colour keys, or r, g, y and c as in the
//! engine's viewer), and so does a click on a choice or its prompt. A
//! viewer who has not chosen yet sees the shared first node.

pub mod pictures;
pub mod story;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use neoteletext_core::consts::COLUMNS;
use neoteletext_input::keys;
use neoteletext_input::modes::MOUSE_BUTTONS;
use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Application, Context, Input, Named, Owner, PageEntry, SessionId};

use super::text::{self, Block, Style, TEXT_COLUMN};
use crate::common::{self, FASTEXT_ROW};
use story::{CROSSROADS, Ending, Id, Leads, NODES, Node};

/// The story's page.
pub const PAGE: u16 = 0x402;
/// The story's name, in the page's title band and the contents.
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
pub const CHOICE_ROW: usize = TEXT_ROW + TEXT_ROWS;
/// The row of the words at the end.
const THE_END_ROW: usize = CHOICE_ROW;
/// The first choice's row at an ending, below the words.
pub const ENDING_CHOICE_ROW: usize = THE_END_ROW + 2;
/// The hint's row.
const HINT_ROW: usize = 23;
/// The cells before a double-height title: Double Height and its colour.
const TITLE_ATTRIBUTES: usize = 2;
/// A mosaic with every sixel set (EN 300 706 Table 47): the key's colour.
const BLOCK: u8 = 0x7F;
/// The Fastext colours, in the order of a node's choices.
const KEY_COLOURS: [Colour; 4] = [Colour::Red, Colour::Green, Colour::Yellow, Colour::Cyan];
/// The keys of the choices, in their order: each choice's function key,
/// remote colour key (INPUT.md section 5.6) and the engine viewer's letter.
const CHOICE_KEYS: [(&str, &str, char); 4] = [("F1", "ColorF0Red", 'r'), ("F2", "ColorF1Green", 'g'), ("F3", "ColorF2Yellow", 'y'), ("F4", "ColorF3Blue", 'c')];
/// The mouse's left button (INPUT.md section 5.4).
const LEFT_BUTTON: u8 = 0;

const _: () = assert!(CHOICE_ROW + 4 <= HINT_ROW, "four choices fit above the hint");
const _: () = assert!(PICTURE_COLUMN >= 4, "a picture's background and attributes fit before it");

/// The Lost Crown: where each viewer who has chosen is in the story.
#[derive(Default)]
pub struct Crown {
    places: BTreeMap<SessionId, Id>,
}

/// The band's colour: how the node ends, if it does.
const fn band(ending: Ending) -> Colour {
    match ending {
        Ending::Not => Colour::Magenta,
        Ending::Won => Colour::Green,
        Ending::Lost => Colour::Red,
        Ending::Other => Colour::Blue,
    }
}

/// The node `id`: every node's id is its place in [`NODES`].
#[must_use]
pub fn node(id: Id) -> Option<&'static Node> {
    NODES.get(usize::from(id.0))
}

/// The labels of `node`'s choices on row 24. They are prompts, not links:
/// a choice is the page's own, made by the channel.
fn labels(node: &Node) -> [&'static str; 4] {
    let mut labels = [""; 4];
    for (slot, choice) in labels.iter_mut().zip(node.choices) {
        *slot = choice.label;
    }
    labels
}

/// The first row of `node`'s choices.
const fn choice_row(node: &Node) -> usize {
    if matches!(node.ending, Ending::Not) { CHOICE_ROW } else { ENDING_CHOICE_ROW }
}

/// The page of `node`.
#[must_use]
pub fn draw(node: &Node) -> Screen {
    let mut screen = common::page(TITLE, band(node.ending), labels(node).map(|label| (label, None)));
    screen.modes(MOUSE_BUTTONS);
    let title_width = node.title.chars().count().saturating_add(TITLE_ATTRIBUTES);
    screen.double_height(NODE_TITLE_ROW, COLUMNS.saturating_sub(title_width) / 2, Colour::Yellow, node.title);
    node.picture.draw(&mut screen, PICTURE_ROW, PICTURE_COLUMN);
    let block = Block { first_row: TEXT_ROW, rows: TEXT_ROWS, column: TEXT_COLUMN, narrow_rows: 0, narrow_width: 0, width: TEXT_WIDTH };
    text::draw(&mut screen, node.text, block, Style { plain: Colour::White, highlight: Colour::Yellow });

    if node.ending != Ending::Not {
        let (words, colour) = match node.ending {
            Ending::Won => ("You have won", Colour::Green),
            Ending::Lost => ("The End", Colour::Red),
            Ending::Not | Ending::Other => ("The End", Colour::Cyan),
        };
        let width = words.chars().count().saturating_add(TITLE_ATTRIBUTES);
        screen.double_height(THE_END_ROW, COLUMNS.saturating_sub(width) / 2, colour, words);
    }
    let first = choice_row(node);
    for (offset, (choice, colour)) in node.choices.iter().zip(KEY_COLOURS).enumerate() {
        let row = first.saturating_add(offset);
        let marker = [colour.mosaic(), BLOCK, Colour::White.text()];
        screen.codes(row, TEXT_COLUMN, &marker);
        screen.text(row, TEXT_COLUMN.saturating_add(marker.len()), choice.text);
    }
    screen.centred(HINT_ROW, Colour::Cyan, "Choose with the coloured keys");
    screen
}

/// The choice `input` makes, 0 to 3, on `node`: a coloured key, or a
/// press of the left button on a choice's line or its prompt.
fn chosen(node: &Node, input: &Input<'_>) -> Option<usize> {
    let place = match *input {
        Input::Key { key: Named::Other(code), .. } if input.pressed(Named::Other(code)) => {
            let name = keys::name(code)?;
            CHOICE_KEYS.iter().position(|&(function, remote, _)| name == function || name == remote)
        }
        Input::Mouse { pressed: true, button: LEFT_BUTTON, column, row } => {
            let (column, row) = (usize::try_from(column).ok()?, usize::try_from(row).ok()?);
            if row == FASTEXT_ROW {
                common::label_columns(labels(node)).iter().position(|span| span.is_some_and(|(start, end)| (start..end).contains(&column)))
            } else {
                row.checked_sub(choice_row(node))
            }
        }
        _ => {
            let typed = input.character()?.to_ascii_lowercase();
            CHOICE_KEYS.iter().position(|&(_, _, letter)| letter == typed)
        }
    }?;
    (place < node.choices.len()).then_some(place)
}

impl Crown {
    /// Where `session` is in the story.
    fn place(&self, session: SessionId) -> Id {
        self.places.get(&session).copied().unwrap_or(CROSSROADS)
    }
}

impl Application for Crown {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(PAGE, TITLE)]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        // What a viewer sees before they choose.
        if let Some(first) = node(CROSSROADS) {
            context.publish(Owner::Channel, PAGE, &draw(first));
        }
    }

    fn input(&mut self, session: SessionId, _page: u16, input: &Input<'_>, context: &mut Context<'_>) {
        let Some(here) = node(self.place(session)) else { return };
        let Some(choice) = chosen(here, input).and_then(|place| here.choices.get(place)) else { return };
        let next = match choice.to {
            Leads::Node(next) => next,
            // Leaving the story begins it again, as the old pages did:
            // the menu's link led to the first.
            Leads::Page(page) => {
                context.goto(session, page);
                CROSSROADS
            }
        };
        let Some(shown) = node(next) else { return };
        self.places.insert(session, next);
        context.publish(Owner::Session(session), PAGE, &draw(shown));
    }

    fn ended(&mut self, session: SessionId, _context: &mut Context<'_>) {
        self.places.remove(&session);
    }
}
