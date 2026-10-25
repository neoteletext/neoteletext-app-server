//! The cave flyer: an endless cave scrolls past, narrowing and speeding up
//! as it goes; climb and dive to stay off its walls and its rocks, and
//! pick up the gold.
//!
//! The cave is a column of rock above and below for each sixel across.
//! The rock is coloured by its row of cells, in strata: colour changes
//! between rows of cells cost nothing (EN 300 706 clause 12.2 sets colour
//! along a row, never down a column).

use alloc::format;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen};

use super::canvas::Canvas;
use super::look::{self, FIELD_HEIGHT, FIELD_SIXELS_ACROSS, FIELD_WIDTH};
use super::sprites::{self, Sprite};
use super::{Dice, Game, Keys, between, control};
use crate::sound::Effect;

/// The ship's column, and how fast it climbs or dives in sixels a tick.
const SHIP_X: i32 = 10;
const CLIMB: i32 = 2;
/// The cave's opening at the start and at its narrowest, in sixels; it
/// narrows a sixel every 60 columns.
const START_GAP: i32 = 36;
const NARROWEST_GAP: i32 = 15;
const COLUMNS_PER_NARROWING: u32 = 60;
/// Rock above and below is never thinner than this.
const MINIMUM_ROCK: i32 = 1;
/// The cave's middle wanders: a new direction one column in this many.
const WANDER_ONE_IN: u32 = 6;
/// Scrolling, in tenths of a column a tick: one column a tick at first,
/// a tenth more every 150 columns, up to two.
const TENTHS: u32 = 10;
const START_SPEED: u32 = 10;
const COLUMNS_PER_SPEEDUP: u32 = 150;
const TOP_SPEED: u32 = 20;
/// Rocks and gold, which appear in one new column in this many, rocks
/// only once the first 100 columns have given the player a start.
const ROCK_ONE_IN: u32 = 35;
const ROCKS_AFTER: u32 = 100;
const GOLD_ONE_IN: u32 = 25;
const MAX_THINGS: usize = 4;
/// The ship's height, and the size of the rocks and gold (the sprites
/// below).
const SHIP_HEIGHT: i32 = 3;
const THING_WIDTH: i32 = 4;
const THING_HEIGHT: i32 = 3;
/// Rocks leave the ship's height and a sixel clear of the rock above and
/// below, so there is always a way past; gold sits two sixels clear.
const ROCK_CLEARANCE: i32 = SHIP_HEIGHT + 1;
const GOLD_CLEARANCE: i32 = 2;
/// Gold is worth this; each column flown is worth one.
const GOLD_POINTS: u32 = 25;
/// Banners go on these rows, away from the ship.
const LOW_BANNER_ROW: usize = 15;
const HIGH_BANNER_ROW: usize = 5;
/// The strata, by row of cells from the top.
const STRATA: [Colour; 4] = [Colour::Red, Colour::Magenta, Colour::Red, Colour::Yellow];

fn ship() -> Sprite {
    Sprite::drawn("Ship", Colour::Cyan, &["#......", "####...", "#######"])
}

fn rock() -> Sprite {
    Sprite::drawn("Rock", Colour::White, &[".##.", "####", ".##."])
}

fn gold() -> Sprite {
    Sprite::drawn("Gold", Colour::Yellow, &[".#.", "###", ".#."])
}

/// A rock or gold, at its top-left.
type Thing = (i32, i32);

/// A cave flight.
#[derive(Clone, Debug)]
pub struct Cave {
    /// For each sixel column, the first row clear of the rock above and
    /// the first row of the rock below.
    top: [u8; FIELD_SIXELS_ACROSS],
    bottom: [u8; FIELD_SIXELS_ACROSS],
    middle: i32,
    wander: i32,
    ship_y: i32,
    rocks: [Option<Thing>; MAX_THINGS],
    golds: [Option<Thing>; MAX_THINGS],
    distance: u32,
    gold_taken: u32,
    owed: u32,
    over: bool,
}

impl Cave {
    /// A new flight at the cave's wide mouth.
    #[must_use]
    pub fn new(dice: &mut dyn Dice) -> Self {
        let middle = FIELD_HEIGHT.checked_div(2).unwrap_or(0);
        let mut cave = Self {
            top: [0; FIELD_SIXELS_ACROSS],
            bottom: [0; FIELD_SIXELS_ACROSS],
            middle,
            wander: 0,
            ship_y: middle.saturating_sub(1),
            rocks: [None; MAX_THINGS],
            golds: [None; MAX_THINGS],
            distance: 0,
            gold_taken: 0,
            owed: 0,
            over: false,
        };
        for column in 0..FIELD_SIXELS_ACROSS {
            let (top, bottom) = cave.next_column(dice);
            if let (Some(above), Some(below)) = (cave.top.get_mut(column), cave.bottom.get_mut(column)) {
                (*above, *below) = (top, bottom);
            }
        }
        cave
    }

    /// The score: a point a column, and the gold.
    #[must_use]
    pub const fn score(&self) -> u32 {
        self.distance.saturating_add(self.gold_taken.saturating_mul(GOLD_POINTS))
    }

    /// Whether the flight is over.
    #[must_use]
    pub const fn over(&self) -> bool {
        self.over
    }

    /// Where a banner goes: away from the ship.
    #[must_use]
    pub const fn banner_row(&self) -> usize {
        if self.ship_y < FIELD_HEIGHT / 2 { LOW_BANNER_ROW } else { HIGH_BANNER_ROW }
    }

    /// The ship's top row.
    #[cfg(test)]
    pub const fn ship_y(&self) -> i32 {
        self.ship_y
    }

    fn gap(&self) -> i32 {
        let narrowed = i32::try_from(self.distance.checked_div(COLUMNS_PER_NARROWING).unwrap_or(0)).unwrap_or(i32::MAX);
        START_GAP.saturating_sub(narrowed).max(NARROWEST_GAP)
    }

    fn speed(&self) -> u32 {
        START_SPEED.saturating_add(self.distance.checked_div(COLUMNS_PER_SPEEDUP).unwrap_or(0)).min(TOP_SPEED)
    }

    /// The next column of the cave: its middle wanders, kept where the
    /// opening fits.
    fn next_column(&mut self, dice: &mut dyn Dice) -> (u8, u8) {
        if dice.below(WANDER_ONE_IN) == 0 {
            self.wander = between(dice, -1, 1);
        }
        let gap = self.gap();
        let half = gap.checked_div(2).unwrap_or(0);
        let lowest = MINIMUM_ROCK.saturating_add(half);
        let highest = FIELD_HEIGHT.saturating_sub(MINIMUM_ROCK).saturating_sub(gap.saturating_sub(half));
        self.middle = self.middle.saturating_add(self.wander).clamp(lowest, highest);
        if self.middle == lowest || self.middle == highest {
            self.wander = self.wander.saturating_neg();
        }
        let top = self.middle.saturating_sub(half);
        let bottom = top.saturating_add(gap);
        (u8::try_from(top).unwrap_or(0), u8::try_from(bottom).unwrap_or(u8::MAX))
    }

    /// Whether the sixel at `x`, `y` is rock.
    fn rock_at(&self, x: i32, y: i32) -> bool {
        let Ok(column) = usize::try_from(x) else { return false };
        let (Some(&top), Some(&bottom)) = (self.top.get(column), self.bottom.get(column)) else { return false };
        y < i32::from(top) || y >= i32::from(bottom)
    }

    /// One tick: whether the screen changed.
    pub fn tick(&mut self, keys: Keys, dice: &mut dyn Dice, sounds: &mut Vec<Effect>) -> bool {
        if self.over {
            return false;
        }
        if keys.active(control::UP) {
            self.ship_y = self.ship_y.saturating_sub(CLIMB).max(0);
        }
        if keys.active(control::DOWN) {
            self.ship_y = self.ship_y.saturating_add(CLIMB).min(FIELD_HEIGHT.saturating_sub(SHIP_HEIGHT));
        }
        self.owed = self.owed.saturating_add(self.speed());
        while self.owed >= TENTHS {
            self.owed = self.owed.saturating_sub(TENTHS);
            self.scroll(dice);
        }
        self.collide(sounds);
        true
    }

    /// The cave moves a column left, and a new column comes in.
    fn scroll(&mut self, dice: &mut dyn Dice) {
        self.top.rotate_left(1);
        self.bottom.rotate_left(1);
        let (top, bottom) = self.next_column(dice);
        if let (Some(above), Some(below)) = (self.top.last_mut(), self.bottom.last_mut()) {
            (*above, *below) = (top, bottom);
        }
        self.distance = self.distance.saturating_add(1);
        for thing in self.rocks.iter_mut().chain(self.golds.iter_mut()) {
            *thing = thing.and_then(|(x, y)| (x > THING_WIDTH.saturating_neg()).then_some((x.saturating_sub(1), y)));
        }
        let (top, bottom) = (i32::from(top), i32::from(bottom));
        if self.distance > ROCKS_AFTER && dice.below(ROCK_ONE_IN) == 0 {
            place(
                &mut self.rocks,
                (FIELD_WIDTH, between(dice, top.saturating_add(ROCK_CLEARANCE), bottom.saturating_sub(ROCK_CLEARANCE).saturating_sub(THING_HEIGHT))),
            );
        }
        if dice.below(GOLD_ONE_IN) == 0 {
            place(
                &mut self.golds,
                (FIELD_WIDTH, between(dice, top.saturating_add(GOLD_CLEARANCE), bottom.saturating_sub(GOLD_CLEARANCE).saturating_sub(THING_HEIGHT))),
            );
        }
    }

    /// The ship against the rock and the gold.
    fn collide(&mut self, sounds: &mut Vec<Effect>) {
        let ship = ship();
        let lit: Vec<(i32, i32)> = (0..ship.height)
            .flat_map(|y| (0..ship.width).map(move |x| (x, y)))
            .filter(|&(x, y)| ship.lit(x, y))
            .map(|(x, y)| (SHIP_X.saturating_add(look::sixels(x)), self.ship_y.saturating_add(look::sixels(y))))
            .collect();
        let rock = rock();
        let touches = |thing: &Option<Thing>, sprite: &Sprite| {
            thing.is_some_and(|(left, top)| {
                lit.iter().any(|&(x, y)| {
                    sprite.lit(usize::try_from(x.saturating_sub(left)).unwrap_or(usize::MAX), usize::try_from(y.saturating_sub(top)).unwrap_or(usize::MAX))
                })
            })
        };
        let crashed = lit.iter().any(|&(x, y)| self.rock_at(x, y)) || self.rocks.iter().any(|thing| touches(thing, &rock));
        let gold = gold();
        for thing in &mut self.golds {
            if touches(thing, &gold) {
                *thing = None;
                self.gold_taken = self.gold_taken.saturating_add(1);
                sounds.push(Effect::Coin);
            }
        }
        if crashed {
            self.over = true;
            sounds.push(Effect::Explosion);
            sounds.push(Effect::Lose);
        }
    }

    /// Draws the flight.
    pub fn draw(&self, screen: &mut Screen) {
        let taken = format!("GOLD {}", self.gold_taken);
        let speed = format!("SPEED {}", self.speed().saturating_sub(START_SPEED).saturating_add(1));
        look::status(
            screen,
            Game::Cave,
            &[(Colour::Yellow, &format!("SCORE {}", look::digits(self.score()))), (Colour::White, &taken), (Colour::Cyan, &speed)],
        );
        let mut field = look::field();
        strata(&mut field, &self.top, &self.bottom);
        for &(x, y) in self.rocks.iter().flatten() {
            field.sprite(&rock(), x, y, Colour::White);
        }
        for &(x, y) in self.golds.iter().flatten() {
            field.sprite(&gold(), x, y, Colour::Yellow);
        }
        field.sprite(&ship(), SHIP_X, self.ship_y, Colour::Cyan);
        if self.over {
            field.sprite(&sprites::burst(), SHIP_X, self.ship_y.saturating_sub(1), Colour::Yellow);
        }
        field.draw(screen, look::FIELD_ROW, look::FIELD_COLUMN);
        look::hint(screen, "Hold \u{2191} to climb, Down to dive");
    }
}

/// Puts `thing` in a free slot, if there is one.
fn place(things: &mut [Option<Thing>; MAX_THINGS], thing: Thing) {
    if let Some(slot) = things.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(thing);
    }
}

/// The rock above `top` and from `bottom` down, each sixel coloured by its
/// row of cells.
fn strata(field: &mut Canvas, top: &[u8], bottom: &[u8]) {
    let colour = |y: i32| {
        STRATA
            .get(
                usize::try_from(y.checked_div(look::sixels(teletext_runtime::screen::SIXELS_DOWN)).unwrap_or(0))
                    .unwrap_or(0)
                    .checked_rem(STRATA.len())
                    .unwrap_or(0),
            )
            .copied()
            .unwrap_or(Colour::Red)
    };
    for (x, (&above, &below)) in top.iter().zip(bottom).enumerate() {
        let x = look::sixels(x);
        for y in 0..i32::from(above) {
            field.plot(x, y, colour(y));
        }
        for y in i32::from(below)..FIELD_HEIGHT {
            field.plot(x, y, colour(y));
        }
    }
}

/// The title page's picture: the cave with the ship in it.
#[must_use]
pub fn art() -> Canvas {
    let mut art = look::art(look::ART_HEIGHT);
    let (mut top, mut bottom) = ([0_u8; FIELD_SIXELS_ACROSS], [0_u8; FIELD_SIXELS_ACROSS]);
    // A cave that rises and falls, as triangle waves of two lengths and
    // six sixels deep, with the ship, gold and a rock along it: the
    // picture's numbers are its layout, chosen by eye.
    let wave = |x: i32, length: i32, height: i32| {
        let phase = x.checked_rem(length).unwrap_or(0);
        let half = length.checked_div(2).unwrap_or(1);
        (if phase < half { phase } else { length.saturating_sub(phase) }).saturating_mul(height).checked_div(half).unwrap_or(0)
    };
    for (x, (above, below)) in top.iter_mut().zip(bottom.iter_mut()).enumerate() {
        let x = look::sixels(x);
        *above = u8::try_from(wave(x, 26, 6).saturating_add(1)).unwrap_or(0);
        *below = u8::try_from(look::sixels(look::ART_HEIGHT).saturating_sub(wave(x.saturating_add(9), 34, 6)).saturating_sub(1)).unwrap_or(0);
    }
    strata(&mut art, &top, &bottom);
    art.sprite(&ship(), SHIP_X, 10, Colour::Cyan);
    art.sprite(&gold(), 30, 11, Colour::Yellow);
    art.sprite(&rock(), 52, 12, Colour::White);
    art
}
