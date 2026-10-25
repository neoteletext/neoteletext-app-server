//! Page 401, Station Zero: a typed text adventure on an abandoned space
//! station, each viewer's game their own (`SERVER.md` section 2: the
//! page is the session's). A command line near the foot of the page has
//! an echoing cursor (INPUT.md section 7.1); Enter acts on what was
//! typed, and the page is published again with the reply.

mod game;
pub mod pictures;
pub mod world;

use alloc::collections::BTreeMap;
use alloc::collections::btree_map::Entry;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::COLUMNS;
use neoteletext_input::cursor::ECHO;
use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Application, Context, Input, Named, Owner, PageEntry, SessionId};

pub use game::{Event, Game, MAX_COMMAND, Place, Tone};

use super::text::{self, Block, Style, TEXT_COLUMN};
use crate::common;
use crate::index::INDEX;
use crate::sound::{Effect, Sounds};

/// The game's page.
pub const PAGE: u16 = 0x401;
/// Its title.
const TITLE: &str = "Station Zero";
/// The title band's colour.
const BAND: Colour = Colour::Blue;
/// Where the room's name starts in the title band, after the title.
const ROOM_NAME_COLUMN: usize = 18;
/// The first row of the description, and of the picture beside it.
const DESCRIPTION_ROW: usize = 4;
/// Rows the description may take.
pub const DESCRIPTION_ROWS: usize = 10;
/// The picture's first column: it runs to the page's right edge.
const PICTURE_COLUMN: usize = 26;
/// The description's width beside the picture: up to the picture's two
/// attribute cells, less a gap.
pub const NARROW_WIDTH: usize = 22;
/// The exits' row.
pub const EXITS_ROW: usize = 14;
/// The row listing what lies here.
pub const ITEMS_ROW: usize = 15;
/// The row repeating the last command.
pub const SAID_ROW: usize = 17;
/// The reply's first row.
const REPLY_ROW: usize = 18;
/// Rows the reply may take.
pub const REPLY_ROWS: usize = 4;
/// The command line's row.
pub const COMMAND_ROW: usize = 22;
/// The command line's prompt.
const PROMPT: &str = ">";
/// Where the prompt goes: after the band's colour and New Background,
/// and the prompt's own colour.
const PROMPT_COLUMN: usize = 3;
/// Where typed text starts on the command line: after the prompt and the
/// text's colour.
pub const COMMAND_COLUMN: usize = PROMPT_COLUMN + PROMPT.len() + 1;
/// The label of the exits.
const EXITS_LABEL: &str = "Exits:";
/// The label of what lies here.
const ITEMS_LABEL: &str = "You see:";
/// The status row.
pub const STATUS_ROW: usize = 23;
/// Where the move count starts on the status row.
const MOVES_COLUMN: usize = 28;
/// The width of text across the page: after the colour attribute in
/// column 0, less a cell's margin at the right.
pub const TEXT_WIDTH: usize = COLUMNS - 2;

const _: () = assert!(COMMAND_COLUMN + MAX_COMMAND < COLUMNS, "the command line holds the longest command and the cursor after it");
const _: () = assert!(DESCRIPTION_ROW + DESCRIPTION_ROWS <= EXITS_ROW, "the description ends above the exits");
const _: () = assert!(REPLY_ROW + REPLY_ROWS <= COMMAND_ROW, "the reply ends above the command line");
const _: () = assert!(NARROW_WIDTH + 1 < PICTURE_COLUMN - 2, "the description stops short of the picture's attributes");

/// The page's Fastext links.
const FASTEXT: [(&str, Option<u16>); 4] = [("Menu", Some(super::MENU)), ("Crown", Some(super::crown::PAGE)), ("", None), ("Index", Some(INDEX))];

/// One viewer: their game and the command they are typing.
struct Player {
    game: Game,
    typed: String,
}

/// Station Zero: every viewer's game, and their sounds.
pub struct Station {
    players: BTreeMap<SessionId, Player>,
    sounds: Sounds,
}

impl Default for Station {
    fn default() -> Self {
        Self::new()
    }
}

impl Station {
    /// No games yet.
    #[must_use]
    pub fn new() -> Self {
        Self { players: BTreeMap::new(), sounds: Sounds::new() }
    }

    /// The game of `session`, if it has one.
    #[must_use]
    pub fn game(&self, session: SessionId) -> Option<&Game> {
        self.players.get(&session).map(|player| &player.game)
    }

    fn publish(&self, session: SessionId, context: &mut Context<'_>) {
        if let Some(player) = self.players.get(&session) {
            context.publish(Owner::Session(session), PAGE, &draw(&player.game, Some(&player.typed)));
        }
    }

    /// The sound of what a command did.
    const fn effect(event: Event) -> Option<Effect> {
        match event {
            Event::Quiet => None,
            Event::Moved => Some(Effect::Click),
            Event::Took => Some(Effect::Coin),
            Event::Dropped | Event::Refused => Some(Effect::Hit),
            Event::Solved => Some(Effect::Bell),
            Event::Hurt => Some(Effect::Lose),
            Event::Won => Some(Effect::Win),
            Event::Restarted => Some(Effect::Jump),
        }
    }
}

/// Appends typed `text` to the command line, as far as it holds; a line
/// break in a paste ends the command. Answers whether a command ended.
fn type_into(typed: &mut String, text: &str) -> bool {
    for character in text.chars() {
        if character == '\n' || character == '\r' {
            return true;
        }
        if !character.is_control() && typed.chars().count() < MAX_COMMAND {
            typed.push(character);
        }
    }
    false
}

impl Application for Station {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(PAGE, TITLE)]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        // What a viewer sees before they type: a new game, without the
        // cursor, which is their own page's.
        context.publish(Owner::Channel, PAGE, &draw(&Game::new(), None));
    }

    fn opened(&mut self, session: SessionId, _page: u16, context: &mut Context<'_>) {
        if let Entry::Vacant(slot) = self.players.entry(session) {
            slot.insert(Player { game: Game::new(), typed: String::new() });
            self.publish(session, context);
        }
    }

    fn input(&mut self, session: SessionId, _page: u16, input: &Input<'_>, context: &mut Context<'_>) {
        let player = self.players.entry(session).or_insert_with(|| Player { game: Game::new(), typed: String::new() });
        let mut event = None;
        match *input {
            Input::Text(text) | Input::Paste(text) => {
                if type_into(&mut player.typed, text) {
                    event = Some(player.game.command(&player.typed));
                    player.typed.clear();
                }
            }
            _ if input.pressed(Named::Enter) => {
                event = Some(player.game.command(&player.typed));
                player.typed.clear();
            }
            _ if input.pressed(Named::Backspace) => {
                player.typed.pop();
            }
            _ if input.pressed(Named::Escape) => player.typed.clear(),
            _ => return,
        }
        self.publish(session, context);
        if let Some(effect) = event.and_then(Self::effect) {
            self.sounds.play(session, effect, context);
        }
    }

    fn ended(&mut self, session: SessionId, _context: &mut Context<'_>) {
        self.players.remove(&session);
        self.sounds.forget(session);
    }
}

/// The column after a label written from column 0: past its colour
/// attribute and its text, where the next colour goes.
const fn after(label: &str) -> usize {
    TEXT_COLUMN.saturating_add(label.len())
}

/// A list of names, joined.
fn listed(names: &[&str]) -> String {
    names.join(", ")
}

/// The page of `game`, with the command line holding `typed` and the
/// cursor after it; with no `typed`, no cursor.
#[must_use]
pub fn draw(game: &Game, typed: Option<&str>) -> Screen {
    let about = world::about(game.room());
    let mut screen = common::page(TITLE, BAND, FASTEXT);
    screen.coloured(1, ROOM_NAME_COLUMN, Colour::Yellow, if game.won() { "Homeward" } else { about.name });

    let section = if game.won() { world::Section::Docking } else { about.section };
    pictures::of(section).draw(&mut screen, DESCRIPTION_ROW, PICTURE_COLUMN);
    let description = if game.won() { String::from(game.ending()) } else { game.description() };
    let block = Block {
        first_row: DESCRIPTION_ROW,
        rows: DESCRIPTION_ROWS,
        column: TEXT_COLUMN,
        narrow_rows: pictures::ROWS,
        narrow_width: NARROW_WIDTH,
        width: TEXT_WIDTH,
    };
    text::draw(&mut screen, &description, block, Style { plain: Colour::White, highlight: Colour::Yellow });

    if game.won() {
        screen.coloured(EXITS_ROW, 0, Colour::Green, &alloc::format!("Mission complete in {} moves", game.moves()));
    } else {
        let exits: Vec<&str> = game.exits().iter().map(|direction| direction.name()).collect();
        screen.coloured(EXITS_ROW, 0, Colour::Cyan, EXITS_LABEL);
        screen.coloured(EXITS_ROW, after(EXITS_LABEL), Colour::White, &listed(&exits));
        let here: Vec<&str> = game.here().iter().map(|item| item.name()).collect();
        if !here.is_empty() {
            screen.coloured(ITEMS_ROW, 0, Colour::Cyan, ITEMS_LABEL);
            screen.coloured(ITEMS_ROW, after(ITEMS_LABEL), Colour::Green, &listed(&here));
        }
    }

    if !game.said().is_empty() {
        screen.coloured(SAID_ROW, 0, Colour::Cyan, &alloc::format!("> {}", game.said()));
    }
    let plain = match game.tone() {
        Tone::Plain => Colour::White,
        Tone::Good => Colour::Green,
        Tone::Bad => Colour::Red,
    };
    let reply = Block { first_row: REPLY_ROW, rows: REPLY_ROWS, column: TEXT_COLUMN, narrow_rows: 0, narrow_width: 0, width: TEXT_WIDTH };
    text::draw(&mut screen, game.reply(), reply, Style { plain, highlight: Colour::Yellow });

    screen.background(COMMAND_ROW, 0, Colour::Blue);
    screen.code(COMMAND_ROW, PROMPT_COLUMN.saturating_sub(1), Colour::Yellow.text());
    screen.text(COMMAND_ROW, PROMPT_COLUMN, PROMPT);
    screen.code(COMMAND_ROW, COMMAND_COLUMN.saturating_sub(1), Colour::White.text());
    if let Some(typed) = typed {
        screen.text(COMMAND_ROW, COMMAND_COLUMN, typed);
        let column = COMMAND_COLUMN.saturating_add(typed.chars().count());
        let width = COLUMNS.saturating_sub(column);
        if let (Ok(row), Ok(column), Ok(width)) = (u8::try_from(COMMAND_ROW), u8::try_from(column), u8::try_from(width)) {
            screen.cursor(row, column, width, ECHO);
        }
    } else {
        screen.coloured(COMMAND_ROW, COMMAND_COLUMN.saturating_sub(1), Colour::Cyan, "type a command, then Enter");
    }

    screen.coloured(STATUS_ROW, 0, Colour::Magenta, "HELP lists the commands");
    screen.coloured(STATUS_ROW, MOVES_COLUMN, Colour::White, &alloc::format!("Moves {}", game.moves()));
    screen
}
