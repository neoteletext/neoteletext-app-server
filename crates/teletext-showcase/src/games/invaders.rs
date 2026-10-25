//! Space invaders, with the sci-fi sprites as the enemies: each row of a
//! wave is a different creature, and each wave brings the next ones.
//!
//! Everything is in sixels on the 78 by 63 playfield. Rows of different
//! colours keep a row of cells apart, and enemies, shields and the player
//! each sit on whole rows of cells, so every colour shows (see `canvas`).

use alloc::format;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, SIXELS_DOWN, Screen};

use super::canvas::Canvas;
use super::look::{self, FIELD_HEIGHT, FIELD_WIDTH};
use super::sprites::{self, Sprite};
use super::{Dice, Game, Keys, between, control};
use crate::sound::Effect;

/// A wave's rows and columns of enemies.
pub const ENEMY_ROWS: usize = 4;
const ENEMY_COLUMNS: usize = 5;
/// An enemy's largest size in sixels: five columns of ten with three
/// between leave the wave sixteen sixels to march across.
pub const ENEMY_WIDTH: usize = 10;
pub const ENEMY_HEIGHT: usize = 6;
/// Enemies' spacing: across, the width and a gap; down, three rows of
/// cells, so each row of enemies keeps to its own two rows of cells and
/// its colour.
const PITCH_X: i32 = 13;
const PITCH_Y: i32 = 9;
/// What an enemy of each row is worth, top row first.
pub const POINTS: [u32; ENEMY_ROWS] = [40, 30, 20, 10];
/// The wave's first row, one row of cells under the saucer's; each later
/// wave starts a row of cells lower, three times at most.
const WAVE_TOP: i32 = 6;
const LOWER_WAVES: u32 = 3;
/// How far the wave drops at an edge: a row of cells, so it stays on
/// whole rows.
const DROP: i32 = look::sixels(SIXELS_DOWN);
/// The player's ship, on the last row of cells.
const PLAYER_Y: i32 = FIELD_HEIGHT - 3;
const PLAYER_SPEED: i32 = 2;
/// The shields: three, spread evenly, on two rows of cells above the
/// player.
const SHIELDS: usize = 3;
const SHIELD_Y: i32 = 48;
const SHIELD_LEFT: [i32; SHIELDS] = [7, 33, 59];
const SHIELD_WIDTH: i32 = 12;
const SHIELD_SHAPE: [u16; 6] = [0b0001_1111_1000, 0b0111_1111_1110, 0b1111_1111_1111, 0b1111_1111_1111, 0b1111_0000_1111, 0b1110_0000_0111];
/// The player's shot climbs this many sixels a tick.
const SHOT_SPEED: i32 = 3;
/// Bombs: at most this many falling at once.
const MAX_BOMBS: usize = 4;
/// The chance of a new bomb each tick, in thirtieths: three, and one more
/// each wave, up to thirteen.
const BOMB_CHANCE_OUT_OF: u32 = 30;
const BOMB_CHANCE_BASE: u32 = 3;
const BOMB_CHANCE_MOST: u32 = 13;
/// The banners' row: under the wave, above the shields.
pub const BANNER_ROW: usize = 15;
/// Lives at the start.
const LIVES: u8 = 3;
/// Ticks the player's explosion lasts before the next ship.
const DYING_TICKS: u8 = 15;
/// Ticks a wave's banner shows before the wave moves.
const BANNER_TICKS: u8 = 15;
/// Ticks a burst shows.
const BURST_TICKS: u8 = 3;
const MAX_BURSTS: usize = 4;
/// The saucer: on the top row of cells, every 15 to 30 seconds, worth 50
/// to 300.
const SAUCER_Y: i32 = 0;
const SAUCER_EVERY: (i32, i32) = (150, 300);
const SAUCER_POINTS: (i32, i32) = (1, 6);
const SAUCER_WIDTH: i32 = 8;
const SAUCER_HEIGHT: i32 = 3;
const SAUCER_POINT_STEP: u32 = 50;
/// The wave marches one step every few ticks: one tick each seven enemies
/// left, less one for each wave after the first two, never under one.
const ENEMIES_PER_TICK: u32 = 7;
/// The player's ship's width, and where its gun is.
const SHIP_WIDTH: i32 = 7;
const GUN: i32 = 3;

fn player_ship() -> Sprite {
    Sprite::drawn("Ship", Colour::Green, &["...#...", ".#####.", "#######"])
}

fn saucer_sprite() -> Sprite {
    Sprite::drawn("Saucer", Colour::Magenta, &["..####..", ".#.##.#.", "########"])
}

/// The enemy of `row` in `wave`: each wave takes the next four creatures.
#[must_use]
pub fn enemy(enemies: &[Sprite], wave: u32, row: usize) -> Option<&Sprite> {
    let rows = u32::try_from(ENEMY_ROWS).unwrap_or(1);
    let index = usize::try_from(wave.saturating_mul(rows)).unwrap_or(0).saturating_add(row);
    enemies.get(index.checked_rem(enemies.len()).unwrap_or(0))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Point {
    x: i32,
    y: i32,
}

/// The saucer crossing the top: where, which way, and its worth.
#[derive(Clone, Copy, Debug)]
struct Saucer {
    at: Point,
    heading: i32,
    worth: u32,
}

/// Whether bit `index` of `bits` is set; outside, not.
fn bit_set(bits: u16, index: i32) -> bool {
    u32::try_from(index).ok().and_then(|index| bits.checked_shr(index)).is_some_and(|rest| rest & 1 == 1)
}

/// `bits` with bit `index` cleared.
fn bit_cleared(bits: u16, index: i32) -> u16 {
    u32::try_from(index).ok().and_then(|index| 1_u16.checked_shl(index)).map_or(bits, |bit| bits & !bit)
}

/// The left edge that centres something `width` wide on the playfield.
fn centred(width: i32) -> i32 {
    FIELD_WIDTH.saturating_sub(width).checked_div(2).unwrap_or(0)
}

/// A game of invaders.
#[derive(Clone, Debug)]
pub struct Invaders {
    score: u32,
    lives: u8,
    /// The wave, from 0.
    wave: u32,
    /// The enemies alive, bit `row * ENEMY_COLUMNS + column`.
    alive: u32,
    /// The wave's top-left, and which way it marches: 1 right, -1 left.
    formation: Point,
    heading: i32,
    march_wait: u32,
    /// The walking frame, flipped at each step.
    stride: bool,
    player_x: i32,
    shot: Option<Point>,
    bombs: [Option<Point>; MAX_BOMBS],
    shields: [[u16; 6]; SHIELDS],
    bursts: [Option<(Point, u8)>; MAX_BURSTS],
    saucer: Option<Saucer>,
    saucer_wait: i32,
    dying: u8,
    banner: u8,
    over: bool,
}

impl Invaders {
    /// A new game at its first wave.
    #[must_use]
    pub fn new() -> Self {
        let mut game = Self {
            score: 0,
            lives: LIVES,
            wave: 0,
            alive: 0,
            formation: Point { x: 0, y: 0 },
            heading: 1,
            march_wait: 0,
            stride: false,
            player_x: 0,
            shot: None,
            bombs: [None; MAX_BOMBS],
            shields: [SHIELD_SHAPE; SHIELDS],
            bursts: [None; MAX_BURSTS],
            saucer: None,
            saucer_wait: SAUCER_EVERY.1,
            dying: 0,
            banner: BANNER_TICKS,
            over: false,
        };
        game.start_wave();
        game
    }

    /// Lines up the wave: all alive, at the left, the shields mended.
    fn start_wave(&mut self) {
        let count = ENEMY_ROWS.saturating_mul(ENEMY_COLUMNS);
        self.alive = 1_u32.checked_shl(u32::try_from(count).unwrap_or(0)).map_or(u32::MAX, |bit| bit.saturating_sub(1));
        let lower = i32::try_from(self.wave.min(LOWER_WAVES)).unwrap_or(0).saturating_mul(DROP);
        self.formation = Point { x: 0, y: WAVE_TOP.saturating_add(lower) };
        self.heading = 1;
        self.shields = [SHIELD_SHAPE; SHIELDS];
        self.player_x = centred(SHIP_WIDTH);
        self.shot = None;
        self.bombs = [None; MAX_BOMBS];
        self.banner = BANNER_TICKS;
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

    /// Lives left.
    #[cfg(test)]
    pub const fn lives(&self) -> u8 {
        self.lives
    }

    /// Every enemy of the wave shot down at once.
    #[cfg(test)]
    pub const fn clear_wave(&mut self) {
        self.alive = 0;
    }

    /// The wave, from 0.
    #[cfg(test)]
    pub const fn wave(&self) -> u32 {
        self.wave
    }

    /// The player's ship's left edge.
    #[cfg(test)]
    pub const fn player_x(&self) -> i32 {
        self.player_x
    }

    fn bit(row: usize, column: usize) -> u32 {
        let index = row.saturating_mul(ENEMY_COLUMNS).saturating_add(column);
        1_u32.checked_shl(u32::try_from(index).unwrap_or(u32::MAX)).unwrap_or(0)
    }

    fn is_alive(&self, row: usize, column: usize) -> bool {
        self.alive & Self::bit(row, column) != 0
    }

    /// The top-left of the enemy at `row`, `column`.
    fn enemy_at(&self, row: usize, column: usize) -> Point {
        let column = i32::try_from(column).unwrap_or(0);
        let row = i32::try_from(row).unwrap_or(0);
        Point { x: self.formation.x.saturating_add(column.saturating_mul(PITCH_X)), y: self.formation.y.saturating_add(row.saturating_mul(PITCH_Y)) }
    }

    /// The enemy whose box holds `point`.
    fn enemy_hit(&self, point: Point) -> Option<(usize, usize)> {
        let width = i32::try_from(ENEMY_WIDTH).unwrap_or(0);
        let height = i32::try_from(ENEMY_HEIGHT).unwrap_or(0);
        (0..ENEMY_ROWS).flat_map(|row| (0..ENEMY_COLUMNS).map(move |column| (row, column))).find(|&(row, column)| {
            let at = self.enemy_at(row, column);
            self.is_alive(row, column) && point.x >= at.x && point.x < at.x.saturating_add(width) && point.y >= at.y && point.y < at.y.saturating_add(height)
        })
    }

    fn burst(&mut self, at: Point) {
        if let Some(slot) = self.bursts.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some((at, BURST_TICKS));
        }
    }

    /// Whether the shield sixel at `point` stands; if so it is knocked out
    /// with its four neighbours.
    fn shield_hit(&mut self, point: Point) -> bool {
        let row = point.y.saturating_sub(SHIELD_Y);
        let Ok(row_index) = usize::try_from(row) else { return false };
        for (shield, left) in self.shields.iter_mut().zip(SHIELD_LEFT) {
            let column = point.x.saturating_sub(left);
            if !shield.get(row_index).is_some_and(|&bits| (0..SHIELD_WIDTH).contains(&column) && bit_set(bits, column)) {
                continue;
            }
            for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (x, y) = (column.saturating_add(dx), row.saturating_add(dy));
                if let (Ok(y), true) = (usize::try_from(y), (0..SHIELD_WIDTH).contains(&x))
                    && let Some(bits) = shield.get_mut(y)
                {
                    *bits = bit_cleared(*bits, x);
                }
            }
            return true;
        }
        false
    }

    /// One tick: whether the screen changed.
    pub fn tick(&mut self, keys: Keys, dice: &mut dyn Dice, sounds: &mut Vec<Effect>) -> bool {
        if self.over {
            return false;
        }
        self.age_bursts();
        // While the wave's banner shows, only the ship moves.
        if self.banner > 0 {
            self.banner = self.banner.saturating_sub(1);
            self.move_player(keys, false, sounds);
            return true;
        }
        self.move_player(keys, true, sounds);
        self.move_shot(sounds);
        self.move_bombs(sounds);
        self.march(sounds);
        self.drop_bombs(dice);
        self.fly_saucer(dice);
        if self.alive == 0 && !self.over {
            self.wave = self.wave.saturating_add(1);
            self.start_wave();
            sounds.push(Effect::Win);
        }
        true
    }

    fn age_bursts(&mut self) {
        for slot in &mut self.bursts {
            *slot = slot.and_then(|(at, ticks)| (ticks > 1).then(|| (at, ticks.saturating_sub(1))));
        }
    }

    fn move_player(&mut self, keys: Keys, may_fire: bool, sounds: &mut Vec<Effect>) {
        if self.dying > 0 {
            self.dying = self.dying.saturating_sub(1);
            if self.dying == 0 {
                self.player_x = centred(SHIP_WIDTH);
                self.bombs = [None; MAX_BOMBS];
            }
            return;
        }
        let widest = FIELD_WIDTH.saturating_sub(SHIP_WIDTH);
        if keys.active(control::LEFT) {
            self.player_x = self.player_x.saturating_sub(PLAYER_SPEED).max(0);
        }
        if keys.active(control::RIGHT) {
            self.player_x = self.player_x.saturating_add(PLAYER_SPEED).min(widest);
        }
        if may_fire && keys.active(control::FIRE) && self.shot.is_none() {
            self.shot = Some(Point { x: self.player_x.saturating_add(GUN), y: PLAYER_Y.saturating_sub(1) });
            sounds.push(Effect::Laser);
        }
    }

    fn move_shot(&mut self, sounds: &mut Vec<Effect>) {
        for _ in 0..SHOT_SPEED {
            let Some(shot) = self.shot else { return };
            let next = Point { x: shot.x, y: shot.y.saturating_sub(1) };
            self.shot = Some(next);
            if next.y < 0 {
                self.shot = None;
            } else if let Some(saucer) = self.saucer
                && next.x >= saucer.at.x
                && next.x < saucer.at.x.saturating_add(SAUCER_WIDTH)
                && next.y < saucer.at.y.saturating_add(SAUCER_HEIGHT)
            {
                self.saucer = None;
                self.shot = None;
                self.score = self.score.saturating_add(saucer.worth);
                self.burst(saucer.at);
                sounds.push(Effect::Coin);
            } else if let Some((row, column)) = self.enemy_hit(next) {
                self.alive &= !Self::bit(row, column);
                self.score = self.score.saturating_add(POINTS.get(row).copied().unwrap_or(0));
                self.shot = None;
                self.burst(self.enemy_at(row, column));
                sounds.push(Effect::Explosion);
            } else if self.shield_hit(next) {
                self.shot = None;
            } else if let Some(bomb) =
                self.bombs.iter_mut().find(|bomb| bomb.is_some_and(|bomb| bomb.x == next.x && (bomb.y == next.y || bomb.y == next.y.saturating_add(1))))
            {
                *bomb = None;
                self.shot = None;
            }
        }
    }

    fn move_bombs(&mut self, sounds: &mut Vec<Effect>) {
        let speed = if self.wave >= 2 { 2 } else { 1 };
        for index in 0..MAX_BOMBS {
            for _ in 0..speed {
                let Some(Some(bomb)) = self.bombs.get(index).copied() else { break };
                let next = Point { x: bomb.x, y: bomb.y.saturating_add(1) };
                let mut keep = Some(next);
                if next.y >= FIELD_HEIGHT || self.shield_hit(next) {
                    keep = None;
                } else if self.dying == 0 && next.y >= PLAYER_Y && next.x >= self.player_x && next.x < self.player_x.saturating_add(SHIP_WIDTH) {
                    keep = None;
                    self.lose_life(sounds);
                }
                if let Some(slot) = self.bombs.get_mut(index) {
                    *slot = keep;
                }
            }
        }
    }

    fn lose_life(&mut self, sounds: &mut Vec<Effect>) {
        self.lives = self.lives.saturating_sub(1);
        self.dying = DYING_TICKS;
        self.burst(Point { x: self.player_x, y: PLAYER_Y.saturating_sub(2) });
        sounds.push(Effect::Explosion);
        if self.lives == 0 {
            self.over = true;
            sounds.push(Effect::Lose);
        }
    }

    /// The wave's step: across, or down at an edge; an enemy that reaches
    /// the shields wrecks them, and one that reaches the player's row ends
    /// the game.
    fn march(&mut self, sounds: &mut Vec<Effect>) {
        if self.march_wait > 0 {
            self.march_wait = self.march_wait.saturating_sub(1);
            return;
        }
        // From the third wave on, each wave marches a tick faster.
        let faster = self.wave.saturating_sub(1);
        self.march_wait = self.alive.count_ones().checked_div(ENEMIES_PER_TICK).unwrap_or(0).saturating_sub(faster);
        let columns: Vec<usize> = (0..ENEMY_COLUMNS).filter(|&column| (0..ENEMY_ROWS).any(|row| self.is_alive(row, column))).collect();
        let (Some(&first), Some(&last)) = (columns.first(), columns.last()) else { return };
        let left = self.enemy_at(0, first).x;
        let right = self.enemy_at(0, last).x.saturating_add(i32::try_from(ENEMY_WIDTH).unwrap_or(0));
        let at_edge = (self.heading > 0 && right >= FIELD_WIDTH) || (self.heading < 0 && left <= 0);
        if at_edge {
            self.formation.y = self.formation.y.saturating_add(DROP);
            self.heading = self.heading.saturating_neg();
        } else {
            self.formation.x = self.formation.x.saturating_add(self.heading);
        }
        self.stride = !self.stride;
        let lowest = (0..ENEMY_ROWS).rev().find(|&row| (0..ENEMY_COLUMNS).any(|column| self.is_alive(row, column)));
        if let Some(row) = lowest {
            let bottom = self.enemy_at(row, 0).y.saturating_add(i32::try_from(ENEMY_HEIGHT).unwrap_or(0));
            if bottom > SHIELD_Y {
                for shield in &mut self.shields {
                    let wrecked = usize::try_from(bottom.saturating_sub(SHIELD_Y)).unwrap_or(0);
                    for bits in shield.iter_mut().take(wrecked) {
                        *bits = 0;
                    }
                }
            }
            if bottom > PLAYER_Y && !self.over {
                self.over = true;
                sounds.push(Effect::Lose);
            }
        }
    }

    fn drop_bombs(&mut self, dice: &mut dyn Dice) {
        let chance = BOMB_CHANCE_BASE.saturating_add(self.wave).min(BOMB_CHANCE_MOST);
        if dice.below(BOMB_CHANCE_OUT_OF) >= chance {
            return;
        }
        let Some(slot) = self.bombs.iter().position(Option::is_none) else { return };
        let column = usize::try_from(dice.below(u32::try_from(ENEMY_COLUMNS).unwrap_or(1))).unwrap_or(0);
        let Some(row) = (0..ENEMY_ROWS).rev().find(|&row| self.is_alive(row, column)) else { return };
        let at = self.enemy_at(row, column);
        let half = look::sixels(ENEMY_WIDTH).checked_div(2).unwrap_or(0);
        if let Some(bomb) = self.bombs.get_mut(slot) {
            *bomb = Some(Point { x: at.x.saturating_add(half), y: at.y.saturating_add(i32::try_from(ENEMY_HEIGHT).unwrap_or(0)) });
        }
    }

    fn fly_saucer(&mut self, dice: &mut dyn Dice) {
        if let Some(saucer) = self.saucer {
            let next = saucer.at.x.saturating_add(saucer.heading);
            self.saucer = (next > SAUCER_WIDTH.saturating_neg() && next < FIELD_WIDTH).then_some(Saucer { at: Point { x: next, y: saucer.at.y }, ..saucer });
            return;
        }
        self.saucer_wait = self.saucer_wait.saturating_sub(1);
        if self.saucer_wait <= 0 {
            self.saucer_wait = between(dice, SAUCER_EVERY.0, SAUCER_EVERY.1);
            let worth = u32::try_from(between(dice, SAUCER_POINTS.0, SAUCER_POINTS.1)).unwrap_or(1).saturating_mul(SAUCER_POINT_STEP);
            let (x, heading) = if dice.below(2) == 0 { (SAUCER_WIDTH.saturating_sub(1).saturating_neg(), 1) } else { (FIELD_WIDTH.saturating_sub(1), -1) };
            self.saucer = Some(Saucer { at: Point { x, y: SAUCER_Y }, heading, worth });
        }
    }

    /// Draws the game: status bar, playfield and controls.
    pub fn draw(&self, screen: &mut Screen, enemies: &[Sprite]) {
        let lives = format!("LIVES {}", self.lives);
        let wave = format!("WAVE {}", self.wave.saturating_add(1));
        look::status(
            screen,
            Game::Invaders,
            &[(Colour::Yellow, &format!("SCORE {}", look::digits(self.score))), (Colour::Cyan, &wave), (Colour::White, &lives)],
        );
        let mut field = look::field();
        if let Some(saucer) = self.saucer {
            field.sprite(&saucer_sprite(), saucer.at.x, saucer.at.y, Colour::Magenta);
        }
        for row in 0..ENEMY_ROWS {
            let Some(sprite) = enemy(enemies, self.wave, row) else { continue };
            for column in (0..ENEMY_COLUMNS).filter(|&column| self.is_alive(row, column)) {
                let at = self.enemy_at(row, column);
                let inset = look::sixels(ENEMY_WIDTH.saturating_sub(sprite.width)).checked_div(2).unwrap_or(0);
                let centred = Point { x: at.x.saturating_add(inset), y: at.y };
                draw_walking(&mut field, sprite, centred, self.stride);
            }
        }
        for (shield, left) in self.shields.iter().zip(SHIELD_LEFT) {
            for (row, bits) in shield.iter().enumerate() {
                for column in (0..SHIELD_WIDTH).filter(|&column| bit_set(*bits, column)) {
                    field.plot(left.saturating_add(column), SHIELD_Y.saturating_add(i32::try_from(row).unwrap_or(0)), Colour::Green);
                }
            }
        }
        if self.dying == 0 {
            field.sprite(&player_ship(), self.player_x, PLAYER_Y, Colour::Green);
        }
        if let Some(shot) = self.shot {
            field.rectangle(shot.x, shot.y, 1, 2, Colour::White);
        }
        for bomb in self.bombs.iter().flatten() {
            field.rectangle(bomb.x, bomb.y.saturating_sub(1), 1, 2, Colour::Yellow);
        }
        for (at, _) in self.bursts.iter().flatten() {
            field.sprite(&sprites::burst(), at.x, at.y, Colour::Yellow);
        }
        field.draw(screen, look::FIELD_ROW, look::FIELD_COLUMN);
        if self.banner > 0 && !self.over {
            let name = enemy(enemies, self.wave, 0).map_or_else(|| alloc::string::String::from("the invaders"), |sprite| sprite.name.clone());
            look::banner(screen, BANNER_ROW, Colour::Blue, &format!("WAVE {}", self.wave.saturating_add(1)), &format!("Here come the {name}s"));
        }
        look::hint(screen, "\u{2190} \u{2192} move   Space fires   Esc menu");
    }
}

impl Default for Invaders {
    fn default() -> Self {
        Self::new()
    }
}

/// Draws an enemy in its colour; on the second stride its lowest two rows
/// step outwards a sixel, so the wave walks.
fn draw_walking(field: &mut Canvas, sprite: &Sprite, at: Point, stride: bool) {
    let middle = sprite.width.checked_div(2).unwrap_or(0);
    let feet = sprite.height.saturating_sub(2);
    for y in 0..sprite.height {
        for x in (0..sprite.width).filter(|&x| sprite.lit(x, y)) {
            let mut column = i32::try_from(x).unwrap_or(0);
            if stride && y >= feet {
                column = if x < middle { column.saturating_sub(1) } else { column.saturating_add(1) };
            }
            field.plot(at.x.saturating_add(column), at.y.saturating_add(i32::try_from(y).unwrap_or(0)), sprite.colour);
        }
    }
}
