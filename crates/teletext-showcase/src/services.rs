//! Live services (pages 101 to 199): news, weather, markets, sport, the
//! clock (page 105), which shows that a shared page changes once and
//! reaches every viewer.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use teletext_runtime::screen::Colour;
use teletext_runtime::{Application, Context, Owner, PageEntry};

use crate::channels::Picture;
use crate::common::page;
use crate::index::{INDEX, Section};

/// This area's applications.
#[must_use]
pub fn applications(_pictures: &[Picture]) -> Vec<Box<dyn Application>> {
    alloc::vec![Box::new(Clock)]
}

/// This area's entries in the index.
#[must_use]
pub fn sections() -> Vec<Section> {
    alloc::vec![Section { page: CLOCK, name: "Server clock", about: "a live shared page" }]
}

/// The clock's page.
pub const CLOCK: u16 = 0x105;
/// A second, in milliseconds.
const SECOND: u64 = 1_000;

/// A clock of the time since the server started, in hours, minutes and
/// seconds: the host's time, which applications are given.
#[derive(Default)]
pub struct Clock;

impl Clock {
    fn draw(context: &mut Context<'_>) {
        let seconds = context.now() / SECOND;
        let (hours, minutes, seconds) = (seconds / 3_600, (seconds / 60) % 60, seconds % 60);
        let mut screen = page("Server clock", Colour::Magenta, [("Index", Some(INDEX)), ("", None), ("", None), ("", None)]);
        screen.double_height(8, 12, Colour::Yellow, &format!("{hours:02}:{minutes:02}:{seconds:02}"));
        screen.centred(12, Colour::Cyan, "Time since the server started");
        screen.centred(14, Colour::White, "One shared page, changed once a");
        screen.centred(15, Colour::White, "second, sent to every viewer");
        context.publish(Owner::Channel, CLOCK, &screen);
        let next = context.now().saturating_add(SECOND).saturating_sub(context.now() % SECOND);
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
