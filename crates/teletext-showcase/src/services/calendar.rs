//! The civil calendar from the Unix time, in integer arithmetic, so it
//! runs on every host without a date library or floating point.

use alloc::format;
use alloc::string::String;

/// Milliseconds in a second.
pub const SECOND_MS: u64 = 1_000;
/// Seconds in a minute.
const MINUTE_SECONDS: u64 = 60;
/// Minutes in an hour.
const HOUR_MINUTES: u64 = 60;
/// Milliseconds in a minute.
pub const MINUTE_MS: u64 = SECOND_MS * MINUTE_SECONDS;
/// Milliseconds in an hour.
pub const HOUR_MS: u64 = MINUTE_MS * HOUR_MINUTES;
/// Hours in a day.
const DAY_HOURS: u64 = 24;
/// Milliseconds in a day: the Unix time counts no leap seconds.
pub const DAY_MS: u64 = HOUR_MS * DAY_HOURS;
/// Minutes in a day.
pub const DAY_MINUTES: u64 = HOUR_MINUTES * DAY_HOURS;
/// Days in a week.
const WEEK_DAYS: u64 = 7;
/// 1 January 1970 was a Thursday: day 0 is weekday 4, counting Sunday as
/// 0.
const EPOCH_WEEKDAY: u64 = 4;

/// The days of the week from Sunday.
const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
/// The months from January.
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
/// The length of a short name: "Sun", "Jan".
const SHORT_NAME: usize = 3;

// The constants of Howard Hinnant's `civil_from_days` ("chrono-Compatible
// Low-Level Date Algorithms", 2013): the calendar counted in 400-year eras
// from 1 March of year 0, so the leap day falls at the end of each year.
/// Days from 0000-03-01 to 1970-01-01.
const EPOCH_FROM_ERA_START: u64 = 719_468;
/// Days in a 400-year era.
const ERA_DAYS: u64 = 146_097;
/// Years in an era.
const ERA_YEARS: u64 = 400;
/// Days in four years, the first leap-year correction.
const FOUR_YEARS_DAYS: u64 = 1_460;
/// Days in a century, the second correction.
const CENTURY_DAYS: u64 = 36_524;
/// The last day of an era, the third correction.
const ERA_LAST_DAY: u64 = 146_096;
/// Days in a common year.
const YEAR_DAYS: u64 = 365;
/// Every fourth year is a leap year...
const LEAP_YEARS: u64 = 4;
/// ...except each century's first, the era's fourth excepted.
const CENTURY_YEARS: u64 = 100;
/// The days of five months from March (31 + 30 + 31 + 30 + 31): the
/// months' lengths repeat in that pattern, so `(153 * month + 2) / 5` is
/// the first day of a month counted from March.
const FIVE_MONTHS_DAYS: u64 = 153;
/// The rounding term of that formula.
const MONTH_ROUNDING: u64 = 2;
/// Months in five months.
const FIVE_MONTHS: u64 = 5;
/// Months from March to December: a month index below this is in the
/// same calendar year.
const MARCH_TO_DECEMBER: u64 = 10;
/// From a March-based month index to January-based.
const MARCH: u64 = 3;
/// From a March-based index of January or February to its month.
const MONTHS_TO_JANUARY: u64 = 9;

/// A day of the civil (proleptic Gregorian) calendar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Date {
    /// The year.
    pub year: u64,
    /// The month, 1 to 12.
    pub month: u64,
    /// The day of the month, 1 to 31.
    pub day: u64,
    /// The day of the week, 0 (Sunday) to 6.
    pub weekday: u64,
}

impl Date {
    /// The day `days` after 1 January 1970 (Hinnant's `civil_from_days`).
    #[must_use]
    pub const fn from_days(days: u64) -> Self {
        let shifted = days.saturating_add(EPOCH_FROM_ERA_START);
        let era = shifted / ERA_DAYS;
        let day_of_era = shifted.saturating_sub(era.saturating_mul(ERA_DAYS));
        let year_of_era =
            day_of_era.saturating_sub(day_of_era / FOUR_YEARS_DAYS).saturating_add(day_of_era / CENTURY_DAYS).saturating_sub(day_of_era / ERA_LAST_DAY)
                / YEAR_DAYS;
        let year = year_of_era.saturating_add(era.saturating_mul(ERA_YEARS));
        let year_start = YEAR_DAYS.saturating_mul(year_of_era).saturating_add(year_of_era / LEAP_YEARS).saturating_sub(year_of_era / CENTURY_YEARS);
        let day_of_year = day_of_era.saturating_sub(year_start);
        let month_index = FIVE_MONTHS.saturating_mul(day_of_year).saturating_add(MONTH_ROUNDING) / FIVE_MONTHS_DAYS;
        let month_start = FIVE_MONTHS_DAYS.saturating_mul(month_index).saturating_add(MONTH_ROUNDING) / FIVE_MONTHS;
        let day = day_of_year.saturating_sub(month_start).saturating_add(1);
        let (month, year) = if month_index < MARCH_TO_DECEMBER {
            (month_index.saturating_add(MARCH), year)
        } else {
            (month_index.saturating_sub(MONTHS_TO_JANUARY), year.saturating_add(1))
        };
        Self { year, month, day, weekday: days.saturating_add(EPOCH_WEEKDAY) % WEEK_DAYS }
    }

    /// The day of the Unix time `epoch_ms`.
    #[must_use]
    pub const fn from_epoch_ms(epoch_ms: u64) -> Self {
        Self::from_days(epoch_ms / DAY_MS)
    }

    /// The weekday's name: "Sunday".
    #[must_use]
    pub fn weekday_name(self) -> &'static str {
        WEEKDAYS.get(usize::try_from(self.weekday).unwrap_or(0)).copied().unwrap_or("")
    }

    /// The weekday's short name: "Sun".
    #[must_use]
    pub fn weekday_short(self) -> &'static str {
        short(self.weekday_name())
    }

    /// The month's name: "January".
    #[must_use]
    pub fn month_name(self) -> &'static str {
        MONTHS.get(usize::try_from(self.month.saturating_sub(1)).unwrap_or(0)).copied().unwrap_or("")
    }

    /// The date in full: "Sunday 25 October 2026".
    #[must_use]
    pub fn long(self) -> String {
        format!("{} {} {} {}", self.weekday_name(), self.day, self.month_name(), self.year)
    }

    /// The date in short: "Sun 25 Oct".
    #[must_use]
    pub fn short(self) -> String {
        format!("{} {:02} {}", self.weekday_short(), self.day, short(self.month_name()))
    }
}

/// A name's first three letters.
fn short(name: &'static str) -> &'static str {
    name.get(..SHORT_NAME).unwrap_or(name)
}

/// The time of day of `epoch_ms`: hours, minutes and seconds.
#[must_use]
pub const fn time_of_day(epoch_ms: u64) -> (u64, u64, u64) {
    let seconds = (epoch_ms % DAY_MS) / SECOND_MS;
    let minutes = seconds / MINUTE_SECONDS;
    (minutes / HOUR_MINUTES, minutes % HOUR_MINUTES, seconds % MINUTE_SECONDS)
}

/// The minute of the day of `epoch_ms`, 0 to 1439.
#[must_use]
pub const fn minute_of_day(epoch_ms: u64) -> u64 {
    (epoch_ms % DAY_MS) / MINUTE_MS
}

/// `epoch_ms` moved by a UTC offset in minutes, as a local clock shows it.
#[must_use]
pub const fn local(epoch_ms: u64, offset_minutes: i64) -> u64 {
    let offset_ms = offset_minutes.saturating_mul(MINUTE_MS.cast_signed());
    epoch_ms.saturating_add_signed(offset_ms)
}

/// "14:03".
#[must_use]
pub fn hours_minutes(epoch_ms: u64) -> String {
    let (hours, minutes, _) = time_of_day(epoch_ms);
    format!("{hours:02}:{minutes:02}")
}

/// "14:03:22".
#[must_use]
pub fn hours_minutes_seconds(epoch_ms: u64) -> String {
    let (hours, minutes, seconds) = time_of_day(epoch_ms);
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
