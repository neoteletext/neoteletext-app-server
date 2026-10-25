//! Snake: steer with the arrows, eat to grow, and never bite yourself.
//!
//! The snake lives on a grid of two-by-two sixel squares, which join into
//! a solid body in contiguous mosaics. A blank column of cells keeps the
//! grid off the side walls, so the snake and the food keep their own
//! colours there (see `canvas`).

use alloc::collections::VecDeque;
use alloc::format;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen};

use super::canvas::Canvas;
use super::look::{self, FIELD_WIDTH};
use super::{Dice, Game, control};
use crate::sound::Effect;

/// A grid square's side, in sixels.
const SQUARE: i32 = 2;
/// The grid: 35 squares across from sixel 4, 27 down from sixel 3, inside
/// walls on the cells around it. The walls keep rows and columns of cells
/// of their own, so the snake and its food keep their colours beside them.
const GRID_LEFT: i32 = 4;
const GRID_TOP: i32 = 3;
const GRID_COLUMNS: i32 = 35;
const GRID_ROWS: i32 = 27;
/// The walls: a sixel line in the outermost cells.
const WALL_LEFT: i32 = 1;
const WALL_RIGHT: i32 = FIELD_WIDTH - 2;
const WALL_TOP: i32 = 2;
const WALL_BOTTOM: i32 = GRID_TOP + GRID_ROWS * SQUARE;
/// The snake at the start, and how much each meal adds.
const START_LENGTH: usize = 4;
const GROWTH: u32 = 3;
/// Speed in steps a tick, in tenths: five steps a second at first, one
/// tenth faster every three meals, up to a step every tick.
const TENTHS: u32 = 10;
const START_SPEED: u32 = 5;
const MEALS_PER_SPEED: u32 = 3;
const TOP_SPEED: u32 = 10;
/// Every seventh meal brings a bonus, for 60 ticks, worth 50 points.
const BONUS_EVERY: u32 = 7;
const BONUS_TICKS: u32 = 60;
const BONUS_POINTS: u32 = 50;
/// A meal is worth this, times the speed.
const MEAL_POINTS: u32 = 2;
/// Turns pressed faster than the snake steps wait their turn, up to two.
const TURNS_QUEUED: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Heading {
    Up,
    Down,
    Left,
    Right,
}

impl Heading {
    const fn step(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }

    const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    const fn of(control: u8) -> Option<Self> {
        match control {
            control::UP => Some(Self::Up),
            control::DOWN => Some(Self::Down),
            control::LEFT => Some(Self::Left),
            control::RIGHT => Some(Self::Right),
            _ => None,
        }
    }
}

/// A grid square.
type Square = (i32, i32);
/// Off the grid: the food's place before it is first put down.
const NOWHERE: Square = (-1, -1);

/// A game of snake.
#[derive(Clone, Debug)]
pub struct Snake {
    /// Head first.
    body: VecDeque<Square>,
    heading: Heading,
    turns: Vec<Heading>,
    growing: u32,
    food: Square,
    bonus: Option<(Square, u32)>,
    meals: u32,
    score: u32,
    /// Steps owed, in tenths.
    owed: u32,
    over: bool,
}

impl Snake {
    /// A new game: a short snake in the middle, heading right.
    #[must_use]
    pub fn new(dice: &mut dyn Dice) -> Self {
        let (middle_x, middle_y) = (GRID_COLUMNS.checked_div(2).unwrap_or(0), GRID_ROWS.checked_div(2).unwrap_or(0));
        let body = (0..START_LENGTH).map(|back| (middle_x.saturating_sub(i32::try_from(back).unwrap_or(0)), middle_y)).collect();
        let mut snake =
            Self { body, heading: Heading::Right, turns: Vec::new(), growing: 0, food: NOWHERE, bonus: None, meals: 0, score: 0, owed: 0, over: false };
        snake.food = snake.free_square(dice);
        snake
    }

    /// The score.
    #[must_use]
    pub const fn score(&self) -> u32 {
        self.score
    }

    /// Whether the game is over.
    #[must_use]
    pub const fn over(&self) -> bool {
        self.over
    }

    /// The snake's length.
    #[must_use]
    pub fn length(&self) -> usize {
        self.body.len()
    }

    /// The head's square.
    #[cfg(test)]
    pub fn head(&self) -> (i32, i32) {
        self.body.front().copied().unwrap_or((0, 0))
    }

    fn speed(&self) -> u32 {
        START_SPEED.saturating_add(self.meals.checked_div(MEALS_PER_SPEED).unwrap_or(0)).min(TOP_SPEED)
    }

    /// An arrow was pressed: the snake turns at its next step, or after
    /// the turn already waiting; never straight back on itself.
    pub fn turn(&mut self, control: u8) {
        let Some(heading) = Heading::of(control) else { return };
        let last = self.turns.last().copied().unwrap_or(self.heading);
        if heading != last && heading != last.opposite() && self.turns.len() < TURNS_QUEUED {
            self.turns.push(heading);
        }
    }

    /// A square no part of the snake or food is on.
    fn free_square(&self, dice: &mut dyn Dice) -> Square {
        let squares = u32::try_from(GRID_COLUMNS.saturating_mul(GRID_ROWS)).unwrap_or(1);
        let start = dice.below(squares);
        // From a random square, the first free one: always ends, as the
        // snake can never fill the grid before it bites itself.
        (0..squares)
            .map(|offset| {
                let index = i32::try_from(start.saturating_add(offset).checked_rem(squares).unwrap_or(0)).unwrap_or(0);
                (index.checked_rem(GRID_COLUMNS).unwrap_or(0), index.checked_div(GRID_COLUMNS).unwrap_or(0))
            })
            .find(|square| !self.body.contains(square) && *square != self.food && self.bonus.is_none_or(|(bonus, _)| bonus != *square))
            .unwrap_or((0, 0))
    }

    /// One tick: whether the screen changed.
    pub fn tick(&mut self, dice: &mut dyn Dice, sounds: &mut Vec<Effect>) -> bool {
        if self.over {
            return false;
        }
        let mut changed = false;
        if let Some((square, ticks)) = self.bonus {
            self.bonus = (ticks > 1).then(|| (square, ticks.saturating_sub(1)));
            changed |= self.bonus.is_none();
        }
        self.owed = self.owed.saturating_add(self.speed());
        while self.owed >= TENTHS && !self.over {
            self.owed = self.owed.saturating_sub(TENTHS);
            self.step(dice, sounds);
            changed = true;
        }
        changed
    }

    fn step(&mut self, dice: &mut dyn Dice, sounds: &mut Vec<Effect>) {
        if !self.turns.is_empty() {
            self.heading = self.turns.remove(0);
        }
        let (dx, dy) = self.heading.step();
        let Some(&(x, y)) = self.body.front() else { return };
        let head = (x.saturating_add(dx), y.saturating_add(dy));
        // The tail moves on as the head moves, unless the snake is
        // growing: the head may take the square the tail leaves.
        let tail_moves = self.growing == 0;
        let body = self.body.len().saturating_sub(usize::from(tail_moves));
        let outside = !(0..GRID_COLUMNS).contains(&head.0) || !(0..GRID_ROWS).contains(&head.1);
        if outside || self.body.iter().take(body).any(|&square| square == head) {
            self.over = true;
            sounds.push(Effect::Explosion);
            sounds.push(Effect::Lose);
            return;
        }
        if tail_moves {
            self.body.pop_back();
        } else {
            self.growing = self.growing.saturating_sub(1);
        }
        self.body.push_front(head);
        if head == self.food {
            self.meals = self.meals.saturating_add(1);
            self.growing = self.growing.saturating_add(GROWTH);
            self.score = self.score.saturating_add(MEAL_POINTS.saturating_mul(self.speed()));
            self.food = self.free_square(dice);
            sounds.push(Effect::Coin);
            if self.meals.checked_rem(BONUS_EVERY) == Some(0) {
                self.bonus = Some((self.free_square(dice), BONUS_TICKS));
            }
        } else if self.bonus.is_some_and(|(bonus, _)| bonus == head) {
            self.bonus = None;
            self.score = self.score.saturating_add(BONUS_POINTS);
            sounds.push(Effect::Jump);
        }
    }

    /// Draws the game.
    pub fn draw(&self, screen: &mut Screen) {
        let length = format!("LENGTH {}", self.length());
        let speed = format!("SPEED {}", self.speed().saturating_sub(START_SPEED).saturating_add(1));
        look::status(
            screen,
            Game::Snake,
            &[(Colour::Yellow, &format!("SCORE {}", look::digits(self.score))), (Colour::White, &length), (Colour::Cyan, &speed)],
        );
        let mut field = look::field();
        walls(&mut field);
        for &square in &self.body {
            fill(&mut field, square, Colour::Green);
        }
        fill(&mut field, self.food, Colour::Red);
        if let Some((square, _)) = self.bonus {
            fill(&mut field, square, Colour::Yellow);
        }
        field.draw(screen, look::FIELD_ROW, look::FIELD_COLUMN);
        look::hint(screen, "Arrows steer   Esc menu");
    }
}

/// The walls round the grid.
fn walls(field: &mut Canvas) {
    let height = WALL_BOTTOM.saturating_sub(WALL_TOP).saturating_add(1);
    let width = WALL_RIGHT.saturating_sub(WALL_LEFT).saturating_add(1);
    field.rectangle(WALL_LEFT, WALL_TOP, width, 1, Colour::Blue);
    field.rectangle(WALL_LEFT, WALL_BOTTOM, width, 1, Colour::Blue);
    field.rectangle(WALL_LEFT, WALL_TOP, 1, height, Colour::Blue);
    field.rectangle(WALL_RIGHT, WALL_TOP, 1, height, Colour::Blue);
}

/// Lights a grid square.
fn fill(field: &mut Canvas, (column, row): Square, colour: Colour) {
    field.rectangle(GRID_LEFT.saturating_add(column.saturating_mul(SQUARE)), GRID_TOP.saturating_add(row.saturating_mul(SQUARE)), SQUARE, SQUARE, colour);
}

/// The title page's picture: a snake winding towards its food.
#[must_use]
pub fn art() -> Canvas {
    let mut art = look::art(look::ART_HEIGHT);
    // A wave of squares, and the food ahead of it: the picture's layout,
    // chosen by eye.
    let path: [Square; 32] = [
        (4, 8),
        (5, 8),
        (6, 8),
        (7, 8),
        (8, 8),
        (8, 7),
        (8, 6),
        (8, 5),
        (9, 5),
        (10, 5),
        (11, 5),
        (12, 5),
        (12, 6),
        (12, 7),
        (12, 8),
        (13, 8),
        (14, 8),
        (15, 8),
        (16, 8),
        (16, 7),
        (16, 6),
        (16, 5),
        (17, 5),
        (18, 5),
        (19, 5),
        (20, 5),
        (20, 6),
        (20, 7),
        (20, 8),
        (21, 8),
        (22, 8),
        (23, 8),
    ];
    for (column, row) in path {
        art.rectangle(column.saturating_mul(SQUARE).saturating_add(GRID_LEFT), row.saturating_mul(SQUARE), SQUARE, SQUARE, Colour::Green);
    }
    art.rectangle(27_i32.saturating_mul(SQUARE).saturating_add(GRID_LEFT), 8_i32.saturating_mul(SQUARE), SQUARE, SQUARE, Colour::Red);
    art
}
