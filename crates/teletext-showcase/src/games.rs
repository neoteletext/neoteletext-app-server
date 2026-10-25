//! Action games (pages 300 to 399): invaders with the sci-fi sprites,
//! snake, breakout and a cave flyer, each viewer's game its own, and a
//! hall of fame every viewer shares.
//!
//! One application serves them all (decision 7, scale): it keeps a small
//! state for each viewer who is playing, and one timer ticks every game in
//! play at once; with nobody playing it asks for no time at all. The menu,
//! the hall of fame and each game's title page are the channel's, one copy
//! for every viewer; a game in play is its viewer's own page, which
//! overrides the title page for that viewer alone (`SERVER.md` section 2).

use alloc::boxed::Box;
use alloc::vec::Vec;

use teletext_runtime::screen::Screen;
use teletext_runtime::{Application, Context, Named};

use crate::channels::Picture;
use crate::index::Section;
use crate::sound::Effect;

mod arcade;
mod breakout;
mod canvas;
mod cave;
mod hall;
mod invaders;
mod look;
mod snake;
pub mod sprites;

#[cfg(test)]
mod tests;

pub use arcade::Arcade;

/// The games' menu.
pub const MENU: u16 = 0x300;
/// Space invaders.
pub const INVADERS: u16 = 0x301;
/// Snake.
pub const SNAKE: u16 = 0x302;
/// Breakout.
pub const BREAKOUT: u16 = 0x303;
/// The cave flyer.
pub const CAVE: u16 = 0x304;
/// The hall of fame, shared by every viewer.
pub const HALL: u16 = 0x305;

/// This area's applications.
#[must_use]
pub fn applications(pictures: &[Picture]) -> Vec<Box<dyn Application>> {
    alloc::vec![Box::new(Arcade::new(pictures))]
}

/// This area's entries in the index.
#[must_use]
pub fn sections() -> Vec<Section> {
    alloc::vec![Section { page: MENU, name: "Action games", about: "invaders, snake, breakout, cave" }]
}

/// A game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Game {
    /// Space invaders with the sci-fi sprites.
    Invaders,
    /// Snake.
    Snake,
    /// Breakout.
    Breakout,
    /// The cave flyer.
    Cave,
}

impl Game {
    /// Every game, in page order.
    pub const ALL: [Self; 4] = [Self::Invaders, Self::Snake, Self::Breakout, Self::Cave];

    /// Its page.
    #[must_use]
    pub const fn page(self) -> u16 {
        match self {
            Self::Invaders => INVADERS,
            Self::Snake => SNAKE,
            Self::Breakout => BREAKOUT,
            Self::Cave => CAVE,
        }
    }

    /// The game on `page`, if one is.
    #[must_use]
    pub fn of_page(page: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|game| game.page() == page)
    }

    /// Its name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Invaders => "Invaders",
            Self::Snake => "Snake",
            Self::Breakout => "Breakout",
            Self::Cave => "Cave flyer",
        }
    }

    /// Its bit in a set of games.
    const fn bit(self) -> u8 {
        match self {
            Self::Invaders => 1,
            Self::Snake => 2,
            Self::Breakout => 4,
            Self::Cave => 8,
        }
    }
}

/// The controls a game reads, as bits.
pub mod control {
    /// Left.
    pub const LEFT: u8 = 1;
    /// Right.
    pub const RIGHT: u8 = 2;
    /// Up.
    pub const UP: u8 = 4;
    /// Down.
    pub const DOWN: u8 = 8;
    /// Fire, launch or thrust.
    pub const FIRE: u8 = 16;
}

/// The control a key works, if any: the arrows, and Space or Enter to
/// fire.
const fn control_of(key: Named) -> Option<u8> {
    match key {
        Named::Left => Some(control::LEFT),
        Named::Right => Some(control::RIGHT),
        Named::Up => Some(control::UP),
        Named::Down => Some(control::DOWN),
        Named::Enter | Named::Character(' ') => Some(control::FIRE),
        _ => None,
    }
}

/// The controls held, and those pressed since the last tick: a tap
/// shorter than a tick still moves a step.
#[derive(Clone, Copy, Default, Debug)]
pub struct Keys {
    held: u8,
    tapped: u8,
}

impl Keys {
    /// `control` went down.
    pub const fn press(&mut self, control: u8) {
        self.held |= control;
        self.tapped |= control;
    }

    /// `control` went down and up again before the next tick.
    pub const fn tap(&mut self, control: u8) {
        self.tapped |= control;
    }

    /// `control` came up.
    pub const fn release(&mut self, control: u8) {
        self.held &= !control;
    }

    /// Everything came up: the receiver lost the focus, and will not say
    /// when the keys held come up (INPUT.md section 5.4, `FOCUS_OUT`).
    pub const fn release_all(&mut self) {
        self.held = 0;
    }

    /// Whether `control` is held or was pressed since the last tick.
    #[must_use]
    pub const fn active(self, control: u8) -> bool {
        (self.held | self.tapped) & control != 0
    }

    /// Whether `control` was pressed since the last tick.
    #[must_use]
    pub const fn tapped(self, control: u8) -> bool {
        self.tapped & control != 0
    }

    /// A tick has read the taps.
    const fn end_tick(&mut self) {
        self.tapped = 0;
    }
}

/// Randomness for the games: the channel's, or a test's.
pub trait Dice {
    /// A number below `bound`; 0 when `bound` is 0.
    fn below(&mut self, bound: u32) -> u32;
}

impl Dice for Context<'_> {
    fn below(&mut self, bound: u32) -> u32 {
        Context::below(self, bound)
    }
}

/// A number from `low` to `high`, both included.
fn between(dice: &mut dyn Dice, low: i32, high: i32) -> i32 {
    let span = u32::try_from(high.saturating_sub(low).saturating_add(1)).unwrap_or(1);
    low.saturating_add(i32::try_from(dice.below(span)).unwrap_or(0))
}

/// A game in play: each game's state, boxed so a viewer's entry is as
/// small as the game it plays.
enum Running {
    Invaders(Box<invaders::Invaders>),
    Snake(Box<snake::Snake>),
    Breakout(Box<breakout::Breakout>),
    Cave(Box<cave::Cave>),
}

impl Running {
    fn new(game: Game, dice: &mut dyn Dice) -> Self {
        match game {
            Game::Invaders => Self::Invaders(Box::default()),
            Game::Snake => Self::Snake(Box::new(snake::Snake::new(dice))),
            Game::Breakout => Self::Breakout(Box::default()),
            Game::Cave => Self::Cave(Box::new(cave::Cave::new(dice))),
        }
    }

    const fn game(&self) -> Game {
        match self {
            Self::Invaders(_) => Game::Invaders,
            Self::Snake(_) => Game::Snake,
            Self::Breakout(_) => Game::Breakout,
            Self::Cave(_) => Game::Cave,
        }
    }

    /// A control went down, for games that act on the press itself.
    fn press(&mut self, control: u8) {
        if let Self::Snake(snake) = self {
            snake.turn(control);
        }
    }

    /// One tick: whether the screen changed.
    fn tick(&mut self, keys: Keys, dice: &mut dyn Dice, sounds: &mut Vec<Effect>) -> bool {
        match self {
            Self::Invaders(game) => game.tick(keys, dice, sounds),
            Self::Snake(game) => game.tick(dice, sounds),
            Self::Breakout(game) => game.tick(keys, sounds),
            Self::Cave(game) => game.tick(keys, dice, sounds),
        }
    }

    fn draw(&self, screen: &mut Screen, enemies: &[sprites::Sprite]) {
        match self {
            Self::Invaders(game) => game.draw(screen, enemies),
            Self::Snake(game) => game.draw(screen),
            Self::Breakout(game) => game.draw(screen),
            Self::Cave(game) => game.draw(screen),
        }
    }

    /// Where a banner over the game goes: clear of what matters most.
    const fn banner_row(&self) -> usize {
        match self {
            Self::Invaders(_) => invaders::BANNER_ROW,
            Self::Snake(_) | Self::Breakout(_) => look::MIDDLE_ROW,
            Self::Cave(game) => game.banner_row(),
        }
    }

    fn score(&self) -> u32 {
        match self {
            Self::Invaders(game) => game.score(),
            Self::Snake(game) => game.score(),
            Self::Breakout(game) => game.score(),
            Self::Cave(game) => game.score(),
        }
    }

    fn over(&self) -> bool {
        match self {
            Self::Invaders(game) => game.over(),
            Self::Snake(game) => game.over(),
            Self::Breakout(game) => game.over(),
            Self::Cave(game) => game.over(),
        }
    }
}
