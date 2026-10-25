//! Page 102: weather, simulated, for a made-up region, the Mosaic Isles:
//! a mosaic map with each town's temperature, a list of the towns, and a
//! five-day outlook with mosaic symbols. Temperatures follow the time of
//! day and drift a little each minute; each day's weather comes from the
//! date, so every viewer and every restart sees the same. The page says
//! it is demonstration data.

use alloc::format;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen, Sixels};
use teletext_runtime::{Application, Context, PageEntry};

use super::calendar::{DAY_MINUTES, DAY_MS, Date, MINUTE_MS, hours_minutes, minute_of_day};
use super::draw::{FIRST_ROW, art, demonstration, publish, right};
use super::{WEATHER, navigation};
use crate::common::page;

/// The page changes once a minute: weather moves slowly.
const INTERVAL_MS: u64 = MINUTE_MS;

/// The map's first row.
const MAP_ROW: usize = 4;
/// The map's first column, after the sea's background (two cells) and
/// the land's mosaic colour.
const MAP_COLUMN: usize = 3;
/// The map in sixels, 22 cells by 10 rows: `#` is land.
const MAP: [&str; 30] = [
    "............................................",
    "..........................######............",
    "........................##########..........",
    "..............######...############.........",
    "...........###########...##########.........",
    ".........###############..########..........",
    "........##################..####............",
    ".......####################.................",
    "......######################................",
    "......#######################...............",
    ".....#########################..............",
    ".....##########################.............",
    "....###########################.............",
    "....############################............",
    ".....###########...##############...........",
    ".....#########.......############...........",
    "......#######.........###########...........",
    ".......#####..........###########...........",
    "........###..........############...........",
    ".....................###########............",
    "....................###########.............",
    "...###.............###########..............",
    "..#####...........###########...............",
    "..######.........###########................",
    "...####.........##########..................",
    "....##..........#########...................",
    "................#######.....................",
    ".................####.......................",
    "............................................",
    "............................................",
];
/// The map's size in sixels.
const MAP_SIZE: (usize, usize) = (44, 30);
/// The map's rows of cells.
const MAP_ROWS: usize = 10;
/// The town list's colour attribute's column, after the map's 22 cells.
const LIST_COLUMN: usize = 25;
/// The column of a town's weather, after its temperature.
const LIST_WEATHER_COLUMN: usize = 29;

/// A town: its name, where its temperature goes on the map (the label's
/// first cell, as a screen row and column), and how much warmer than the
/// region it runs, in degrees.
struct Town {
    name: &'static str,
    row: usize,
    column: usize,
    warmth: i64,
}

/// The towns of the Mosaic Isles.
const TOWNS: [Town; 5] = [
    Town { name: "Header Hill", row: 5, column: 17, warmth: -2 },
    Town { name: "Mosaic Bay", row: 7, column: 6, warmth: 0 },
    Town { name: "Hamming Moor", row: 8, column: 14, warmth: -1 },
    Town { name: "Port Sixel", row: 10, column: 4, warmth: 1 },
    Town { name: "Parity Point", row: 12, column: 12, warmth: 2 },
];

/// The kinds of weather, with their symbols.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Sunshine.
    Sunny,
    /// Sun and cloud.
    Bright,
    /// Cloud.
    Cloudy,
    /// Rain.
    Rain,
    /// Fog.
    Fog,
}

/// The kinds to choose from, each as often as it is listed: rain and fog
/// less often than the rest.
const KINDS: [Kind; 8] = [Kind::Sunny, Kind::Sunny, Kind::Bright, Kind::Bright, Kind::Cloudy, Kind::Cloudy, Kind::Rain, Kind::Fog];

impl Kind {
    /// Its word, at most six characters.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Sunny => "Sunny",
            Self::Bright => "Bright",
            Self::Cloudy => "Cloudy",
            Self::Rain => "Rain",
            Self::Fog => "Fog",
        }
    }

    /// Its symbol, two rows of cells, each five cells by one: the sixels
    /// and the colour of each half.
    const fn symbol(self) -> ([&'static str; 3], Colour, [&'static str; 3], Colour) {
        match self {
            Self::Sunny => (["..#.##.#..", "...####...", ".########."], Colour::Yellow, [".########.", "...####...", "..#.##.#.."], Colour::Yellow),
            Self::Bright => ([".#.#......", "..###.....", ".#####...."], Colour::Yellow, ["...######.", ".#########", "##########"], Colour::White),
            Self::Cloudy => (["...###....", ".######...", "##########"], Colour::White, ["##########", ".########.", ".........."], Colour::White),
            Self::Rain => (["...####...", ".########.", "##########"], Colour::White, [".#..#..#..", "#..#..#...", ".........."], Colour::Cyan),
            Self::Fog => (["##########", "..........", "..########"], Colour::White, ["..........", "########..", ".........."], Colour::White),
        }
    }
}

/// A symbol's half in sixels.
const SYMBOL_SIZE: (usize, usize) = (10, 3);

/// The region's coolest afternoon high, in degrees Celsius.
const COOLEST_HIGH: i64 = 8;
/// The spread of highs from day to day.
const HIGH_SPREAD: u64 = 10;
/// How much cooler the night is than the afternoon, at least...
const NIGHT_DROP: i64 = 4;
/// ...plus up to this much more.
const NIGHT_SPREAD: u64 = 5;
/// The coolest minute of the day, 05:00.
const COOLEST_MINUTE: u64 = 300;
/// The warmest, 15:00.
const WARMEST_MINUTE: u64 = 900;
/// The furthest a town's drift goes from its expected temperature.
const DRIFT_LIMIT: i64 = 2;
/// Days in the outlook.
const OUTLOOK_DAYS: usize = 5;
/// The outlook's heading row.
const OUTLOOK_ROW: usize = 15;
/// The width of each day's column.
const OUTLOOK_WIDTH: usize = 8;

/// A number from a day and a purpose, the same for everyone who asks
/// (`SplitMix64`'s finaliser, Steele, Lea and Flood 2014).
const fn mix(day: u64, purpose: u64) -> u64 {
    let mut value = day.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(purpose);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// What `mix` is asked for.
const PURPOSE_HIGH: u64 = 1;
/// The night's drop.
const PURPOSE_LOW: u64 = 2;
/// The day's weather; a town's is this plus its index.
const PURPOSE_KIND: u64 = 3;

/// A day's forecast.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Forecast {
    /// The weather.
    pub kind: Kind,
    /// The afternoon's high, in degrees Celsius.
    pub high: i64,
    /// The night's low.
    pub low: i64,
}

/// Day `day`'s forecast for the region (days since 1970), or a town's
/// with `place` its index plus one.
#[must_use]
pub fn forecast(day: u64, place: u64) -> Forecast {
    let high = COOLEST_HIGH.saturating_add(i64::try_from(mix(day, PURPOSE_HIGH) % HIGH_SPREAD).unwrap_or(0));
    let drop = NIGHT_DROP.saturating_add(i64::try_from(mix(day, PURPOSE_LOW) % NIGHT_SPREAD).unwrap_or(0));
    let kinds = u64::try_from(KINDS.len()).unwrap_or(1);
    let kind =
        KINDS.get(usize::try_from(mix(day, PURPOSE_KIND.saturating_add(place)).checked_rem(kinds).unwrap_or(0)).unwrap_or(0)).copied().unwrap_or(Kind::Cloudy);
    Forecast { kind, high, low: high.saturating_sub(drop) }
}

/// The temperature between the night's low at 05:00 and the afternoon's
/// high at 15:00 at `minute` of the day: rising for ten hours, falling
/// for fourteen.
#[must_use]
pub fn temperature(day: Forecast, minute: u64) -> i64 {
    let rising = WARMEST_MINUTE.saturating_sub(COOLEST_MINUTE);
    let falling = DAY_MINUTES.saturating_sub(rising);
    let since_coolest = minute.saturating_add(DAY_MINUTES).saturating_sub(COOLEST_MINUTE) % DAY_MINUTES;
    let range = day.high.saturating_sub(day.low);
    let (numerator, denominator) = if since_coolest <= rising { (since_coolest, rising) } else { (DAY_MINUTES.saturating_sub(since_coolest), falling) };
    let numerator = i64::try_from(numerator).unwrap_or(0);
    let denominator = i64::try_from(denominator).unwrap_or(1);
    day.low.saturating_add(range.saturating_mul(numerator).checked_div(denominator).unwrap_or(0))
}

/// The weather: each town's drift, a small random walk on top of the
/// expected temperature.
#[derive(Debug, Default)]
pub struct Weather {
    /// Each town's drift in degrees, within [`DRIFT_LIMIT`].
    pub drift: [i64; TOWNS.len()],
}

impl Weather {
    /// Each town's drift moves by a degree or stays.
    pub fn step(&mut self, context: &mut Context<'_>) {
        for drift in &mut self.drift {
            let change = i64::from(context.below(3)).saturating_sub(1);
            *drift = drift.saturating_add(change).clamp(DRIFT_LIMIT.saturating_neg(), DRIFT_LIMIT);
        }
    }

    /// The page at the Unix time `epoch_ms`.
    #[must_use]
    pub fn screen(&self, epoch_ms: u64) -> Screen {
        let mut screen = page("Weather", Colour::Blue, navigation(WEATHER));
        let today = epoch_ms / DAY_MS;
        let minute = minute_of_day(epoch_ms);
        screen.coloured(FIRST_ROW, 0, Colour::Cyan, &format!("Mosaic Isles, {} UTC", hours_minutes(epoch_ms)));
        right(&mut screen, FIRST_ROW, Colour::White, "Celsius ");
        let mut map = Sixels::new(MAP_SIZE.0, MAP_SIZE.1);
        art(&mut map, 0, 0, &MAP);
        for row in MAP_ROW..MAP_ROW.saturating_add(MAP_ROWS) {
            screen.background(row, 0, Colour::Blue);
        }
        map.draw(&mut screen, MAP_ROW, MAP_COLUMN, Colour::Green);
        for (index, (town, drift)) in TOWNS.iter().zip(self.drift).enumerate() {
            let place = u64::try_from(index).unwrap_or(0).saturating_add(1);
            let day = forecast(today, place);
            let degrees = temperature(day, minute).saturating_add(town.warmth).saturating_add(drift);
            let label = format!("{degrees:>2}");
            // The label in yellow on the sea's blue, then the land again.
            screen.coloured(town.row, town.column, Colour::Yellow, &label);
            screen.code(town.row, town.column.saturating_add(label.len()).saturating_add(1), Colour::Green.mosaic());
            let list_row = MAP_ROW.saturating_add(index.saturating_mul(2));
            screen.coloured(list_row, LIST_COLUMN, Colour::White, town.name);
            screen.coloured(list_row.saturating_add(1), LIST_COLUMN, Colour::Yellow, &label);
            screen.coloured(list_row.saturating_add(1), LIST_WEATHER_COLUMN, Colour::Cyan, day.kind.word());
        }
        screen.coloured(OUTLOOK_ROW, 0, Colour::Yellow, "Five-day outlook for the region");
        for (index, ahead) in (1..).take(OUTLOOK_DAYS).enumerate() {
            let day = today.saturating_add(ahead);
            let outlook = forecast(day, 0);
            let column = index.saturating_mul(OUTLOOK_WIDTH);
            let row = OUTLOOK_ROW.saturating_add(1);
            screen.coloured(row, column, Colour::White, Date::from_days(day).weekday_short());
            let (top, top_colour, bottom, bottom_colour) = outlook.kind.symbol();
            for (half, (lines, colour)) in [(top, top_colour), (bottom, bottom_colour)].into_iter().enumerate() {
                let mut canvas = Sixels::new(SYMBOL_SIZE.0, SYMBOL_SIZE.1);
                art(&mut canvas, 0, 0, &lines);
                canvas.draw(&mut screen, row.saturating_add(1).saturating_add(half), column.saturating_add(2), colour);
            }
            screen.coloured(row.saturating_add(3), column, Colour::Green, outlook.kind.word());
            screen.coloured(row.saturating_add(4), column, Colour::Yellow, &format!("Hi{:>3}", outlook.high));
            screen.coloured(row.saturating_add(5), column, Colour::Cyan, &format!("Lo{:>3}", outlook.low));
        }
        demonstration(&mut screen);
        screen
    }
}

impl Application for Weather {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(WEATHER, "Weather")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        let screen = self.screen(context.epoch_ms());
        publish(context, WEATHER, &screen);
        context.wake_in(INTERVAL_MS.saturating_sub(context.epoch_ms() % INTERVAL_MS));
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        self.step(context);
        let screen = self.screen(context.epoch_ms());
        publish(context, WEATHER, &screen);
        // At the next whole minute of the real time.
        context.wake_in(INTERVAL_MS.saturating_sub(context.epoch_ms() % INTERVAL_MS));
    }
}
