//! Row 0's header, as a teletext inserter writes it (EN 300 706 clause
//! 9.3.1): the service's name, the page's number, the date and a clock,
//! the same on every page, so a receiver searching for a page shows them
//! rolling and keeps its clock from them. Columns 0 to 7 are the
//! receiver's own (the engine's decision E.10); a channel's program does
//! not write the header, the runtime does, at each page's publication, and
//! the host sends a time-filling header every second so that the clock on
//! a page that does not change keeps time (the engine's decision E.121).
//! The time is UTC: the runtime has no time zones.

use neoteletext_core::consts::{C11_SERIAL, CLOCK_BYTES, CLOCK_COLUMN, HEADER_DISPLAY_BYTES, HEADER_DISPLAY_COLUMN, PAGE_TIME_FILLING, SPACE};
use neoteletext_core::ecc::with_odd_parity;
use neoteletext_core::packet::encode;
use neoteletext_core::types::{ControlBits, Magazine, PageNumber, SubCode};
use neoteletext_formats::page::PageFile;

/// The header's fields, as columns 8 to 39 hold them: the name, the page
/// number, the date and the clock, a space between each.
const NAME_COLUMNS: usize = 8;
const NUMBER_COLUMNS: usize = 3;
const DATE_COLUMNS: usize = 10;
const _: () = assert!(NAME_COLUMNS + 1 + NUMBER_COLUMNS + 1 + DATE_COLUMNS + 1 + CLOCK_BYTES == HEADER_DISPLAY_BYTES);
const _: () = assert!(CLOCK_COLUMN - HEADER_DISPLAY_COLUMN == NAME_COLUMNS + 1 + NUMBER_COLUMNS + 1 + DATE_COLUMNS + 1);
/// The magazine a time-filling header goes in: any will do (clause 9.3.1.1).
const FILLING_MAGAZINE: u8 = 1;

/// Milliseconds, seconds, minutes and hours in the units above them.
const MS_PER_SECOND: u64 = 1_000;
const SECONDS_PER_DAY: u64 = 86_400;
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_MINUTE: u64 = 60;
/// 1 January 1970, the epoch, was a Thursday: day 4 counting Sunday as 0.
const EPOCH_WEEKDAY: u64 = 4;
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The header's 32 characters for `page`, or for a time-filling header with
/// none, at `epoch_ms`: such as `HACKADAY 100 Mon 26 Oct 23:15/42`.
#[must_use]
pub fn header_text(name: &str, page: Option<u16>, epoch_ms: u64) -> [u8; HEADER_DISPLAY_BYTES] {
    let mut text = [SPACE; HEADER_DISPLAY_BYTES];
    let mut put = |at: usize, field: &[u8]| {
        for (slot, &byte) in text.iter_mut().skip(at).zip(field) {
            *slot = byte;
        }
    };
    let upper: alloc::vec::Vec<u8> =
        name.bytes().filter(u8::is_ascii_graphic).chain(core::iter::repeat(SPACE)).take(NAME_COLUMNS).map(|byte| byte.to_ascii_uppercase()).collect();
    put(0, &upper);
    if let Some(page) = page {
        put(NAME_COLUMNS + 1, alloc::format!("{page:03X}").as_bytes());
    }
    let (date, clock) = date_and_clock(epoch_ms);
    put(NAME_COLUMNS + 1 + NUMBER_COLUMNS + 1, date.as_bytes());
    put(CLOCK_COLUMN - HEADER_DISPLAY_COLUMN, clock.as_bytes());
    text
}

/// Writes `page`'s header, as [`header_text`] has it, into its row 0.
pub fn stamp(page: &mut PageFile, name: &str, epoch_ms: u64) {
    let number = page.address.mpp();
    let mut row = page.row(0).copied().unwrap_or([SPACE; 40]);
    for (slot, &byte) in row.iter_mut().skip(HEADER_DISPLAY_COLUMN).zip(header_text(name, Some(number), epoch_ms).iter()) {
        *slot = byte;
    }
    page.set_row(0, &row);
}

/// A time-filling header (page FF, clause 9.3.1.1) at `epoch_ms`: its clock
/// keeps a receiver's time, and it starts no page.
#[must_use]
pub fn time_filling(name: &str, epoch_ms: u64) -> [u8; 42] {
    let text = header_text(name, None, epoch_ms);
    let mut display = [with_odd_parity(SPACE); HEADER_DISPLAY_BYTES];
    for (slot, &byte) in display.iter_mut().zip(text.iter()) {
        *slot = with_odd_parity(byte);
    }
    let magazine = Magazine::from_bits(FILLING_MAGAZINE);
    encode::header(magazine, PageNumber(PAGE_TIME_FILLING), SubCode::new(0), ControlBits(C11_SERIAL), &display).bytes
}

/// The date as `Mon 26 Oct` and the clock as `23:15/42`, UTC.
fn date_and_clock(epoch_ms: u64) -> (alloc::string::String, alloc::string::String) {
    let seconds = epoch_ms / MS_PER_SECOND;
    let days = seconds / SECONDS_PER_DAY;
    let of_day = seconds % SECONDS_PER_DAY;
    let (_, month, day) = civil(days);
    let weekday = DAYS.get(usize::try_from(days.saturating_add(EPOCH_WEEKDAY) % 7).unwrap_or(0)).copied().unwrap_or("");
    let month = MONTHS.get(month.saturating_sub(1)).copied().unwrap_or("");
    let date = alloc::format!("{weekday} {day:02} {month}");
    let clock = alloc::format!("{:02}:{:02}/{:02}", of_day / SECONDS_PER_HOUR, of_day % SECONDS_PER_HOUR / SECONDS_PER_MINUTE, of_day % SECONDS_PER_MINUTE);
    (date, clock)
}

/// The year, month (1 to 12) and day of `days` after the epoch: Howard
/// Hinnant's `civil_from_days`, for the proleptic Gregorian calendar.
/// Every step stays within `u64` for any day a clock can give.
fn civil(days: u64) -> (u64, usize, u64) {
    /// Days in a 400-year era, and the epoch's place in its era (1 March 0000 to 1 January 1970).
    const ERA_DAYS: u64 = 146_097;
    const EPOCH_SHIFT: u64 = 719_468;
    let shifted = days.saturating_add(EPOCH_SHIFT);
    let era = shifted / ERA_DAYS;
    let of_era = shifted % ERA_DAYS;
    let year_of_era = (of_era.saturating_sub(of_era / 1_460).saturating_add(of_era / 36_524).saturating_sub(of_era / 146_096)) / 365;
    let year_start = year_of_era.saturating_mul(365).saturating_add(year_of_era / 4).saturating_sub(year_of_era / 100);
    let of_year = of_era.saturating_sub(year_start);
    let month_index = of_year.saturating_mul(5).saturating_add(2) / 153;
    let day = of_year.saturating_sub(month_index.saturating_mul(153).saturating_add(2) / 5).saturating_add(1);
    let month = if month_index < 10 { month_index.saturating_add(3) } else { month_index.saturating_sub(9) };
    let year = year_of_era.saturating_add(era.saturating_mul(400)).saturating_add(u64::from(month <= 2));
    (year, usize::try_from(month).unwrap_or(1), day)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 26 October 2026 23:15:42 UTC, a Monday.
    const WHEN_MS: u64 = 1_793_056_542_000;

    #[test]
    fn the_header_is_name_number_date_and_clock() {
        assert_eq!(&header_text("Hackaday", Some(0x100), WHEN_MS), b"HACKADAY 100 Mon 26 Oct 23:15/42");
        assert_eq!(&header_text("NeoTeletext showcase", Some(0x8FF), WHEN_MS)[..12], b"NEOTELET 8FF", "the name cut to its place");
        assert_eq!(&header_text("Hackaday", None, WHEN_MS)[8..12], b"    ", "a time-filling header names no page");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(11_016), (2000, 2, 29), "a leap day");
    }

    #[test]
    fn a_time_filling_header_is_page_ff_with_the_clock() {
        let bytes = time_filling("Hackaday", WHEN_MS);
        let mut buffer = [0_u8; 42];
        buffer.copy_from_slice(&bytes);
        let packet = neoteletext_core::packet::Packet { bytes: buffer };
        let decoded = neoteletext_core::packet::decode(&packet).unwrap();
        let neoteletext_core::packet::Decoded::Header(header) = decoded else { panic!("{decoded:?}") };
        assert!(header.page.is_some_and(PageNumber::is_time_filling));
        let clock: alloc::vec::Vec<u8> = header.display[24..].iter().map(|byte| byte & 0x7F).collect();
        assert_eq!(clock, b"23:15/42");
    }
}
