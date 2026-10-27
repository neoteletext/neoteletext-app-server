// Dates and times as the pages write them: one formatter that every
// component and channel uses, and a component that shows one date.
//
// A pattern is written in the tokens of Moment, Day.js and react-moment
// ("DD MMM YYYY"), the ones most React code knows; text in [brackets] is
// kept as it is. Unlike Intl.DateTimeFormat (react-intl's FormattedDate,
// React Aria's useDateFormatter) the result never depends on the host's
// locale or ICU data, so a page is the same on every server, and every token
// but the unpadded and spelled-out ones keeps one width, so dates line up in
// a list. Times are UTC: one channel serves viewers everywhere, and the
// archives it shows keep their dates in UTC.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { cells, fit } from "../core/text.ts";
import { Colour } from "../kit.ts";

/** A date: a `Date`, milliseconds since the Unix epoch, or ISO 8601 text, which without a zone is UTC and without a time is that calendar day. */
export type DateValue = Date | number | string;

/** The patterns the library and its channels use, by what they look like. */
export const DATE_FORMATS = {
  /** "27 Oct 26" */
  short: "DD MMM YY",
  /** "27 Oct 2026" */
  long: "DD MMM YYYY",
  /** "27.10.26": the narrowest, for a list's column. */
  numeric: "DD.MM.YY",
  /** "2026-10-27": in a table sorted by its text, the order of the days. */
  iso: "YYYY-MM-DD",
  /** "Tue 27 Oct": a teletext service's header. */
  weekday: "ddd DD MMM",
  /** "Tue 27 Oct 14:05": when a story was written. */
  stamp: "ddd DD MMM HH:mm",
  /** "October 2026": a calendar's heading. */
  month: "MMMM YYYY",
  /** "14:05" */
  time: "HH:mm",
  /** "14:05:09" */
  seconds: "HH:mm:ss",
} as const;

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"] as const;
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"] as const;
/** A short name is the first three letters of the whole one. */
const SHORT_NAME = 3;
const CENTURY = 100;

/** Longer tokens first, so "MMMM" is not read as "MM" twice. */
const TOKENS = /\[([^\]]*)\]|YYYY|YY|MMMM|MMM|MM|M|DD|D|dddd|ddd|HH|H|mm|ss/gu;
/** A calendar day alone: that day, not midnight somewhere else. */
const DAY_ONLY = /^\d{4}-\d{2}-\d{2}$/u;
/** A date and time with no zone, as databases keep UTC ("2026-10-27 14:05:09"). */
const NO_ZONE = /^(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?)$/u;

/** The moment `value` names, or null when it names none. */
export function dateOf(value: DateValue): Date | null {
  let epochMs: number;
  if (value instanceof Date) {
    epochMs = value.getTime();
  } else if (typeof value === "number") {
    epochMs = value;
  } else {
    const text = value.trim();
    const local = NO_ZONE.exec(text);
    epochMs = Date.parse(DAY_ONLY.test(text) ? `${text}T00:00:00Z` : local === null ? text : `${local[1] ?? ""}T${local[2] ?? ""}Z`);
  }
  return Number.isFinite(epochMs) ? new Date(epochMs) : null;
}

const two = (value: number): string => String(value).padStart(2, "0");

/** `value` written in `pattern` (UTC), or `fallback` when it is not a date. */
export function formatDate(value: DateValue, pattern: string = DATE_FORMATS.long, fallback = ""): string {
  const date = dateOf(value);
  if (date === null) {
    return fallback;
  }
  const month = MONTHS[date.getUTCMonth()] ?? "";
  const weekday = WEEKDAYS[date.getUTCDay()] ?? "";
  return pattern.replace(TOKENS, (token: string, literal: string | undefined) => {
    if (literal !== undefined) {
      return literal;
    }
    switch (token) {
      case "YYYY":
        return String(date.getUTCFullYear()).padStart(4, "0");
      case "YY":
        return two(date.getUTCFullYear() % CENTURY);
      case "MMMM":
        return month;
      case "MMM":
        return month.slice(0, SHORT_NAME);
      case "MM":
        return two(date.getUTCMonth() + 1);
      case "M":
        return String(date.getUTCMonth() + 1);
      case "DD":
        return two(date.getUTCDate());
      case "D":
        return String(date.getUTCDate());
      case "dddd":
        return weekday;
      case "ddd":
        return weekday.slice(0, SHORT_NAME);
      case "HH":
        return two(date.getUTCHours());
      case "H":
        return String(date.getUTCHours());
      case "mm":
        return two(date.getUTCMinutes());
      default:
        return two(date.getUTCSeconds());
    }
  });
}

export interface DateTextProps {
  readonly value: DateValue;
  /** DATE_FORMATS.long when absent. */
  readonly pattern?: string;
  readonly foreground?: Colour;
  /** What shows when `value` is not a date. */
  readonly fallback?: string;
  /** In the middle of its width. */
  readonly centre?: boolean;
}

/** One date on one row, in its colour. */
export class DateText extends Component<DateTextProps> {
  get text(): string {
    return formatDate(this.props.value, this.props.pattern, this.props.fallback);
  }

  override measure(space: Size): Size {
    return { width: Math.min(space.width, cells(this.text) + 1), height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const shown = fit(this.text, canvas.width - 1);
    const column = this.props.centre === true ? Math.max(0, Math.floor((canvas.width - cells(shown) - 1) / 2)) : 0;
    canvas.coloured(0, column, this.props.foreground ?? Colour.White, shown);
  }
}
