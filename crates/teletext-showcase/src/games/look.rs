//! The games' screens: the frame every game page shares (header, status
//! bar, playfield, hint and Fastext rows), banners, and the menu and title
//! pages.

use alloc::format;
use alloc::string::String;

use neoteletext_core::consts::COLUMNS;
use neoteletext_input::modes::{ALL_KEYS, KEY_RELEASE};
use teletext_runtime::screen::{Colour, SIXELS_ACROSS, SIXELS_DOWN, SPACE, Screen, attribute};

use super::canvas::Canvas;
use super::sprites::Sprite;
use super::{Game, HALL, MENU};
use crate::common::page;
use crate::index::INDEX;

/// The status bar's row: the title band of other pages.
pub const STATUS_ROW: usize = 1;
/// The playfield's first row and column: rows 2 to 22 and columns 1 to
/// 39, column 0 kept for each row's first colour attribute.
pub const FIELD_ROW: usize = 2;
pub const FIELD_COLUMN: usize = 1;
/// The playfield's rows and columns of cells.
const FIELD_ROWS: usize = 21;
const FIELD_COLUMNS: usize = COLUMNS - FIELD_COLUMN;
/// The playfield's size in sixels: 78 by 63.
pub const FIELD_SIXELS_ACROSS: usize = FIELD_COLUMNS * SIXELS_ACROSS;
pub const FIELD_WIDTH: i32 = sixels(FIELD_SIXELS_ACROSS);
pub const FIELD_HEIGHT: i32 = sixels(FIELD_ROWS * SIXELS_DOWN);

/// A count of sixels from the page's layout as the games' signed
/// coordinates: the page is 80 by 75 sixels, far inside `i32`.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, reason = "the page's sizes are under a hundred")]
pub const fn sixels(count: usize) -> i32 {
    count as i32
}
/// The playfield's middle row, where a banner goes by default.
pub const MIDDLE_ROW: usize = 11;
/// The row under the playfield, for the controls.
const HINT_ROW: usize = 23;
/// The page's Fastext row (EN 300 706 clause 9.6, shown on row 24).
const FASTEXT_ROW: usize = 24;
/// The widest Fastext label: the house style puts the four ten cells
/// apart, a colour attribute before each.
const LABEL_WIDTH: usize = 9;

/// A canvas the size of the playfield.
#[must_use]
pub fn field() -> Canvas {
    Canvas::new(usize::try_from(FIELD_WIDTH).unwrap_or(0), usize::try_from(FIELD_HEIGHT).unwrap_or(0))
}

/// A canvas as wide as the playfield and `height` sixels tall, for a
/// title page's picture.
#[must_use]
pub fn art(height: usize) -> Canvas {
    Canvas::new(usize::try_from(FIELD_WIDTH).unwrap_or(0), height)
}

/// Each game's colour, for its title band and status bar.
#[must_use]
pub const fn band(game: Game) -> Colour {
    match game {
        Game::Invaders => Colour::Blue,
        Game::Snake => Colour::Green,
        Game::Breakout => Colour::Magenta,
        Game::Cave => Colour::Red,
    }
}

/// A game's name as a Fastext label.
fn label(game: Game) -> &'static str {
    let name = game.name();
    if name.len() <= LABEL_WIDTH { name } else { name.split(' ').next().unwrap_or(name) }
}

/// The game after `game`, for the green link.
fn next(game: Game) -> Game {
    let index = Game::ALL.iter().position(|&each| each == game).unwrap_or(0);
    Game::ALL.get(index.saturating_add(1)).copied().unwrap_or(Game::Invaders)
}

/// A game page's frame: the house header, title band and Fastext row,
/// asking for every key and its release, which held arrows need (INPUT.md
/// section 6.3, `ALL_KEYS` and `KEY_RELEASE`). No cursor: nothing is typed.
#[must_use]
pub fn game_page(game: Game) -> Screen {
    let following = next(game);
    let mut screen =
        page(game.name(), band(game), [("Menu", Some(MENU)), (label(following), Some(following.page())), ("Scores", Some(HALL)), ("Index", Some(INDEX))]);
    screen.modes(ALL_KEYS | KEY_RELEASE);
    screen
}

/// A game page in play: the frame with the title band's rows cleared for
/// the status bar and the playfield.
#[must_use]
pub fn play_page(game: Game) -> Screen {
    let mut screen = game_page(game);
    for row in STATUS_ROW..FASTEXT_ROW {
        screen.fill(row, 0, COLUMNS, SPACE);
    }
    screen
}

/// The status bar: the game's band with `items`, each in its colour.
pub fn status(screen: &mut Screen, game: Game, items: &[(Colour, &str)]) {
    screen.background(STATUS_ROW, 0, band(game));
    let mut column = 2;
    for (colour, text) in items {
        screen.coloured(STATUS_ROW, column, *colour, text);
        column = column.saturating_add(text.chars().count()).saturating_add(1);
    }
}

/// The controls, under the playfield.
pub fn hint(screen: &mut Screen, text: &str) {
    screen.fill(HINT_ROW, 0, COLUMNS, SPACE);
    screen.centred(HINT_ROW, Colour::Cyan, text);
}

/// A banner across the playfield at `row`: `title` in double height on a
/// band of `colour` (rows `row` and `row + 1`, EN 300 706 clause 12.3),
/// and `line` on the row under it.
pub fn banner(screen: &mut Screen, row: usize, colour: Colour, title: &str, line: &str) {
    const BANNER_ROWS: usize = 3;
    // The band's colour and New Background take the first two cells.
    const BAND_CELLS: usize = 2;
    for each in row..row.saturating_add(BANNER_ROWS) {
        screen.fill(each, 0, COLUMNS, SPACE);
    }
    screen.background(row, 0, colour);
    // Double Height and the text's colour take a cell each.
    let width = title.chars().count().saturating_add(2);
    let column = COLUMNS.saturating_sub(width).checked_div(2).unwrap_or(0).max(BAND_CELLS);
    screen.code(row, column, attribute::DOUBLE_HEIGHT);
    screen.coloured(row, column.saturating_add(1), Colour::Yellow, title);
    screen.centred(row.saturating_add(BANNER_ROWS.saturating_sub(1)), Colour::White, line);
}

/// A score as six digits, as arcades show them.
#[must_use]
pub fn digits(score: u32) -> String {
    format!("{score:06}")
}

/// The best line of a title page.
fn best_line(best: Option<(u32, String)>) -> String {
    best.map_or_else(|| String::from("No score yet: be the first"), |(score, player)| format!("Best {} by {player}", digits(score)))
}

/// The first row under a page's title band (the house style's band is
/// rows 1 and 2).
const BODY_ROW: usize = 4;
/// The menu: an entry every three rows, its page number at column 1, its
/// name and what it is at column 5.
const MENU_STEP: usize = 3;
const NUMBER_COLUMN: usize = 1;
const NAME_COLUMN: usize = 5;
/// The menu's parade of enemies: five, sixteen sixels apart, on the three
/// rows of cells from row 20.
const PARADE: usize = 5;
const PARADE_STEP: i32 = 16;
const PARADE_ROW: usize = 20;
const PARADE_HEIGHT: usize = 3 * SIXELS_DOWN;
/// Title pages: the picture from row 4, lines of text every two rows from
/// row 14, then how to start and the best score.
const TEXT_ROW: usize = 14;
const TEXT_STEP: usize = 2;
const CONTROLS_ROW: usize = 20;
const START_ROW: usize = 21;
const BEST_ROW: usize = 22;
/// The invaders' title: a portrait every four rows, its name and worth
/// beside it.
const PORTRAIT_COLUMN: usize = 3;
const PORTRAIT_STEP: usize = 4;
const PORTRAIT_TEXT_COLUMN: usize = 13;

/// The menu: each game with its page, and enemies across the foot.
#[must_use]
pub fn menu(enemies: &[Sprite]) -> Screen {
    let mut screen = page(
        "Action games",
        Colour::Red,
        [("Invaders", Some(super::INVADERS)), ("Snake", Some(super::SNAKE)), ("Breakout", Some(super::BREAKOUT)), ("Scores", Some(HALL))],
    );
    let entries: [(u16, &str, &str); 5] = [
        (super::INVADERS, "Space invaders", "twenty sci-fi foes, wave on wave"),
        (super::SNAKE, "Snake", "eat, grow, never bite your tail"),
        (super::BREAKOUT, "Breakout", "a paddle, a ball, five walls"),
        (super::CAVE, "Cave flyer", "an endless cave, ever narrower"),
        (HALL, "Hall of fame", "every player's best, live"),
    ];
    let mut row = BODY_ROW;
    for (number, name, about) in entries {
        screen.coloured(row, NUMBER_COLUMN, Colour::Yellow, &format!("{number:03X}"));
        screen.coloured(row, NAME_COLUMN, Colour::White, name);
        screen.coloured(row.saturating_add(1), NAME_COLUMN, Colour::Cyan, about);
        row = row.saturating_add(MENU_STEP);
    }
    // The enemies across the foot, a cell or more apart so each keeps its
    // own colour, the row of them centred.
    let mut parade = art(PARADE_HEIGHT);
    let shown = enemies.iter().take(PARADE);
    let widest = widest_of(enemies.iter().take(PARADE));
    let span = PARADE_STEP.saturating_mul(sixels(shown.len()).saturating_sub(1)).saturating_add(widest);
    let left = FIELD_WIDTH.saturating_sub(span).checked_div(2).unwrap_or(0);
    for (index, enemy) in enemies.iter().take(PARADE).enumerate() {
        let inset = widest.saturating_sub(sixels(enemy.width)).checked_div(2).unwrap_or(0);
        let x = left.saturating_add(PARADE_STEP.saturating_mul(sixels(index))).saturating_add(inset);
        let y = sixels(PARADE_HEIGHT.saturating_sub(enemy.height)).checked_div(2).unwrap_or(0);
        parade.sprite(enemy, x, y, enemy.colour);
    }
    parade.draw(&mut screen, PARADE_ROW, FIELD_COLUMN);
    screen.centred(HINT_ROW, Colour::Green, "Your game is yours; scores are shared");
    screen
}

/// The widest of `sprites`, in sixels.
fn widest_of<'a>(sprites: impl Iterator<Item = &'a Sprite>) -> i32 {
    sixels(sprites.map(|sprite| sprite.width).max().unwrap_or(0))
}

/// The invaders' title page: the first wave's enemies, large, with their
/// names and points.
#[must_use]
pub fn invaders_title(portraits: &[Sprite], points: &[u32], best: Option<(u32, String)>) -> Screen {
    let mut screen = game_page(Game::Invaders);
    let mut row = BODY_ROW;
    let widest = widest_of(portraits.iter());
    let tallest = portraits.iter().map(|portrait| portrait.height).max().unwrap_or(0);
    for (portrait, worth) in portraits.iter().zip(points) {
        let mut picture = Canvas::new(usize::try_from(widest).unwrap_or(0), tallest);
        let inset = widest.saturating_sub(sixels(portrait.width)).checked_div(2).unwrap_or(0);
        picture.sprite(portrait, inset, 0, portrait.colour);
        picture.draw(&mut screen, row, PORTRAIT_COLUMN);
        screen.coloured(row, PORTRAIT_TEXT_COLUMN, portrait.colour, &portrait.name.to_uppercase());
        screen.coloured(row.saturating_add(1), PORTRAIT_TEXT_COLUMN, Colour::White, &format!("{worth} points"));
        row = row.saturating_add(PORTRAIT_STEP);
    }
    screen.centred(CONTROLS_ROW, Colour::White, "\u{2190} \u{2192} move    Space fires");
    screen.centred(START_ROW, Colour::Green, "Press Space to start");
    screen.centred(BEST_ROW, Colour::Yellow, &best_line(best));
    screen
}

/// A plain title page: `art` drawn from row 4, then `lines`.
#[must_use]
pub fn title(game: Game, art: &Canvas, lines: &[(Colour, &str)], best: Option<(u32, String)>) -> Screen {
    let mut screen = game_page(game);
    art.draw(&mut screen, BODY_ROW, FIELD_COLUMN);
    let mut row = TEXT_ROW;
    for (colour, line) in lines {
        screen.centred(row, *colour, line);
        row = row.saturating_add(TEXT_STEP);
    }
    screen.centred(START_ROW, Colour::Green, "Press Space to start");
    screen.centred(BEST_ROW, Colour::Yellow, &best_line(best));
    screen
}

/// The height of a title page's picture, in sixels: rows 4 to 11.
pub const ART_HEIGHT: usize = 8 * SIXELS_DOWN;
