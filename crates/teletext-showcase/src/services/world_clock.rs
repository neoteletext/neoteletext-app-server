//! Page 103: the world clock, the real time from the host's clock: UTC in
//! large mosaic digits, the date, and twelve cities at their standard
//! time. Daylight saving is not applied, and the page says so: its rules
//! change by law, and a fixed table would go wrong without notice.

use alloc::format;
use alloc::vec::Vec;

use neoteletext_core::consts::COLUMNS;
use teletext_runtime::screen::{Colour, Screen, Sixels};
use teletext_runtime::{Application, Context, PageEntry};

use super::calendar::{Date, SECOND_MS, hours_minutes, local, time_of_day};
use super::draw::{DIGIT_HEIGHT, digits, publish};
use super::{WORLD_CLOCK, navigation};
use crate::common::page;

/// The cities and their standard time's offset from UTC, in minutes.
const CITIES: [(&str, i64); 12] = [
    ("Honolulu", -600),
    ("Vancouver", -480),
    ("New York", -300),
    ("Sao Paulo", -180),
    ("London", 0),
    ("Lagos", 60),
    ("Moscow", 180),
    ("Dubai", 240),
    ("Mumbai", 330),
    ("Singapore", 480),
    ("Tokyo", 540),
    ("Sydney", 600),
];

/// The hours and minutes' font pixels, in sixels each way.
const LARGE: usize = 2;
/// The seconds' font pixels.
const SMALL: usize = 1;
/// The large digits' canvas: "HH:MM" is four digits of six pixels and a
/// colon of two, at two sixels a pixel; the digits' height likewise.
const LARGE_CANVAS: (usize, usize) = (52, DIGIT_HEIGHT * LARGE);
/// The seconds' canvas: two digits of six pixels, as tall as the large
/// digits so its rows line up with theirs.
const SMALL_CANVAS: (usize, usize) = (12, DIGIT_HEIGHT * LARGE);
/// The first column of the large digits, which centres the two canvases
/// and the attribute between them (26 + 1 + 6 cells).
const LARGE_COLUMN: usize = 4;
/// The first column of the seconds: after the large digits' 26 cells and
/// the seconds' colour attribute.
const SMALL_COLUMN: usize = 31;
/// The digits' first row.
const DIGITS_ROW: usize = 5;
/// The date's row, double height.
const DATE_ROW: usize = 11;
/// The cities' first row.
const CITIES_ROW: usize = 14;
/// The cities in each of the two columns.
const CITIES_DOWN: usize = 6;
/// A city column's width: the name's colour and nine characters, the
/// time's colour and five, the day's colour and three.
const CITY_COLUMN_WIDTH: usize = 20;
/// Within a city column: where the time's colour attribute goes.
const TIME_OFFSET: usize = 10;
/// Within a city column: where the day's colour attribute goes.
const DAY_OFFSET: usize = 16;
/// The note's first row.
const NOTE_ROW: usize = 21;

/// The world clock: it has no state, the time is the host's.
pub struct WorldClock;

/// The world clock's page at the Unix time `epoch_ms`.
#[must_use]
pub fn screen(epoch_ms: u64) -> Screen {
    let mut screen = page("World clock", Colour::Blue, navigation(WORLD_CLOCK));
    screen.centred(3, Colour::Cyan, "Coordinated Universal Time");
    let (_, _, seconds) = time_of_day(epoch_ms);
    let mut large = Sixels::new(LARGE_CANVAS.0, LARGE_CANVAS.1);
    digits(&mut large, 0, 0, &hours_minutes(epoch_ms), LARGE);
    large.draw(&mut screen, DIGITS_ROW, LARGE_COLUMN, Colour::Yellow);
    let mut small = Sixels::new(SMALL_CANVAS.0, SMALL_CANVAS.1);
    // The seconds sit on the large digits' baseline.
    digits(&mut small, 0, LARGE_CANVAS.1.saturating_sub(DIGIT_HEIGHT), &format!("{seconds:02}"), SMALL);
    small.draw(&mut screen, DIGITS_ROW, SMALL_COLUMN, Colour::White);
    let date = Date::from_epoch_ms(epoch_ms).long();
    // The double-height and colour attributes take two cells.
    let width = date.len().saturating_add(2);
    screen.double_height(DATE_ROW, COLUMNS.saturating_sub(width) / 2, Colour::White, &date);
    for (index, &(city, offset)) in CITIES.iter().enumerate() {
        let row = CITIES_ROW.saturating_add(index % CITIES_DOWN);
        let column = CITY_COLUMN_WIDTH.saturating_mul(index / CITIES_DOWN);
        let there = local(epoch_ms, offset);
        screen.coloured(row, column, Colour::White, city);
        screen.coloured(row, column.saturating_add(TIME_OFFSET), Colour::Yellow, &hours_minutes(there));
        screen.coloured(row, column.saturating_add(DAY_OFFSET), Colour::Cyan, Date::from_epoch_ms(there).weekday_short());
    }
    screen.centred(NOTE_ROW, Colour::Green, "Standard times: daylight saving");
    screen.centred(NOTE_ROW.saturating_add(1), Colour::Green, "is not applied");
    screen
}

impl Application for WorldClock {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(WORLD_CLOCK, "World clock")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        self.wake(context);
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        let epoch_ms = context.epoch_ms();
        publish(context, WORLD_CLOCK, &screen(epoch_ms));
        // At the next whole second of the real time.
        context.wake_in(SECOND_MS.saturating_sub(epoch_ms % SECOND_MS));
    }
}
