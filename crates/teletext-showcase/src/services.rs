//! Live services (pages 101 to 199): news, weather, a world clock,
//! markets, the server clock, sport, a TV guide, pages about NeoTeletext
//! and teletext, and a long read that scrolls. Every page but the long
//! read is the channel's: drawn once per change and sent to every viewer
//! (decision 7), each application waking only as often as its page
//! changes. The long read is each reader's own once they scroll.
//!
//! Weather, markets, sport and the TV guide are simulated, with made-up
//! places, companies, teams and programmes, and say so on the page.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Sixels};
use teletext_runtime::{Application, Context, PageEntry};

use crate::channels::Picture;
use crate::common::page;
use crate::index::{INDEX, Section};

pub mod about;
pub mod calendar;
pub mod draw;
pub mod guide;
pub mod long_read;
pub mod markets;
pub mod news;
pub mod sport;
pub mod weather;
pub mod world_clock;

use calendar::SECOND_MS;
use draw::{DIGIT_HEIGHT, digits, publish};

/// The news page.
pub const NEWS: u16 = 0x101;
/// The weather page.
pub const WEATHER: u16 = 0x102;
/// The world clock's page.
pub const WORLD_CLOCK: u16 = 0x103;
/// The markets page.
pub const MARKETS: u16 = 0x104;
/// The server clock's page.
pub const CLOCK: u16 = 0x105;
/// The sport page.
pub const SPORT: u16 = 0x106;
/// The TV guide's page.
pub const GUIDE: u16 = 0x107;
/// The page about NeoTeletext.
pub const ABOUT: u16 = 0x108;
/// The teletext facts page.
pub const FACTS: u16 = 0x109;
/// The long read, scrolled within the page.
pub const LONG_READ: u16 = 0x110;

/// The services in page order, with each one's Fastext label (at most
/// nine characters, the width of a quarter of row 24 after its colour).
const SERVICES: [(u16, &str); 10] = [
    (NEWS, "News"),
    (WEATHER, "Weather"),
    (WORLD_CLOCK, "World"),
    (MARKETS, "Markets"),
    (CLOCK, "Clock"),
    (SPORT, "Sport"),
    (GUIDE, "TV guide"),
    (ABOUT, "About"),
    (FACTS, "Facts"),
    (LONG_READ, "Long read"),
];

/// The yellow link's choices, in order: the first that is not the page
/// itself nor already red or green.
const YELLOW_CHOICES: [(u16, &str); 3] = [(NEWS, "News"), (ABOUT, "About"), (WEATHER, "Weather")];

/// A service page's Fastext: red the service before, green the one after
/// (wrapping round), yellow the news (or the next choice where the news
/// is already linked), and cyan the index.
#[must_use]
pub fn navigation(number: u16) -> [(&'static str, Option<u16>); 4] {
    let position = SERVICES.iter().position(|&(page, _)| page == number).unwrap_or(0);
    let count = SERVICES.len();
    let before = SERVICES.get(position.checked_sub(1).unwrap_or(count.saturating_sub(1))).copied();
    let after = SERVICES.get(position.saturating_add(1).checked_rem(count).unwrap_or(0)).copied();
    let taken = |page: u16| page == number || before.is_some_and(|(other, _)| other == page) || after.is_some_and(|(other, _)| other == page);
    let third = YELLOW_CHOICES.iter().copied().find(|&(page, _)| !taken(page));
    let link = |service: Option<(u16, &'static str)>| service.map_or(("", None), |(page, label)| (label, Some(page)));
    [link(before), link(after), link(third), ("Index", Some(INDEX))]
}

/// This area's applications.
#[must_use]
pub fn applications(_pictures: &[Picture]) -> Vec<Box<dyn Application>> {
    alloc::vec![
        Box::new(news::News),
        Box::new(weather::Weather::default()),
        Box::new(world_clock::WorldClock),
        Box::new(markets::Markets::default()),
        Box::new(Clock),
        Box::new(sport::Sport::default()),
        Box::new(guide::Guide),
        Box::new(about::About),
        Box::new(long_read::LongRead::default()),
    ]
}

/// This area's entries in the index: three, each naming the pages it
/// leads to, so the index keeps room for the other areas.
#[must_use]
pub fn sections() -> Vec<Section> {
    alloc::vec![
        Section { page: NEWS, name: "News and live services", about: "weather 102 markets 104 sport 106" },
        Section { page: WORLD_CLOCK, name: "Clocks and TV guide", about: "world 103, server 105, TV 107" },
        Section { page: ABOUT, name: "About NeoTeletext", about: "facts 109 and a long read on 110" },
    ]
}

/// The server clock's digits: one font pixel to a sixel.
const CLOCK_SCALE: usize = 1;
/// The canvas the time is drawn on, in sixels: "HH:MM:SS" is six digits
/// six sixels wide and two colons two wide.
const CLOCK_CANVAS_WIDTH: usize = 40;
/// The digits' first column: the canvas's 20 cells centred.
const CLOCK_COLUMN: usize = 10;
/// The digits' first row.
const CLOCK_ROW: usize = 9;
/// Seconds in a minute.
const MINUTE_SECONDS: u64 = 60;
/// Seconds in an hour.
const HOUR_SECONDS: u64 = 3_600;
/// Seconds in a day.
const DAY_SECONDS: u64 = 86_400;

/// A clock of the time since the server started, in hours, minutes and
/// seconds: the host's time, which applications are given.
#[derive(Default)]
pub struct Clock;

impl Clock {
    fn draw(context: &mut Context<'_>) {
        let seconds = context.now() / SECOND_MS;
        let days = seconds / DAY_SECONDS;
        let hours = (seconds % DAY_SECONDS) / HOUR_SECONDS;
        let minutes = (seconds / MINUTE_SECONDS) % MINUTE_SECONDS;
        let seconds = seconds % MINUTE_SECONDS;
        let mut screen = page("Server clock", Colour::Magenta, navigation(CLOCK));
        screen.centred(6, Colour::Cyan, "Time since the server started");
        let mut canvas = Sixels::new(CLOCK_CANVAS_WIDTH, DIGIT_HEIGHT);
        digits(&mut canvas, 0, 0, &format!("{hours:02}:{minutes:02}:{seconds:02}"), CLOCK_SCALE);
        canvas.draw(&mut screen, CLOCK_ROW, CLOCK_COLUMN, Colour::Yellow);
        let days = match days {
            0 => String::from("hours, minutes and seconds"),
            1 => String::from("and 1 day"),
            more => format!("and {more} days"),
        };
        screen.centred(13, Colour::Yellow, &days);
        screen.centred(17, Colour::White, "One shared page, changed once a");
        screen.centred(18, Colour::White, "second, sent to every viewer");
        publish(context, CLOCK, &screen);
        let next = context.now().saturating_add(SECOND_MS).saturating_sub(context.now() % SECOND_MS);
        context.wake_at(next);
    }
}

impl Application for Clock {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(CLOCK, "Server clock")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        Self::draw(context);
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        Self::draw(context);
    }
}

#[cfg(test)]
mod tests;
