//! Breakout: five walls of coloured bricks, a ball and a paddle held left
//! and right.
//!
//! Each row of bricks fills one row of cells, so each has its own colour
//! (EN 300 706 clause 12.2: one colour attribute at the row's start). The
//! ball moves in sixteenths of a sixel, in steps of at most half a sixel,
//! so it cannot pass through a brick between frames.

use alloc::format;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, SIXELS_DOWN, Screen};

use super::canvas::Canvas;
use super::look::{self, FIELD_HEIGHT, FIELD_WIDTH};
use super::{Game, Keys, control};
use crate::sound::Effect;

/// Bricks: thirteen across in slots of six sixels, five lit and one gap,
/// on five rows of cells, two sixels lit and one gap.
const BRICK_COLUMNS: usize = 13;
const BRICK_ROWS: usize = 5;
const SLOT_WIDTH: i32 = 6;
const BRICK_WIDTH: i32 = 5;
const SLOT_HEIGHT: i32 = look::sixels(SIXELS_DOWN);
const BRICK_HEIGHT: i32 = 2;
/// The wall's first row: the fourth row of cells, leaving room above to
/// bounce the ball behind the wall.
const BRICK_TOP: i32 = 3 * SLOT_HEIGHT;
/// Each row's colour and worth, top row first.
const ROW_COLOURS: [Colour; BRICK_ROWS] = [Colour::Red, Colour::Magenta, Colour::Yellow, Colour::Green, Colour::Cyan];
const ROW_POINTS: [u32; BRICK_ROWS] = [50, 40, 30, 20, 10];
/// A full row of bricks, a bit each.
const FULL_ROW: u16 = (1 << BRICK_COLUMNS) - 1;
/// The paddle, on the last row of cells.
const PADDLE_Y: i32 = FIELD_HEIGHT - SLOT_HEIGHT;
const PADDLE_WIDTH: i32 = 12;
const PADDLE_HEIGHT: i32 = 2;
const PADDLE_SPEED: i32 = 3;
/// The ball: two sixels square, its place in sixteenths of a sixel.
const BALL: i32 = 2;
const FIXED: i32 = 16;
/// The longest step the ball takes, so it meets every brick it crosses.
const HALF_SIXEL: i32 = FIXED / 2;
/// The ball's speed down the screen, in sixteenths of a sixel a tick:
/// eighteen at first, three more each wall, one more each eight paddle
/// hits, forty at most.
const START_SPEED: i32 = 18;
const WALL_SPEEDUP: i32 = 3;
const HITS_PER_SPEEDUP: u32 = 8;
const TOP_SPEED: i32 = 40;
/// How far the ball's direction swings with where it meets the paddle:
/// across speed is the speed times the distance from the paddle's middle
/// over this.
const SWING: i32 = 8;
/// A ball left on the paddle launches itself after this many ticks.
const AUTO_LAUNCH_TICKS: u32 = 40;
/// Lives at the start.
const LIVES: u8 = 3;

/// A game of breakout.
#[derive(Clone, Debug)]
pub struct Breakout {
    bricks: [u16; BRICK_ROWS],
    paddle_x: i32,
    /// The ball, in sixteenths of a sixel, and its speed.
    ball: (i32, i32),
    velocity: (i32, i32),
    /// Ticks the ball has sat on the paddle; `None` in flight.
    held: Option<u32>,
    speed: i32,
    hits: u32,
    wall: u32,
    lives: u8,
    score: u32,
    over: bool,
}

/// `bits` with bit `column` cleared.
fn cleared(bits: u16, column: i32) -> u16 {
    u32::try_from(column).ok().and_then(|column| 1_u16.checked_shl(column)).map_or(bits, |bit| bits & !bit)
}

/// Whether bit `column` of `bits` is set.
fn has(bits: u16, column: i32) -> bool {
    u32::try_from(column).ok().and_then(|column| bits.checked_shr(column)).is_some_and(|rest| rest & 1 == 1)
}

impl Breakout {
    /// A new game: a full wall and the ball on the paddle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bricks: [FULL_ROW; BRICK_ROWS],
            paddle_x: FIELD_WIDTH.saturating_sub(PADDLE_WIDTH).checked_div(2).unwrap_or(0),
            ball: (0, 0),
            velocity: (0, 0),
            held: Some(0),
            speed: START_SPEED,
            hits: 0,
            wall: 0,
            lives: LIVES,
            score: 0,
            over: false,
        }
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

    /// The paddle's left edge.
    #[cfg(test)]
    pub const fn paddle_x(&self) -> i32 {
        self.paddle_x
    }

    /// Bricks left.
    #[cfg(test)]
    pub fn bricks_left(&self) -> u32 {
        self.bricks.iter().map(|row| row.count_ones()).sum()
    }

    /// Whether the ball is in flight.
    #[cfg(test)]
    pub const fn in_flight(&self) -> bool {
        self.held.is_none()
    }

    /// The ball sitting on the paddle's middle.
    fn ball_on_paddle(&self) -> (i32, i32) {
        let x = self.paddle_x.saturating_add(PADDLE_WIDTH.saturating_sub(BALL).checked_div(2).unwrap_or(0));
        (x.saturating_mul(FIXED), PADDLE_Y.saturating_sub(BALL).saturating_mul(FIXED))
    }

    /// One tick: whether the screen changed.
    pub fn tick(&mut self, keys: Keys, sounds: &mut Vec<Effect>) -> bool {
        if self.over {
            return false;
        }
        let before = (self.paddle_x, self.ball);
        if keys.active(control::LEFT) {
            self.paddle_x = self.paddle_x.saturating_sub(PADDLE_SPEED).max(0);
        }
        if keys.active(control::RIGHT) {
            self.paddle_x = self.paddle_x.saturating_add(PADDLE_SPEED).min(FIELD_WIDTH.saturating_sub(PADDLE_WIDTH));
        }
        if let Some(waited) = self.held {
            self.ball = self.ball_on_paddle();
            if keys.active(control::FIRE) || keys.active(control::UP) || waited >= AUTO_LAUNCH_TICKS {
                self.held = None;
                // Up and away from the nearer wall.
                let leftwards = self.paddle_x.saturating_mul(2) > FIELD_WIDTH.saturating_sub(PADDLE_WIDTH);
                let across = self.speed.checked_div(2).unwrap_or(0);
                self.velocity = (if leftwards { across.saturating_neg() } else { across }, self.speed.saturating_neg());
                sounds.push(Effect::Click);
            } else {
                self.held = Some(waited.saturating_add(1));
            }
            return before != (self.paddle_x, self.ball);
        }
        self.fly(sounds);
        true
    }

    /// The ball's flight for a tick, in steps of at most half a sixel.
    fn fly(&mut self, sounds: &mut Vec<Effect>) {
        let (vx, vy) = self.velocity;
        let steps = vx.saturating_abs().max(vy.saturating_abs()).checked_div(HALF_SIXEL).unwrap_or(0).saturating_add(1);
        let mut moved = (0, 0);
        for step in 1..=steps {
            // Where this step should have the ball, from the tick's start,
            // so no fraction of the velocity is lost to rounding.
            let target =
                (self.velocity.0.saturating_mul(step).checked_div(steps).unwrap_or(0), self.velocity.1.saturating_mul(step).checked_div(steps).unwrap_or(0));
            let (dx, dy) = (target.0.saturating_sub(moved.0), target.1.saturating_sub(moved.1));
            moved = target;
            let x = self.ball.0.saturating_add(dx);
            if x < 0 || x.saturating_add(BALL.saturating_mul(FIXED)) > FIELD_WIDTH.saturating_mul(FIXED) || self.hit_bricks(x, self.ball.1, sounds) {
                self.velocity.0 = self.velocity.0.saturating_neg();
            } else {
                self.ball.0 = x;
            }
            let y = self.ball.1.saturating_add(dy);
            if y < 0 || self.hit_bricks(self.ball.0, y, sounds) {
                self.velocity.1 = self.velocity.1.saturating_neg();
            } else if self.velocity.1 > 0 && self.meets_paddle(y) {
                self.bounce(sounds);
            } else {
                self.ball.1 = y;
            }
            if self.ball.1 >= FIELD_HEIGHT.saturating_mul(FIXED) {
                self.lose_ball(sounds);
                return;
            }
        }
        if self.bricks.iter().all(|row| *row == 0) {
            self.wall = self.wall.saturating_add(1);
            self.bricks = [FULL_ROW; BRICK_ROWS];
            self.speed = START_SPEED.saturating_add(WALL_SPEEDUP.saturating_mul(i32::try_from(self.wall).unwrap_or(0))).min(TOP_SPEED);
            self.held = Some(0);
            sounds.push(Effect::Win);
        }
    }

    /// Whether the ball, its top at `y`, lands on the paddle.
    fn meets_paddle(&self, y: i32) -> bool {
        let bottom = y.saturating_add(BALL.saturating_mul(FIXED)).checked_div(FIXED).unwrap_or(0);
        let left = self.ball.0.checked_div(FIXED).unwrap_or(0);
        bottom > PADDLE_Y
            && bottom <= PADDLE_Y.saturating_add(PADDLE_HEIGHT)
            && left.saturating_add(BALL) > self.paddle_x
            && left < self.paddle_x.saturating_add(PADDLE_WIDTH)
    }

    /// Off the paddle: upwards, swung by where it met the paddle.
    fn bounce(&mut self, sounds: &mut Vec<Effect>) {
        self.hits = self.hits.saturating_add(1);
        if self.hits.checked_rem(HITS_PER_SPEEDUP) == Some(0) {
            self.speed = self.speed.saturating_add(1).min(TOP_SPEED);
        }
        let middle = self.paddle_x.saturating_add(PADDLE_WIDTH.checked_div(2).unwrap_or(0));
        let ball_middle = self.ball.0.checked_div(FIXED).unwrap_or(0).saturating_add(BALL.checked_div(2).unwrap_or(0));
        let offset = ball_middle.saturating_sub(middle);
        self.velocity = (self.speed.saturating_mul(offset).checked_div(SWING).unwrap_or(0), self.speed.saturating_neg());
        sounds.push(Effect::Click);
    }

    fn lose_ball(&mut self, sounds: &mut Vec<Effect>) {
        self.lives = self.lives.saturating_sub(1);
        sounds.push(Effect::Explosion);
        if self.lives == 0 {
            self.over = true;
            sounds.push(Effect::Lose);
        } else {
            self.held = Some(0);
            self.ball = self.ball_on_paddle();
        }
    }

    /// Knocks out every brick the ball would cover at `x`, `y`; whether
    /// any.
    fn hit_bricks(&mut self, x: i32, y: i32, sounds: &mut Vec<Effect>) -> bool {
        let (left, top) = (x.checked_div(FIXED).unwrap_or(0), y.checked_div(FIXED).unwrap_or(0));
        let mut hit = false;
        for sy in top..top.saturating_add(BALL) {
            for sx in left..left.saturating_add(BALL) {
                if let Some((row, column)) = brick_at(sx, sy)
                    && let Some(bits) = self.bricks.get_mut(row)
                    && has(*bits, column)
                {
                    *bits = cleared(*bits, column);
                    self.score = self.score.saturating_add(ROW_POINTS.get(row).copied().unwrap_or(0));
                    hit = true;
                }
            }
        }
        if hit {
            sounds.push(Effect::Hit);
        }
        hit
    }

    /// Draws the game.
    pub fn draw(&self, screen: &mut Screen) {
        let lives = format!("BALLS {}", self.lives);
        let wall = format!("WALL {}", self.wall.saturating_add(1));
        look::status(
            screen,
            Game::Breakout,
            &[(Colour::Yellow, &format!("SCORE {}", look::digits(self.score))), (Colour::Cyan, &wall), (Colour::White, &lives)],
        );
        let mut field = look::field();
        wall_of(&mut field, &self.bricks, BRICK_TOP);
        field.rectangle(self.paddle_x, PADDLE_Y, PADDLE_WIDTH, PADDLE_HEIGHT, Colour::White);
        let ball = if self.held.is_some() { self.ball_on_paddle() } else { self.ball };
        field.rectangle(ball.0.checked_div(FIXED).unwrap_or(0), ball.1.checked_div(FIXED).unwrap_or(0), BALL, BALL, Colour::White);
        field.draw(screen, look::FIELD_ROW, look::FIELD_COLUMN);
        let hint = if self.held.is_some() { "\u{2190} \u{2192} move   Space launches   Esc menu" } else { "Hold \u{2190} \u{2192} to move   Esc menu" };
        look::hint(screen, hint);
    }
}

impl Default for Breakout {
    fn default() -> Self {
        Self::new()
    }
}

/// The brick whose lit part holds the sixel `x`, `y`: its row and column.
fn brick_at(x: i32, y: i32) -> Option<(usize, i32)> {
    let down = y.checked_sub(BRICK_TOP).filter(|down| *down >= 0)?;
    let row = usize::try_from(down.checked_div(SLOT_HEIGHT)?).ok().filter(|row| *row < BRICK_ROWS)?;
    let column = x.checked_div(SLOT_WIDTH).filter(|column| (0..look::sixels(BRICK_COLUMNS)).contains(column) && x >= 0)?;
    let lit = down.checked_rem(SLOT_HEIGHT)? < BRICK_HEIGHT && x.checked_rem(SLOT_WIDTH)? < BRICK_WIDTH;
    lit.then_some((row, column))
}

/// Draws the bricks of `rows` from sixel row `top`.
fn wall_of(field: &mut Canvas, rows: &[u16], top: i32) {
    for ((row, bits), colour) in rows.iter().enumerate().zip(ROW_COLOURS) {
        let y = top.saturating_add(look::sixels(row).saturating_mul(SLOT_HEIGHT));
        for column in (0..look::sixels(BRICK_COLUMNS)).filter(|&column| has(*bits, column)) {
            field.rectangle(column.saturating_mul(SLOT_WIDTH), y, BRICK_WIDTH, BRICK_HEIGHT, colour);
        }
    }
}

/// The title page's picture: a wall with a gap knocked in it, the ball on
/// its way back, and the paddle.
#[must_use]
pub fn art() -> Canvas {
    let mut art = look::art(look::ART_HEIGHT);
    // The picture's layout, chosen by eye: a hole knocked up into the
    // wall, the ball under it, the paddle below.
    let knocked = [FULL_ROW, FULL_ROW & !0b0000_0110_0000, FULL_ROW & !0b0000_1111_0000, FULL_ROW & !0b0001_1111_1000, FULL_ROW & !0b0011_1111_1100];
    wall_of(&mut art, &knocked, 0);
    art.rectangle(44, 17, BALL, BALL, Colour::White);
    art.rectangle(36, 22, PADDLE_WIDTH, 1, Colour::White);
    art
}
