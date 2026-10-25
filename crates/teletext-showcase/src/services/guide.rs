//! Page 107: a TV guide for a made-up channel, Mosaic One, whose day's
//! schedule repeats every day: the programme on now, by the real time,
//! with a bar of how far it has run, and what follows. The page says it is
//! demonstration data.

use alloc::format;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen, Sixels};
use teletext_runtime::{Application, Context, PageEntry};

use super::calendar::{DAY_MINUTES, Date, MINUTE_MS, minute_of_day};
use super::draw::{FIRST_ROW, demonstration, publish, right};
use super::{GUIDE, navigation};
use crate::common::page;

/// The page changes once a minute, with the bar.
const INTERVAL_MS: u64 = MINUTE_MS;
/// Minutes in an hour, for the times shown.
const HOUR_MINUTES: u64 = 60;

/// The day's schedule: each programme's start, in minutes from midnight
/// UTC, and its title. Each runs until the next begins.
const SCHEDULE: [(u64, &str); 20] = [
    (0, "Night Sixels"),
    (360, "Breakfast on Mosaic One"),
    (540, "Carousel Kitchen"),
    (570, "The Magazine Hour"),
    (630, "The Fastext Files"),
    (690, "Parity Gardens"),
    (720, "Lunchtime News"),
    (750, "Pixel Puzzlers"),
    (810, "Film: The Lost Sub-page"),
    (930, "Header Hill Vets"),
    (990, "Raster Club"),
    (1_050, "Quiz: Reveal the Answer"),
    (1_080, "Evening News"),
    (1_110, "Mosaic Isles Tonight"),
    (1_140, "Sixel Street"),
    (1_170, "Hold Mosaics"),
    (1_200, "Contiguous"),
    (1_260, "Late Film: Double Height"),
    (1_380, "Late News"),
    (1_395, "The Separated Sessions"),
];

/// The channel's name.
const CHANNEL: &str = "Mosaic One";
/// The row of the programme on now.
const NOW_ROW: usize = 5;
/// The row of its bar.
const BAR_ROW: usize = 6;
/// The bar's first column, after its colour attribute.
const BAR_COLUMN: usize = 3;
/// The bar's width in sixels: 34 cells.
const BAR_WIDTH: usize = 68;
/// The bar's height in sixels: one row of cells.
const BAR_HEIGHT: usize = 3;
/// The track's sixel row, in the middle of the bar.
const TRACK: usize = 1;
/// The heading of what follows.
const LATER_ROW: usize = 9;
/// How many programmes follow.
const LATER_COUNT: usize = 12;
/// A time's colour attribute's column, leaving two cells for a
/// background.
const TIME_COLUMN: usize = 2;
/// A title's colour attribute's column.
const TITLE_COLUMN: usize = 8;

/// "14:30" for a minute of the day.
fn clock(minute: u64) -> alloc::string::String {
    let minute = minute % DAY_MINUTES;
    format!("{:02}:{:02}", minute / HOUR_MINUTES, minute % HOUR_MINUTES)
}

/// The index of the programme on at `minute` of the day.
#[must_use]
pub fn on_now(minute: u64) -> usize {
    SCHEDULE.iter().rposition(|&(start, _)| start <= minute).unwrap_or(0)
}

/// The programme `index` places after the first of the day, wrapping into
/// the next day: its start and title.
fn programme(index: usize) -> (u64, &'static str) {
    SCHEDULE.get(index.checked_rem(SCHEDULE.len()).unwrap_or(0)).copied().unwrap_or((0, ""))
}

/// The guide; it has no state, the schedule is fixed and the time the
/// host's.
pub struct Guide;

/// The guide's page at the Unix time `epoch_ms`.
#[must_use]
pub fn screen(epoch_ms: u64) -> Screen {
    let mut screen = page("TV guide", Colour::Blue, navigation(GUIDE));
    let date = Date::from_epoch_ms(epoch_ms);
    screen.coloured(FIRST_ROW, 0, Colour::Cyan, CHANNEL);
    let after_name = CHANNEL.len().saturating_add(1);
    screen.coloured(FIRST_ROW, after_name, Colour::White, &format!("{} {} {}", date.weekday_name(), date.day, date.month_name()));
    right(&mut screen, FIRST_ROW, Colour::Green, "Times UTC");
    let minute = minute_of_day(epoch_ms);
    let now = on_now(minute);
    let (start, title) = programme(now);
    let (end, _) = programme(now.saturating_add(1));
    // The last programme runs to midnight, the next day's first start.
    let end = if end <= start { DAY_MINUTES } else { end };
    screen.background(NOW_ROW, 0, Colour::Blue);
    screen.coloured(NOW_ROW, TIME_COLUMN, Colour::Yellow, &clock(start));
    screen.coloured(NOW_ROW, TITLE_COLUMN, Colour::White, title);
    right(&mut screen, NOW_ROW, Colour::Yellow, "NOW ");
    let length = end.saturating_sub(start).max(1);
    let elapsed = minute.saturating_sub(start);
    let filled = usize::try_from(elapsed.saturating_mul(u64::try_from(BAR_WIDTH).unwrap_or(0)).checked_div(length).unwrap_or(0)).unwrap_or(0);
    let mut bar = Sixels::new(BAR_WIDTH, BAR_HEIGHT);
    for x in 0..BAR_WIDTH {
        for y in 0..BAR_HEIGHT {
            bar.set(x, y, x < filled || y == TRACK);
        }
    }
    bar.draw(&mut screen, BAR_ROW, BAR_COLUMN, Colour::Green);
    let left = end.saturating_sub(minute);
    let left = match (left / HOUR_MINUTES, left % HOUR_MINUTES) {
        (0, 1) => alloc::string::String::from("1 minute to go"),
        (0, minutes) => format!("{minutes} minutes to go"),
        (hours, minutes) => format!("{hours} h {minutes:02} min to go"),
    };
    screen.coloured(BAR_ROW.saturating_add(1), TIME_COLUMN, Colour::White, &format!("Ends {}, {left}", clock(end)));
    screen.coloured(LATER_ROW, TIME_COLUMN, Colour::Yellow, "Later");
    for (offset, row) in (1..=LATER_COUNT).zip(LATER_ROW.saturating_add(1)..) {
        let (start, title) = programme(now.saturating_add(offset));
        screen.coloured(row, TIME_COLUMN, Colour::Cyan, &clock(start));
        screen.coloured(row, TITLE_COLUMN, Colour::White, title);
    }
    demonstration(&mut screen);
    screen
}

impl Application for Guide {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(GUIDE, "TV guide")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        self.wake(context);
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        let epoch_ms = context.epoch_ms();
        publish(context, GUIDE, &screen(epoch_ms));
        // At the next whole minute of the real time.
        context.wake_in(INTERVAL_MS.saturating_sub(epoch_ms % INTERVAL_MS));
    }
}
