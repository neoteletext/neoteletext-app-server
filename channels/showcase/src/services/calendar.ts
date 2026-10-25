// The civil calendar from the Unix time, in integer arithmetic
// (crates/teletext-showcase/src/services/calendar.rs), so every viewer and
// every host sees the same day whatever its time zone.

/** Milliseconds in a second. */
export const SECOND_MS = 1_000;
/** Seconds in a minute. */
const MINUTE_SECONDS = 60;
/** Minutes in an hour. */
const HOUR_MINUTES = 60;
/** Milliseconds in a minute. */
export const MINUTE_MS = SECOND_MS * MINUTE_SECONDS;
/** Milliseconds in an hour. */
export const HOUR_MS = MINUTE_MS * HOUR_MINUTES;
/** Hours in a day. */
const DAY_HOURS = 24;
/** Milliseconds in a day: the Unix time counts no leap seconds. */
export const DAY_MS = HOUR_MS * DAY_HOURS;
/** Minutes in a day. */
export const DAY_MINUTES = HOUR_MINUTES * DAY_HOURS;
/** Days in a week. */
const WEEK_DAYS = 7;
/** 1 January 1970 was a Thursday: day 0 is weekday 4, counting Sunday as 0. */
const EPOCH_WEEKDAY = 4;

/** The days of the week from Sunday. */
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"] as const;
/** The months from January. */
const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"] as const;
/** The length of a short name: "Sun", "Jan". */
const SHORT_NAME = 3;

// The constants of Howard Hinnant's `civil_from_days` ("chrono-Compatible
// Low-Level Date Algorithms", 2013): the calendar counted in 400-year eras
// from 1 March of year 0, so the leap day falls at the end of each year.
/** Days from 0000-03-01 to 1970-01-01. */
const EPOCH_FROM_ERA_START = 719_468;
/** Days in a 400-year era. */
const ERA_DAYS = 146_097;
/** Years in an era. */
const ERA_YEARS = 400;
/** Days in four years, the first leap-year correction. */
const FOUR_YEARS_DAYS = 1_460;
/** Days in a century, the second correction. */
const CENTURY_DAYS = 36_524;
/** The last day of an era, the third correction. */
const ERA_LAST_DAY = 146_096;
/** Days in a common year. */
const YEAR_DAYS = 365;
/** Every fourth year is a leap year... */
const LEAP_YEARS = 4;
/** ...except each century's first, the era's fourth excepted. */
const CENTURY_YEARS = 100;
/** The days of five months from March (31 + 30 + 31 + 30 + 31): `(153 * month + 2) / 5` is a month's first day from March. */
const FIVE_MONTHS_DAYS = 153;
/** The rounding term of that formula. */
const MONTH_ROUNDING = 2;
/** Months in five months. */
const FIVE_MONTHS = 5;
/** Months from March to December: a month index below this is in the same calendar year. */
const MARCH_TO_DECEMBER = 10;
/** From a March-based month index to January-based. */
const MARCH = 3;
/** From a March-based index of January or February to its month. */
const MONTHS_TO_JANUARY = 9;

/** A day of the civil (proleptic Gregorian) calendar. */
export class CivilDate {
  readonly year: number;
  /** 1 to 12. */
  readonly month: number;
  /** The day of the month, 1 to 31. */
  readonly day: number;
  /** The day of the week, 0 (Sunday) to 6. */
  readonly weekday: number;

  constructor(year: number, month: number, day: number, weekday: number) {
    this.year = year;
    this.month = month;
    this.day = day;
    this.weekday = weekday;
  }

  /** The day `days` after 1 January 1970 (Hinnant's `civil_from_days`). */
  static fromDays(days: number): CivilDate {
    const shifted = days + EPOCH_FROM_ERA_START;
    const era = Math.floor(shifted / ERA_DAYS);
    const dayOfEra = shifted - era * ERA_DAYS;
    const yearOfEra = Math.floor(
      (dayOfEra - Math.floor(dayOfEra / FOUR_YEARS_DAYS) + Math.floor(dayOfEra / CENTURY_DAYS) - Math.floor(dayOfEra / ERA_LAST_DAY)) / YEAR_DAYS,
    );
    const year = yearOfEra + era * ERA_YEARS;
    const yearStart = YEAR_DAYS * yearOfEra + Math.floor(yearOfEra / LEAP_YEARS) - Math.floor(yearOfEra / CENTURY_YEARS);
    const dayOfYear = dayOfEra - yearStart;
    const monthIndex = Math.floor((FIVE_MONTHS * dayOfYear + MONTH_ROUNDING) / FIVE_MONTHS_DAYS);
    const monthStart = Math.floor((FIVE_MONTHS_DAYS * monthIndex + MONTH_ROUNDING) / FIVE_MONTHS);
    const day = dayOfYear - monthStart + 1;
    const [month, inYear] = monthIndex < MARCH_TO_DECEMBER ? [monthIndex + MARCH, year] : [monthIndex - MONTHS_TO_JANUARY, year + 1];
    return new CivilDate(inYear, month, day, (days + EPOCH_WEEKDAY) % WEEK_DAYS);
  }

  /** The day of the Unix time `epochMs`. */
  static fromEpochMs(epochMs: number): CivilDate {
    return CivilDate.fromDays(Math.floor(epochMs / DAY_MS));
  }

  /** The weekday's name: "Sunday". */
  weekdayName(): string {
    return WEEKDAYS[this.weekday] ?? "";
  }

  /** The weekday's short name: "Sun". */
  weekdayShort(): string {
    return this.weekdayName().slice(0, SHORT_NAME);
  }

  /** The month's name: "January". */
  monthName(): string {
    return MONTHS[this.month - 1] ?? "";
  }

  /** The date in full: "Sunday 25 October 2026". */
  long(): string {
    return `${this.weekdayName()} ${this.day} ${this.monthName()} ${this.year}`;
  }

  /** The date in short: "Sun 25 Oct". */
  short(): string {
    return `${this.weekdayShort()} ${two(this.day)} ${this.monthName().slice(0, SHORT_NAME)}`;
  }
}

/** A number in at least two digits: "07". */
export function two(value: number): string {
  return value.toString().padStart(2, "0");
}

/** The time of day of `epochMs`: hours, minutes and seconds. */
export function timeOfDay(epochMs: number): readonly [hours: number, minutes: number, seconds: number] {
  const seconds = Math.floor((epochMs % DAY_MS) / SECOND_MS);
  const minutes = Math.floor(seconds / MINUTE_SECONDS);
  return [Math.floor(minutes / HOUR_MINUTES), minutes % HOUR_MINUTES, seconds % MINUTE_SECONDS];
}

/** The minute of the day of `epochMs`, 0 to 1439. */
export function minuteOfDay(epochMs: number): number {
  return Math.floor((epochMs % DAY_MS) / MINUTE_MS);
}

/** `epochMs` moved by a UTC offset in minutes, as a local clock shows it; never before the epoch. */
export function local(epochMs: number, offsetMinutes: number): number {
  return Math.max(0, epochMs + offsetMinutes * MINUTE_MS);
}

/** "14:03". */
export function hoursMinutes(epochMs: number): string {
  const [hours, minutes] = timeOfDay(epochMs);
  return `${two(hours)}:${two(minutes)}`;
}

/** "14:03:22". */
export function hoursMinutesSeconds(epochMs: number): string {
  const [hours, minutes, seconds] = timeOfDay(epochMs);
  return `${two(hours)}:${two(minutes)}:${two(seconds)}`;
}
