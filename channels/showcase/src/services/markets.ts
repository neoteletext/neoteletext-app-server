// Page 104: markets, simulated (crates/teletext-showcase/src/services/
// markets.rs). Made-up indices and companies move by a bounded random walk
// every few seconds, with arrows in green and red and a mosaic chart of
// the first index. The page says it is demonstration data.

import { page } from "../common.ts";
import { type Application, Attribute, Colour, type Context, type PageEntry, SIXELS_DOWN, Screen, Sixels, mosaicColour } from "../kit.ts";
import { SECOND_MS, hoursMinutesSeconds } from "./calendar.ts";
import { FIRST_ROW, art, band, demonstration, publish } from "./draw.ts";
import { MARKETS, navigation } from "./pages.ts";

/** Prices change every three seconds: often enough to watch move. */
const INTERVAL_MS = 3 * SECOND_MS;
/** Prices are kept in hundredths. */
const HUNDREDTHS = 100;
/** Basis points in a whole: a change is reckoned in hundredths of a per cent. */
const BASIS_POINTS = 10_000;
/** The largest random move in one step, in basis points (0.25%). */
const STEP_BASIS_POINTS = 25;
/** The walk's choices of move: every basis point from down to up, or none. */
export const STEP_CHOICES = STEP_BASIS_POINTS * 2 + 1;
/** The pull back towards the starting price: one basis point a step for each this many away from it, so the walk wanders but returns. */
const REVERSION = 200;
/** A price never falls below half its starting price... */
const FLOOR_DIVISOR = 2;
/** ...nor rises above twice it. */
const CEILING_FACTOR = 2;

/** The indices: name and starting level in hundredths. */
const INDICES: readonly (readonly [string, number])[] = [
  ["MOSAIC 100", 742_150],
  ["SIXEL 50", 318_825],
  ["FASTEXT TECH", 1_204_000],
  ["PARITY ALL", 455_390],
];
/** The companies: name and starting price in hundredths of a pound. */
const SHARES: readonly (readonly [string, number])[] = [
  ["Sixel Systems", 41_250],
  ["Subpage Foods", 8_760],
  ["Parity Power", 23_115],
  ["Header Rail", 5_480],
  ["Hamming Mines", 61_900],
];

/** The chart's width in sixels, one a sample: three minutes of steps. */
const CHART_WIDTH = 60;
/** Its height in sixels: four rows. */
const CHART_HEIGHT = 12;
/** Its first row. */
const CHART_ROW = 11;
/** Its first column, after its colour attribute. */
const CHART_COLUMN = 1;
/** The high and low labels' column, after the chart's 30 cells. */
const CHART_LABEL_COLUMN = 31;

/** The row of the indices' heading. */
const INDICES_ROW = 4;
/** The row of the shares' heading. */
const SHARES_ROW = 16;
/** Where a line's level goes: its colour attribute's column. */
const LEVEL_COLUMN = 14;
/** The level's width, right-aligned. */
const LEVEL_WIDTH = 10;
/**
 * The arrow's mosaic colour attribute's column; Separated Graphics follows
 * it, so arrows on rows one above the other do not touch, and the arrow
 * takes the three cells after that.
 */
const ARROW_COLUMN = 25;
/** The arrow's first cell, after its colour and Separated Graphics. */
const ARROW_CELLS_COLUMN = ARROW_COLUMN + 2;
/** The change's colour attribute's column. */
const CHANGE_COLUMN = 30;
/** The arrows' size in sixels: three cells of one row. */
const ARROW_WIDTH = 6;
const ARROW_HEIGHT = 3;
/** The arrows' sixels: a triangle pointing up or down. */
const UP = ["..##..", ".####.", "######"] as const;
/** The down arrow. */
const DOWN = ["######", ".####.", "..##.."] as const;

/** One price that moves. */
export class Instrument {
  readonly name: string;
  /** Its price or level now, in hundredths. */
  value: number;
  /** Where it started, which its walk returns towards and is bounded by. */
  readonly base: number;
  /** Its value before the last step. */
  previous: number;

  /** An instrument starting at `value`. */
  constructor(name: string, value: number) {
    this.name = name;
    this.value = value;
    this.base = value;
    this.previous = value;
  }

  /** The lowest the walk may go. */
  floor(): number {
    return Math.trunc(this.base / FLOOR_DIVISOR);
  }

  /** The highest. */
  ceiling(): number {
    return this.base * CEILING_FACTOR;
  }

  /**
   * One step of the walk with `random` below {@link STEP_CHOICES}: a move
   * of up to 25 basis points either way, pulled back towards the start,
   * and kept between the floor and the ceiling.
   */
  walk(random: number): void {
    const moved = Math.min(random, STEP_CHOICES - 1) - STEP_BASIS_POINTS;
    const away = this.base === 0 ? 0 : Math.trunc(((this.value - this.base) * BASIS_POINTS) / this.base);
    const basisPoints = moved - Math.trunc(away / REVERSION);
    const change = Math.trunc((this.value * basisPoints) / BASIS_POINTS);
    this.previous = this.value;
    this.value = Math.min(this.ceiling(), Math.max(this.floor(), this.value + change));
  }

  /** Its change since the start, in basis points. */
  change(): number {
    return this.base === 0 ? 0 : Math.trunc(((this.value - this.base) * BASIS_POINTS) / this.base);
  }
}

/** "1234.56". */
function decimal(hundredths: number): string {
  const sign = hundredths < 0 ? "-" : "";
  const magnitude = Math.abs(hundredths);
  return `${sign}${Math.floor(magnitude / HUNDREDTHS)}.${(magnitude % HUNDREDTHS).toString().padStart(2, "0")}`;
}

/** The colour of a change: green up, red down, white unchanged. */
function changeColour(change: number): Colour {
  return change > 0 ? Colour.Green : change < 0 ? Colour.Red : Colour.White;
}

/** One instrument's line: name, level, arrow and change. */
function line(screen: Screen, row: number, instrument: Instrument): void {
  screen.coloured(row, 0, Colour.White, instrument.name);
  const level = decimal(instrument.value);
  screen.coloured(row, LEVEL_COLUMN + Math.max(0, LEVEL_WIDTH - level.length), Colour.Yellow, level);
  // The arrow agrees with the change beside it, since the start, as a
  // markets page's arrows do; the last step alone made a red arrow sit
  // beside a rise.
  const change = instrument.change();
  const colour = changeColour(change);
  const arrow = change > 0 ? UP : change < 0 ? DOWN : null;
  if (arrow !== null) {
    const canvas = new Sixels(ARROW_WIDTH, ARROW_HEIGHT);
    art(canvas, 0, 0, arrow);
    canvas.draw(screen, row, ARROW_CELLS_COLUMN, colour);
    // The canvas puts its colour in the cell before it; the colour goes
    // first, and Separated Graphics after it, which is set-at.
    screen.codes(row, ARROW_COLUMN, [mosaicColour(colour), Attribute.Separated]);
  }
  screen.coloured(row, CHANGE_COLUMN, colour, `${change > 0 ? "+" : ""}${decimal(change)}%`);
}

/** The markets: the indices, the shares and the first index's recent levels. */
export class Markets implements Application {
  readonly indices: Instrument[] = INDICES.map(([name, value]) => new Instrument(name, value));
  readonly shares: Instrument[] = SHARES.map(([name, value]) => new Instrument(name, value));
  readonly #history: number[] = [];

  pages(): readonly PageEntry[] {
    return [{ page: MARKETS, title: "Markets" }];
  }

  /** Moves every price one step. */
  step(context: Context): void {
    for (const instrument of [...this.indices, ...this.shares]) {
      instrument.walk(context.below(STEP_CHOICES));
    }
    if (this.#history.length >= CHART_WIDTH) {
      this.#history.shift();
    }
    const first = this.indices[0];
    if (first !== undefined) {
      this.#history.push(first.value);
    }
  }

  /** The page at the Unix time `epochMs`. */
  screen(epochMs: number): Screen {
    const screen = page("Markets", Colour.Green, navigation(MARKETS));
    screen.coloured(FIRST_ROW, 0, Colour.Cyan, `Prices at ${hoursMinutesSeconds(epochMs)} UTC`);
    band(screen, INDICES_ROW, Colour.Blue, Colour.White, "INDEX            LEVEL       CHANGE");
    this.indices.forEach((instrument, index) => line(screen, INDICES_ROW + 1 + index, instrument));
    const first = this.indices[0];
    if (first !== undefined) {
      screen.coloured(CHART_ROW - 1, 1, Colour.Cyan, `${first.name}, the last three minutes`);
    }
    this.#chart(screen);
    band(screen, SHARES_ROW, Colour.Blue, Colour.White, "SHARE            PRICE       CHANGE");
    this.shares.forEach((instrument, index) => line(screen, SHARES_ROW + 1 + index, instrument));
    demonstration(screen);
    return screen;
  }

  /** The first index's levels as a filled mosaic chart, scaled between its lowest and highest, green when it rose over the chart and red when it fell. */
  #chart(screen: Screen): void {
    if (this.#history.length === 0) {
      return;
    }
    const low = Math.min(...this.#history);
    const high = Math.max(...this.#history);
    const range = Math.max(high - low, 1);
    const canvas = new Sixels(CHART_WIDTH, CHART_HEIGHT);
    const tallest = CHART_HEIGHT - 1;
    this.#history.forEach((level, x) => {
      // At least the bottom sixel, so the lowest level still shows.
      const height = Math.trunc(((level - low) * tallest) / range);
      for (let y = tallest - height; y < CHART_HEIGHT; y += 1) {
        canvas.set(x, y, true);
      }
    });
    const rose = (this.#history[this.#history.length - 1] ?? 0) >= (this.#history[0] ?? 0);
    canvas.draw(screen, CHART_ROW, CHART_COLUMN, rose ? Colour.Green : Colour.Red);
    const bottom = CHART_ROW + Math.ceil(CHART_HEIGHT / SIXELS_DOWN) - 1;
    screen.coloured(CHART_ROW, CHART_LABEL_COLUMN, Colour.White, decimal(high));
    screen.coloured(bottom, CHART_LABEL_COLUMN, Colour.White, decimal(low));
  }

  start(context: Context): void {
    // A chart's worth of history, so the page opens with one.
    for (let step = 0; step < CHART_WIDTH; step += 1) {
      this.step(context);
    }
    this.wake(context);
  }

  wake(context: Context): void {
    this.step(context);
    publish(context, MARKETS, this.screen(context.epochMs()));
    context.wakeIn(INTERVAL_MS);
  }
}
