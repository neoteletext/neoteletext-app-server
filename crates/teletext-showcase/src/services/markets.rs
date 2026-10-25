//! Page 104: markets, simulated. Made-up indices and companies move by a
//! bounded random walk every few seconds, with arrows in green and red and
//! a mosaic chart of the first index. The page says it is demonstration
//! data.

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, SIXELS_DOWN, Screen, Sixels, attribute};
use teletext_runtime::{Application, Context, PageEntry};

use super::calendar::{SECOND_MS, hours_minutes_seconds};
use super::draw::{FIRST_ROW, band, demonstration, publish};
use super::{MARKETS, navigation};
use crate::common::page;

/// Prices change every three seconds: often enough to watch move.
const INTERVAL_MS: u64 = 3 * SECOND_MS;
/// Prices are kept in hundredths.
const HUNDREDTHS: u64 = 100;
/// Basis points in a whole: a change is reckoned in hundredths of a per
/// cent.
const BASIS_POINTS: i64 = 10_000;
/// The largest random move in one step, in basis points (0.25%).
const STEP_BASIS_POINTS: i64 = 25;
/// The walk's choices of move: every basis point from down to up.
pub const STEP_CHOICES: u32 = 51;
const _: () = assert!(STEP_CHOICES as i64 == STEP_BASIS_POINTS * 2 + 1, "a move either way, or none");
/// The pull back towards the starting price: one basis point a step for
/// each this many basis points away from it, so the walk wanders but
/// returns.
const REVERSION: i64 = 200;
/// A price never falls below half its starting price...
const FLOOR_DIVISOR: i64 = 2;
/// ...nor rises above twice it.
const CEILING_FACTOR: i64 = 2;

/// The indices: name and starting level in hundredths.
const INDICES: [(&str, i64); 4] = [("MOSAIC 100", 742_150), ("SIXEL 50", 318_825), ("FASTEXT TECH", 1_204_000), ("PARITY ALL", 455_390)];
/// The companies: name and starting price in hundredths of a pound.
const SHARES: [(&str, i64); 5] =
    [("Sixel Systems", 41_250), ("Subpage Foods", 8_760), ("Parity Power", 23_115), ("Header Rail", 5_480), ("Hamming Mines", 61_900)];

/// The chart's width in sixels, one a sample: three minutes of steps.
const CHART_WIDTH: usize = 60;
/// Its height in sixels: four rows.
const CHART_HEIGHT: usize = 12;
/// Its first row.
const CHART_ROW: usize = 11;
/// Its first column, after its colour attribute.
const CHART_COLUMN: usize = 1;
/// The high and low labels' column, after the chart's 30 cells.
const CHART_LABEL_COLUMN: usize = 31;

/// The row of the indices' heading.
const INDICES_ROW: usize = 4;
/// The row of the shares' heading.
const SHARES_ROW: usize = 16;
/// Where a line's level goes: its colour attribute's column.
const LEVEL_COLUMN: usize = 14;
/// The level's width, right-aligned.
const LEVEL_WIDTH: usize = 10;
/// The arrow's mosaic colour attribute's column; Separated Graphics
/// follows it, so arrows on rows one above the other do not touch, and
/// the arrow takes the three cells after that.
const ARROW_COLUMN: usize = 25;
/// The change's colour attribute's column.
const CHANGE_COLUMN: usize = 30;
/// The arrows' size in sixels: three cells of one row.
const ARROW_SIZE: (usize, usize) = (6, 3);
/// The arrows' sixels: a triangle pointing up or down.
const UP: [&str; 3] = ["..##..", ".####.", "######"];
/// The down arrow.
const DOWN: [&str; 3] = ["######", ".####.", "..##.."];

/// One price that moves.
#[derive(Clone, Debug)]
pub struct Instrument {
    /// Its name.
    pub name: &'static str,
    /// Its price or level now, in hundredths.
    pub value: i64,
    /// Where it started, which its walk returns towards and is bounded
    /// by.
    pub base: i64,
    /// Its value before the last step, for the arrow.
    pub previous: i64,
}

impl Instrument {
    /// An instrument starting at `value`.
    #[must_use]
    pub const fn new(name: &'static str, value: i64) -> Self {
        Self { name, value, base: value, previous: value }
    }

    /// The lowest the walk may go.
    #[must_use]
    pub const fn floor(&self) -> i64 {
        self.base / FLOOR_DIVISOR
    }

    /// The highest.
    #[must_use]
    pub const fn ceiling(&self) -> i64 {
        self.base.saturating_mul(CEILING_FACTOR)
    }

    /// One step of the walk with `random` below [`STEP_CHOICES`]: a move
    /// of up to [`STEP_BASIS_POINTS`] either way, pulled back towards the
    /// start, and kept between the floor and the ceiling.
    pub fn walk(&mut self, random: u32) {
        let moved = i64::from(random.min(STEP_CHOICES.saturating_sub(1))).saturating_sub(STEP_BASIS_POINTS);
        let away = self.value.saturating_sub(self.base).saturating_mul(BASIS_POINTS).checked_div(self.base).unwrap_or(0);
        let basis_points = moved.saturating_sub(away / REVERSION);
        let change = self.value.saturating_mul(basis_points) / BASIS_POINTS;
        self.previous = self.value;
        self.value = self.value.saturating_add(change).clamp(self.floor(), self.ceiling());
    }

    /// Its change since the start, in basis points.
    #[must_use]
    pub fn change(&self) -> i64 {
        self.value.saturating_sub(self.base).saturating_mul(BASIS_POINTS).checked_div(self.base).unwrap_or(0)
    }
}

/// "1234.56".
fn decimal(hundredths: i64) -> String {
    let sign = if hundredths < 0 { "-" } else { "" };
    let magnitude = hundredths.unsigned_abs();
    let whole = magnitude.checked_div(HUNDREDTHS).unwrap_or(0);
    let part = magnitude.checked_rem(HUNDREDTHS).unwrap_or(0);
    format!("{sign}{whole}.{part:02}")
}

/// The markets: the indices, the shares and the first index's recent
/// levels.
#[derive(Debug)]
pub struct Markets {
    /// The indices.
    pub indices: Vec<Instrument>,
    /// The shares.
    pub shares: Vec<Instrument>,
    history: VecDeque<i64>,
}

impl Default for Markets {
    fn default() -> Self {
        Self {
            indices: INDICES.iter().map(|&(name, value)| Instrument::new(name, value)).collect(),
            shares: SHARES.iter().map(|&(name, value)| Instrument::new(name, value)).collect(),
            history: VecDeque::with_capacity(CHART_WIDTH),
        }
    }
}

impl Markets {
    /// Moves every price one step.
    pub fn step(&mut self, context: &mut Context<'_>) {
        for instrument in self.indices.iter_mut().chain(self.shares.iter_mut()) {
            instrument.walk(context.below(STEP_CHOICES));
        }
        if self.history.len() >= CHART_WIDTH {
            self.history.pop_front();
        }
        if let Some(first) = self.indices.first() {
            self.history.push_back(first.value);
        }
    }

    /// The page at the Unix time `epoch_ms`.
    #[must_use]
    pub fn screen(&self, epoch_ms: u64) -> Screen {
        let mut screen = page("Markets", Colour::Green, navigation(MARKETS));
        screen.coloured(FIRST_ROW, 0, Colour::Cyan, &format!("Prices at {} UTC", hours_minutes_seconds(epoch_ms)));
        band(&mut screen, INDICES_ROW, Colour::Blue, Colour::White, "INDEX            LEVEL       CHANGE");
        for (instrument, row) in self.indices.iter().zip(INDICES_ROW.saturating_add(1)..) {
            line(&mut screen, row, instrument);
        }
        if let Some(first) = self.indices.first() {
            screen.coloured(CHART_ROW.saturating_sub(1), 1, Colour::Cyan, &format!("{}, the last three minutes", first.name));
        }
        self.chart(&mut screen);
        band(&mut screen, SHARES_ROW, Colour::Blue, Colour::White, "SHARE            PRICE       CHANGE");
        for (instrument, row) in self.shares.iter().zip(SHARES_ROW.saturating_add(1)..) {
            line(&mut screen, row, instrument);
        }
        demonstration(&mut screen);
        screen
    }

    /// The first index's levels as a filled mosaic chart, scaled between
    /// its lowest and highest, green when it rose over the chart and red
    /// when it fell.
    fn chart(&self, screen: &mut Screen) {
        let (Some(&low), Some(&high)) = (self.history.iter().min(), self.history.iter().max()) else { return };
        let range = high.saturating_sub(low).max(1);
        let mut canvas = Sixels::new(CHART_WIDTH, CHART_HEIGHT);
        let tallest = i64::try_from(CHART_HEIGHT.saturating_sub(1)).unwrap_or(0);
        for (x, &level) in self.history.iter().enumerate() {
            // At least the bottom sixel, so the lowest level still shows.
            let height = level.saturating_sub(low).saturating_mul(tallest).checked_div(range).unwrap_or(0);
            let top = usize::try_from(tallest.saturating_sub(height)).unwrap_or(0);
            for y in top..CHART_HEIGHT {
                canvas.set(x, y, true);
            }
        }
        let first = self.history.front().copied().unwrap_or(0);
        let last = self.history.back().copied().unwrap_or(0);
        let colour = if last >= first { Colour::Green } else { Colour::Red };
        canvas.draw(screen, CHART_ROW, CHART_COLUMN, colour);
        let bottom = CHART_ROW.saturating_add(CHART_HEIGHT.div_ceil(SIXELS_DOWN)).saturating_sub(1);
        screen.coloured(CHART_ROW, CHART_LABEL_COLUMN, Colour::White, &decimal(high));
        screen.coloured(bottom, CHART_LABEL_COLUMN, Colour::White, &decimal(low));
    }
}

/// One instrument's line: name, level, arrow and change.
fn line(screen: &mut Screen, row: usize, instrument: &Instrument) {
    screen.coloured(row, 0, Colour::White, instrument.name);
    let level = decimal(instrument.value);
    let pad = LEVEL_WIDTH.saturating_sub(level.len());
    screen.coloured(row, LEVEL_COLUMN.saturating_add(pad), Colour::Yellow, &level);
    let (arrow, colour) = match instrument.value.cmp(&instrument.previous) {
        core::cmp::Ordering::Greater => (Some(UP), Colour::Green),
        core::cmp::Ordering::Less => (Some(DOWN), Colour::Red),
        core::cmp::Ordering::Equal => (None, Colour::White),
    };
    if let Some(arrow) = arrow {
        let mut canvas = Sixels::new(ARROW_SIZE.0, ARROW_SIZE.1);
        super::draw::art(&mut canvas, 0, 0, &arrow);
        canvas.draw(screen, row, ARROW_COLUMN.saturating_add(2), colour);
        // The canvas puts its colour in the cell before it; the colour goes
        // first, and Separated Graphics after it, which is set-at.
        screen.codes(row, ARROW_COLUMN, &[colour.mosaic(), attribute::SEPARATED]);
    }
    let change = instrument.change();
    let sign = if change > 0 { "+" } else { "" };
    let change_colour = match change.cmp(&0) {
        core::cmp::Ordering::Greater => Colour::Green,
        core::cmp::Ordering::Less => Colour::Red,
        core::cmp::Ordering::Equal => Colour::White,
    };
    screen.coloured(row, CHANGE_COLUMN, change_colour, &format!("{sign}{}%", decimal(change)));
}

impl Application for Markets {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(MARKETS, "Markets")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        // A chart's worth of history, so the page opens with one.
        for _ in 0..CHART_WIDTH {
            self.step(context);
        }
        self.wake(context);
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        self.step(context);
        let screen = self.screen(context.epoch_ms());
        publish(context, MARKETS, &screen);
        context.wake_in(INTERVAL_MS);
    }
}
