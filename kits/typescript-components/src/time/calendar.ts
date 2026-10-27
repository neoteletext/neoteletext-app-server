// A month: its name, the days of the week, and its days in weeks from
// Monday. A day takes four cells: its colour, a cell for Flash or Steady,
// and its two digits, so a day can flash without moving the others. Today
// is yellow, days with something on are green, the weekend cyan; the day
// chosen flashes. Chosen with the arrows (a day, a week), Page Up and Page
// Down (a month), Enter or a click; a click on an arrow of the heading
// turns the month.
//
// Every day can be clicked, but only the arrows and the days of the chosen
// week are controls: a month's 31 days and two arrows are more than the 32
// a whole page may declare (core/controls.ts), and a calendar that took
// them all would leave none for the page's buttons and lists. The chosen
// week follows the keys and every click, so the hand and the looks are
// where the viewer is choosing; nine controls at most.

import type { Canvas } from "../core/canvas.ts";
import type { Size } from "../core/component.ts";
import type { PageNeeds } from "../core/needs.ts";
import { Control, type ControlProps } from "../input/control.ts";
import { isPress, type KeyEvent, type PointerEvent } from "../core/events.ts";
import { cells } from "../core/text.ts";
import { Attribute, Colour, Key, textColour } from "../kit.ts";
import { DATE_FORMATS, formatDate } from "./date-text.ts";

export interface CalendarDate {
  readonly year: number;
  /** 0 for January. */
  readonly month: number;
  readonly day: number;
}

export interface CalendarProps extends ControlProps {
  readonly year: number;
  readonly month: number;
  /** Today, UTC, in milliseconds since the Unix epoch. */
  readonly today?: number;
  /** Days with something on, in any month. */
  readonly marked?: readonly CalendarDate[];
  /** A day can be chosen. */
  readonly selectable?: boolean;
  readonly onChoose?: (date: CalendarDate, session: number) => void;
}

interface CalendarState {
  readonly year: number;
  readonly month: number;
  readonly day: number;
}

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] as const;
const DAYS_IN_WEEK = 7;
/** A day's cells: its colour, Flash or Steady, two digits. */
const DAY_CELLS = 4;
/** The rows over the days: the month and the days of the week. */
const HEAD_ROWS = 2;
/** The most weeks a month spans. */
const MOST_WEEKS = 6;
/** Saturday and Sunday, from Monday. */
const WEEKEND = 5;
const MONTHS_IN_YEAR = 12;
/** The heading's arrows, each with the space beside it: two cells to aim at rather than one. */
const ARROW_CELLS = 2;
/** A day's cells a click takes: Flash or Steady and its two digits, after its colour's cell. */
const DAY_TARGET = DAY_CELLS - 1;
/** The regions: the arrows, and each day by its number. */
const BACK = "back";
const ON = "on";
const DAY = "day-";

function daysIn(year: number, month: number): number {
  return new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
}

/** The place in the week of the month's first day, Monday 0. */
function firstWeekday(year: number, month: number): number {
  return (new Date(Date.UTC(year, month, 1)).getUTCDay() + DAYS_IN_WEEK - 1) % DAYS_IN_WEEK;
}

export class Calendar extends Control<CalendarProps, CalendarState> {
  override state: CalendarState = { year: this.props.year, month: this.props.month, day: 1 };

  override focusable(): boolean {
    return this.props.selectable === true && !this.disabled;
  }

  /** One to read is the same for everyone; one to choose in is each viewer's. */
  override needs(): PageNeeds {
    return { ownCopy: this.props.selectable === true && this.props.shared !== true };
  }

  protected override keysHint(): string {
    return "Arrows a day or week, PgUp PgDn a month, Enter";
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, HEAD_ROWS + MOST_WEEKS) };
  }

  override paint(canvas: Canvas): void {
    const { year, month } = this.state;
    const width = DAYS_IN_WEEK * DAY_CELLS;
    const title = formatDate(Date.UTC(year, month, 1), DATE_FORMATS.month);
    const choosing = this.focusable();
    const heading = this.props.selectable === true ? `← ${title} →` : title;
    const headingAt = Math.max(0, Math.floor((width - cells(heading)) / 2));
    canvas.coloured(0, headingAt, Colour.Yellow, heading);
    if (choosing) {
      // After the heading's colour's cell: "← " at its start and " →" at its end.
      canvas.control(0, headingAt + 1, ARROW_CELLS, 1, BACK);
      canvas.control(0, headingAt + 1 + cells(heading) - ARROW_CELLS, ARROW_CELLS, 1, ON);
    }
    WEEKDAYS.forEach((name, index) => canvas.coloured(1, index * DAY_CELLS, index >= WEEKEND ? Colour.Cyan : Colour.White, ` ${name}`));
    const today = this.props.today === undefined ? null : new Date(this.props.today);
    const first = firstWeekday(year, month);
    const chosenWeek = Math.floor((first + this.state.day - 1) / DAYS_IN_WEEK);
    for (let day = 1; day <= daysIn(year, month); day += 1) {
      const place = first + day - 1;
      const row = HEAD_ROWS + Math.floor(place / DAYS_IN_WEEK);
      const weekday = place % DAYS_IN_WEEK;
      const isToday = today !== null && today.getUTCFullYear() === year && today.getUTCMonth() === month && today.getUTCDate() === day;
      const marked = this.props.marked?.some((date) => date.year === year && date.month === month && date.day === day) === true;
      const chosen = this.props.selectable === true && day === this.state.day;
      const foreground = chosen || isToday ? Colour.Yellow : marked ? Colour.Green : weekday >= WEEKEND ? Colour.Cyan : Colour.White;
      canvas.codes(row, weekday * DAY_CELLS, [textColour(foreground), chosen && this.focused ? Attribute.Flash : Attribute.Steady]);
      canvas.text(row, weekday * DAY_CELLS + 2, day.toString().padStart(2, " "));
      if (choosing && Math.floor(place / DAYS_IN_WEEK) === chosenWeek) {
        canvas.control(row, weekday * DAY_CELLS + 1, DAY_TARGET, 1, `${DAY}${day}`);
      } else if (choosing) {
        canvas.region(`${DAY}${day}`, row, weekday * DAY_CELLS + 1, DAY_TARGET);
      }
    }
  }

  #move(days: number): void {
    const { year, month, day } = this.state;
    const date = new Date(Date.UTC(year, month, day + days));
    this.setState({ year: date.getUTCFullYear(), month: date.getUTCMonth(), day: date.getUTCDate() });
  }

  #turn(months: number): void {
    const { year, month, day } = this.state;
    const total = year * MONTHS_IN_YEAR + month + months;
    const next = { year: Math.floor(total / MONTHS_IN_YEAR), month: total % MONTHS_IN_YEAR };
    this.setState({ ...next, day: Math.min(day, daysIn(next.year, next.month)) });
  }

  override onKey(event: KeyEvent): boolean {
    if (!this.focusable()) {
      return false;
    }
    const moves: Record<string, () => void> = {
      [Key.Left]: () => this.#move(-1),
      [Key.Right]: () => this.#move(1),
      [Key.Up]: () => this.#move(-DAYS_IN_WEEK),
      [Key.Down]: () => this.#move(DAYS_IN_WEEK),
      [Key.PageUp]: () => this.#turn(-1),
      [Key.PageDown]: () => this.#turn(1),
    };
    if (isPress(event, Key.Enter)) {
      this.activate({ ...this.state }, event.session);
      this.props.onChoose?.({ ...this.state }, event.session);
      return true;
    }
    const move = event.action === "release" ? undefined : moves[event.key];
    if (move === undefined) {
      return false;
    }
    move();
    return true;
  }

  override onClick(event: PointerEvent): boolean {
    const region = event.region ?? "";
    if (region === BACK || region === ON) {
      this.#turn(region === BACK ? -1 : 1);
      return true;
    }
    if (!region.startsWith(DAY)) {
      return false;
    }
    const day = Number(region.slice(DAY.length));
    this.setState({ day });
    this.activate({ year: this.state.year, month: this.state.month, day }, event.session);
    this.props.onChoose?.({ year: this.state.year, month: this.state.month, day }, event.session);
    return true;
  }
}
