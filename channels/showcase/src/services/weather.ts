// Page 102: weather, simulated, for a made-up region, the Mosaic Isles
// (crates/teletext-showcase/src/services/weather.rs): a mosaic map with
// each town's temperature, a list of the towns, and a five-day outlook with
// mosaic symbols. Temperatures follow the time of day and drift a little
// each minute; each day's weather comes from the date, so every viewer and
// every restart sees the same. The page says it is demonstration data.

import { page } from "../common.ts";
import { type Application, Colour, type Context, type PageEntry, Screen, Sixels, mosaicColour } from "../kit.ts";
import { CivilDate, DAY_MINUTES, DAY_MS, MINUTE_MS, hoursMinutes, minuteOfDay } from "./calendar.ts";
import { FIRST_ROW, art, demonstration, publish, right } from "./draw.ts";
import { WEATHER, navigation } from "./pages.ts";

/** The page changes once a minute: weather moves slowly. */
const INTERVAL_MS = MINUTE_MS;

/** The map's first row. */
const MAP_ROW = 4;
/** The map's first column, after the sea's background (two cells) and the land's mosaic colour. */
const MAP_COLUMN = 3;
/** The map in sixels, 22 cells by 10 rows: `#` is land. */
const MAP = [
  "............................................",
  "..........................######............",
  "........................##########..........",
  "..............######...############.........",
  "...........###########...##########.........",
  ".........###############..########..........",
  "........##################..####............",
  ".......####################.................",
  "......######################................",
  "......#######################...............",
  ".....#########################..............",
  ".....##########################.............",
  "....###########################.............",
  "....############################............",
  ".....###########...##############...........",
  ".....#########.......############...........",
  "......#######.........###########...........",
  ".......#####..........###########...........",
  "........###..........############...........",
  ".....................###########............",
  "....................###########.............",
  "...###.............###########..............",
  "..#####...........###########...............",
  "..######.........###########................",
  "...####.........##########..................",
  "....##..........#########...................",
  "................#######.....................",
  ".................####.......................",
  "............................................",
  "............................................",
] as const;
/** The map's size in sixels. */
const MAP_WIDTH = 44;
const MAP_HEIGHT = 30;
/** The map's rows of cells. */
const MAP_ROWS = 10;
/** The town list's colour attribute's column, after the map's 22 cells. */
const LIST_COLUMN = 25;
/** The column of a town's weather, after its temperature. */
const LIST_WEATHER_COLUMN = 29;
/** A town takes two rows of the list: its name and its weather. */
const LIST_ROWS_EACH = 2;
/** The width of a temperature, right-aligned. */
const DEGREES_WIDTH = 2;

/** A town: its name, where its temperature goes on the map (the label's first cell), and how much warmer than the region it runs. */
interface Town {
  readonly name: string;
  readonly row: number;
  readonly column: number;
  readonly warmth: number;
}

/** The towns of the Mosaic Isles. */
const TOWNS: readonly Town[] = [
  { name: "Header Hill", row: 5, column: 17, warmth: -2 },
  { name: "Mosaic Bay", row: 7, column: 6, warmth: 0 },
  { name: "Hamming Moor", row: 8, column: 14, warmth: -1 },
  { name: "Port Sixel", row: 10, column: 4, warmth: 1 },
  { name: "Parity Point", row: 12, column: 12, warmth: 2 },
];

/** The kinds of weather. */
export const Kind = { Sunny: "Sunny", Bright: "Bright", Cloudy: "Cloudy", Rain: "Rain", Fog: "Fog" } as const;
/** A kind of weather; its value is its word, at most six characters. */
export type Kind = (typeof Kind)[keyof typeof Kind];

/** The kinds to choose from, each as often as it is listed: rain and fog less often than the rest. */
const KINDS: readonly Kind[] = [Kind.Sunny, Kind.Sunny, Kind.Bright, Kind.Bright, Kind.Cloudy, Kind.Cloudy, Kind.Rain, Kind.Fog];

/** A kind's symbol, two rows of cells, each five cells by one: the sixels and the colour of each half. */
interface WeatherSymbol {
  readonly top: readonly string[];
  readonly topColour: Colour;
  readonly bottom: readonly string[];
  readonly bottomColour: Colour;
}

const SYMBOLS: Readonly<Record<Kind, WeatherSymbol>> = {
  Sunny: { top: ["..#.##.#..", "...####...", ".########."], topColour: Colour.Yellow, bottom: [".########.", "...####...", "..#.##.#.."], bottomColour: Colour.Yellow },
  Bright: { top: [".#.#......", "..###.....", ".#####...."], topColour: Colour.Yellow, bottom: ["...######.", ".#########", "##########"], bottomColour: Colour.White },
  Cloudy: { top: ["...###....", ".######...", "##########"], topColour: Colour.White, bottom: ["##########", ".########.", ".........."], bottomColour: Colour.White },
  Rain: { top: ["...####...", ".########.", "##########"], topColour: Colour.White, bottom: [".#..#..#..", "#..#..#...", ".........."], bottomColour: Colour.Cyan },
  Fog: { top: ["##########", "..........", "..########"], topColour: Colour.White, bottom: ["..........", "########..", ".........."], bottomColour: Colour.White },
};

/** A symbol's half in sixels. */
const SYMBOL_WIDTH = 10;
const SYMBOL_HEIGHT = 3;

/** The region's coolest afternoon high, in degrees Celsius. */
const COOLEST_HIGH = 8;
/** The spread of highs from day to day. */
const HIGH_SPREAD = 10n;
/** How much cooler the night is than the afternoon, at least... */
const NIGHT_DROP = 4;
/** ...plus up to this much more. */
const NIGHT_SPREAD = 5n;
/** The coolest minute of the day, 05:00. */
const COOLEST_MINUTE = 300;
/** The warmest, 15:00. */
const WARMEST_MINUTE = 900;
/** The furthest a town's drift goes from its expected temperature. */
const DRIFT_LIMIT = 2;
/** A drift's step is -1, 0 or 1: three choices. */
const DRIFT_CHOICES = 3;
/** Days in the outlook. */
const OUTLOOK_DAYS = 5;
/** The outlook's heading row. */
const OUTLOOK_ROW = 15;
/** The width of each day's column. */
const OUTLOOK_WIDTH = 8;
/** A symbol's column within a day's, after the day's colour and a gap. */
const SYMBOL_OFFSET = 2;

/** The 64 bits of `mix`. */
const BITS = 64;
/** SplitMix64's increment, the golden ratio's 64 bits, and its two multipliers. */
const GOLDEN = 0x9e37_79b9_7f4a_7c15n;
const MULTIPLIER_A = 0xbf58_476d_1ce4_e5b9n;
const MULTIPLIER_B = 0x94d0_49bb_1331_11ebn;

/** A number from a day and a purpose, the same for everyone who asks (SplitMix64's finaliser, Steele, Lea and Flood 2014). */
function mix(day: number, purpose: number): bigint {
  let value = BigInt.asUintN(BITS, BigInt(day) * GOLDEN + BigInt(purpose));
  value = BigInt.asUintN(BITS, (value ^ (value >> 30n)) * MULTIPLIER_A);
  value = BigInt.asUintN(BITS, (value ^ (value >> 27n)) * MULTIPLIER_B);
  return value ^ (value >> 31n);
}

/** What `mix` is asked for: the high, the night's drop, and the day's weather (a town's is this plus its index). */
const PURPOSE_HIGH = 1;
const PURPOSE_LOW = 2;
const PURPOSE_KIND = 3;

/** A day's forecast: the weather, the afternoon's high and the night's low, in degrees Celsius. */
export interface Forecast {
  readonly kind: Kind;
  readonly high: number;
  readonly low: number;
}

/** Day `day`'s forecast for the region (days since 1970), or a town's with `place` its index plus one. */
export function forecast(day: number, place: number): Forecast {
  const high = COOLEST_HIGH + Number(mix(day, PURPOSE_HIGH) % HIGH_SPREAD);
  const drop = NIGHT_DROP + Number(mix(day, PURPOSE_LOW) % NIGHT_SPREAD);
  const kind = KINDS[Number(mix(day, PURPOSE_KIND + place) % BigInt(KINDS.length))] ?? Kind.Cloudy;
  return { kind, high, low: high - drop };
}

/**
 * The temperature between the night's low at 05:00 and the afternoon's
 * high at 15:00 at `minute` of the day: rising for ten hours, falling for
 * fourteen.
 */
export function temperature(day: Forecast, minute: number): number {
  const rising = WARMEST_MINUTE - COOLEST_MINUTE;
  const falling = DAY_MINUTES - rising;
  const sinceCoolest = (minute + DAY_MINUTES - COOLEST_MINUTE) % DAY_MINUTES;
  const range = day.high - day.low;
  const [numerator, denominator] = sinceCoolest <= rising ? [sinceCoolest, rising] : [DAY_MINUTES - sinceCoolest, falling];
  return day.low + Math.trunc((range * numerator) / denominator);
}

/** The weather: each town's drift, a small random walk on top of the expected temperature. */
export class Weather implements Application {
  /** Each town's drift in degrees, within the limit. */
  readonly drift: number[] = TOWNS.map(() => 0);

  pages(): readonly PageEntry[] {
    return [{ page: WEATHER, title: "Weather" }];
  }

  /** Each town's drift moves by a degree or stays. */
  step(context: Context): void {
    this.drift.forEach((drift, index) => {
      const change = context.below(DRIFT_CHOICES) - 1;
      this.drift[index] = Math.min(DRIFT_LIMIT, Math.max(-DRIFT_LIMIT, drift + change));
    });
  }

  /** The page at the Unix time `epochMs`. */
  screen(epochMs: number): Screen {
    const screen = page("Weather", Colour.Blue, navigation(WEATHER));
    const today = Math.floor(epochMs / DAY_MS);
    const minute = minuteOfDay(epochMs);
    screen.coloured(FIRST_ROW, 0, Colour.Cyan, `Mosaic Isles, ${hoursMinutes(epochMs)} UTC`);
    right(screen, FIRST_ROW, Colour.White, "Celsius ");
    const map = new Sixels(MAP_WIDTH, MAP_HEIGHT);
    art(map, 0, 0, MAP);
    for (let row = MAP_ROW; row < MAP_ROW + MAP_ROWS; row += 1) {
      screen.background(row, 0, Colour.Blue);
    }
    map.draw(screen, MAP_ROW, MAP_COLUMN, Colour.Green);
    TOWNS.forEach((town, index) => {
      const day = forecast(today, index + 1);
      const degrees = temperature(day, minute) + town.warmth + (this.drift[index] ?? 0);
      const label = degrees.toString().padStart(DEGREES_WIDTH);
      // The label in yellow on the sea's blue, then the land again.
      screen.coloured(town.row, town.column, Colour.Yellow, label);
      screen.code(town.row, town.column + label.length + 1, mosaicColour(Colour.Green));
      const listRow = MAP_ROW + index * LIST_ROWS_EACH;
      screen.coloured(listRow, LIST_COLUMN, Colour.White, town.name);
      screen.coloured(listRow + 1, LIST_COLUMN, Colour.Yellow, label);
      screen.coloured(listRow + 1, LIST_WEATHER_COLUMN, Colour.Cyan, day.kind);
    });
    screen.coloured(OUTLOOK_ROW, 0, Colour.Yellow, "Five-day outlook for the region");
    for (let index = 0; index < OUTLOOK_DAYS; index += 1) {
      const day = today + index + 1;
      const outlook = forecast(day, 0);
      const column = index * OUTLOOK_WIDTH;
      const row = OUTLOOK_ROW + 1;
      screen.coloured(row, column, Colour.White, CivilDate.fromDays(day).weekdayShort());
      const symbol = SYMBOLS[outlook.kind];
      const halves = [
        [symbol.top, symbol.topColour],
        [symbol.bottom, symbol.bottomColour],
      ] as const;
      halves.forEach(([lines, colour], half) => {
        const canvas = new Sixels(SYMBOL_WIDTH, SYMBOL_HEIGHT);
        art(canvas, 0, 0, lines);
        canvas.draw(screen, row + 1 + half, column + SYMBOL_OFFSET, colour);
      });
      screen.coloured(row + 3, column, Colour.Green, outlook.kind);
      screen.coloured(row + 4, column, Colour.Yellow, `Hi${outlook.high.toString().padStart(3)}`);
      screen.coloured(row + 5, column, Colour.Cyan, `Lo${outlook.low.toString().padStart(3)}`);
    }
    demonstration(screen);
    return screen;
  }

  start(context: Context): void {
    publish(context, WEATHER, this.screen(context.epochMs()));
    context.wakeIn(INTERVAL_MS - (context.epochMs() % INTERVAL_MS));
  }

  wake(context: Context): void {
    this.step(context);
    publish(context, WEATHER, this.screen(context.epochMs()));
    // At the next whole minute of the real time.
    context.wakeIn(INTERVAL_MS - (context.epochMs() % INTERVAL_MS));
  }
}
