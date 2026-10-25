// Page 103: the world clock, the real time from the server's clock
// (crates/teletext-showcase/src/services/world_clock.rs): UTC in large
// mosaic digits, the date, and twelve cities at their standard time.
// Daylight saving is not applied, and the page says so: its rules change
// by law, and a fixed table would go wrong without notice.

import { page } from "../common.ts";
import { type Application, COLUMNS, Colour, type Context, type PageEntry, Screen, Sixels } from "../kit.ts";
import { CivilDate, SECOND_MS, hoursMinutes, local, timeOfDay, two } from "./calendar.ts";
import { DIGIT_HEIGHT, digits, publish } from "./draw.ts";
import { WORLD_CLOCK, navigation } from "./pages.ts";

/** The cities and their standard time's offset from UTC, in minutes. */
const CITIES: readonly (readonly [city: string, offsetMinutes: number])[] = [
  ["Honolulu", -600],
  ["Vancouver", -480],
  ["New York", -300],
  ["Sao Paulo", -180],
  ["London", 0],
  ["Lagos", 60],
  ["Moscow", 180],
  ["Dubai", 240],
  ["Mumbai", 330],
  ["Singapore", 480],
  ["Tokyo", 540],
  ["Sydney", 600],
];

/** The hours and minutes' font pixels, in sixels each way. */
const LARGE = 2;
/** The seconds' font pixels. */
const SMALL = 1;
/** The large digits' canvas: "HH:MM" is four digits of six pixels and a colon of two, at two sixels a pixel; the digits' height likewise. */
const LARGE_WIDTH = 52;
const LARGE_HEIGHT = DIGIT_HEIGHT * LARGE;
/** The seconds' canvas: two digits of six pixels, as tall as the large digits so its rows line up with theirs. */
const SMALL_WIDTH = 12;
const SMALL_HEIGHT = DIGIT_HEIGHT * LARGE;
/** The first column of the large digits, which centres the two canvases and the attribute between them (26 + 1 + 6 cells). */
const LARGE_COLUMN = 4;
/** The first column of the seconds: after the large digits' 26 cells and the seconds' colour attribute. */
const SMALL_COLUMN = 31;
/** The digits' first row. */
const DIGITS_ROW = 5;
/** The date's row, double height. */
const DATE_ROW = 11;
/** The cities' first row. */
const CITIES_ROW = 14;
/** The cities in each of the two columns. */
const CITIES_DOWN = 6;
/** A city column's width: the name's colour and nine characters, the time's colour and five, the day's colour and three. */
const CITY_COLUMN_WIDTH = 20;
/** Within a city column: where the time's colour attribute goes. */
const TIME_OFFSET = 10;
/** Within a city column: where the day's colour attribute goes. */
const DAY_OFFSET = 16;
/** The note's first row. */
const NOTE_ROW = 21;
/** The double-height and colour attributes before the date. */
const DATE_ATTRIBUTES = 2;

/** The world clock's page at the Unix time `epochMs`. */
export function screen(epochMs: number): Screen {
  const drawn = page("World clock", Colour.Blue, navigation(WORLD_CLOCK));
  drawn.centred(3, Colour.Cyan, "Coordinated Universal Time");
  const [, , seconds] = timeOfDay(epochMs);
  const large = new Sixels(LARGE_WIDTH, LARGE_HEIGHT);
  digits(large, 0, 0, hoursMinutes(epochMs), LARGE);
  large.draw(drawn, DIGITS_ROW, LARGE_COLUMN, Colour.Yellow);
  const small = new Sixels(SMALL_WIDTH, SMALL_HEIGHT);
  // The seconds sit on the large digits' baseline.
  digits(small, 0, LARGE_HEIGHT - DIGIT_HEIGHT, two(seconds), SMALL);
  small.draw(drawn, DIGITS_ROW, SMALL_COLUMN, Colour.White);
  const date = CivilDate.fromEpochMs(epochMs).long();
  const width = date.length + DATE_ATTRIBUTES;
  drawn.doubleHeight(DATE_ROW, Math.floor(Math.max(0, COLUMNS - width) / 2), Colour.White, date);
  CITIES.forEach(([city, offset], index) => {
    const row = CITIES_ROW + (index % CITIES_DOWN);
    const column = CITY_COLUMN_WIDTH * Math.floor(index / CITIES_DOWN);
    const there = local(epochMs, offset);
    drawn.coloured(row, column, Colour.White, city);
    drawn.coloured(row, column + TIME_OFFSET, Colour.Yellow, hoursMinutes(there));
    drawn.coloured(row, column + DAY_OFFSET, Colour.Cyan, CivilDate.fromEpochMs(there).weekdayShort());
  });
  drawn.centred(NOTE_ROW, Colour.Green, "Standard times: daylight saving");
  drawn.centred(NOTE_ROW + 1, Colour.Green, "is not applied");
  return drawn;
}

/** The world clock: it has no state, the time is the server's. */
export class WorldClock implements Application {
  pages(): readonly PageEntry[] {
    return [{ page: WORLD_CLOCK, title: "World clock" }];
  }

  start(context: Context): void {
    this.wake(context);
  }

  wake(context: Context): void {
    const epochMs = context.epochMs();
    publish(context, WORLD_CLOCK, screen(epochMs));
    // At the next whole second of the real time.
    context.wakeIn(SECOND_MS - (epochMs % SECOND_MS));
  }
}
