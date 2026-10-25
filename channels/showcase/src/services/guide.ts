// Page 107: a TV guide for a made-up channel, Mosaic One, whose day's
// schedule repeats every day (crates/teletext-showcase/src/services/
// guide.rs): the programme on now, by the real time, with a bar of how far
// it has run, and what follows. The page says it is demonstration data.

import { page } from "../common.ts";
import { type Application, Colour, type Context, type PageEntry, Screen, Sixels } from "../kit.ts";
import { CivilDate, DAY_MINUTES, MINUTE_MS, minuteOfDay, two } from "./calendar.ts";
import { FIRST_ROW, demonstration, publish, right } from "./draw.ts";
import { GUIDE, navigation } from "./pages.ts";

/** The page changes once a minute, with the bar. */
const INTERVAL_MS = MINUTE_MS;
/** Minutes in an hour, for the times shown. */
const HOUR_MINUTES = 60;

/** The day's schedule: each programme's start, in minutes from midnight UTC, and its title. Each runs until the next begins. */
const SCHEDULE: readonly (readonly [start: number, title: string])[] = [
  [0, "Night Sixels"],
  [360, "Breakfast on Mosaic One"],
  [540, "Carousel Kitchen"],
  [570, "The Magazine Hour"],
  [630, "The Fastext Files"],
  [690, "Parity Gardens"],
  [720, "Lunchtime News"],
  [750, "Pixel Puzzlers"],
  [810, "Film: The Lost Sub-page"],
  [930, "Header Hill Vets"],
  [990, "Raster Club"],
  [1_050, "Quiz: Reveal the Answer"],
  [1_080, "Evening News"],
  [1_110, "Mosaic Isles Tonight"],
  [1_140, "Sixel Street"],
  [1_170, "Hold Mosaics"],
  [1_200, "Contiguous"],
  [1_260, "Late Film: Double Height"],
  [1_380, "Late News"],
  [1_395, "The Separated Sessions"],
];

/** The channel's name. */
const CHANNEL = "Mosaic One";
/** The row of the programme on now. */
const NOW_ROW = 5;
/** The row of its bar. */
const BAR_ROW = 6;
/** The bar's first column, after its colour attribute. */
const BAR_COLUMN = 3;
/** The bar's width in sixels: 34 cells. */
const BAR_WIDTH = 68;
/** The bar's height in sixels: one row of cells. */
const BAR_HEIGHT = 3;
/** The track's sixel row, in the middle of the bar. */
const TRACK = 1;
/** The heading of what follows. */
const LATER_ROW = 9;
/** How many programmes follow. */
const LATER_COUNT = 12;
/** A time's colour attribute's column, leaving two cells for a background. */
const TIME_COLUMN = 2;
/** A title's colour attribute's column. */
const TITLE_COLUMN = 8;

/** "14:30" for a minute of the day. */
function clock(minute: number): string {
  const inDay = minute % DAY_MINUTES;
  return `${two(Math.floor(inDay / HOUR_MINUTES))}:${two(inDay % HOUR_MINUTES)}`;
}

/** The index of the programme on at `minute` of the day. */
export function onNow(minute: number): number {
  const found = SCHEDULE.findLastIndex(([start]) => start <= minute);
  return found === -1 ? 0 : found;
}

/** The programme `index` places after the first of the day, wrapping into the next day: its start and title. */
function programme(index: number): readonly [number, string] {
  return SCHEDULE[index % SCHEDULE.length] ?? [0, ""];
}

/** What is left of a programme, in words. */
function leftOf(minutes: number): string {
  const hours = Math.floor(minutes / HOUR_MINUTES);
  const rest = minutes % HOUR_MINUTES;
  if (hours === 0) {
    return rest === 1 ? "1 minute to go" : `${rest} minutes to go`;
  }
  return `${hours} h ${two(rest)} min to go`;
}

/** The guide's page at the Unix time `epochMs`. */
export function screen(epochMs: number): Screen {
  const drawn = page("TV guide", Colour.Blue, navigation(GUIDE));
  const date = CivilDate.fromEpochMs(epochMs);
  drawn.coloured(FIRST_ROW, 0, Colour.Cyan, CHANNEL);
  drawn.coloured(FIRST_ROW, CHANNEL.length + 1, Colour.White, `${date.weekdayName()} ${date.day} ${date.monthName()}`);
  right(drawn, FIRST_ROW, Colour.Green, "Times UTC");
  const minute = minuteOfDay(epochMs);
  const now = onNow(minute);
  const [start, title] = programme(now);
  const [following] = programme(now + 1);
  // The last programme runs to midnight, the next day's first start.
  const end = following <= start ? DAY_MINUTES : following;
  drawn.background(NOW_ROW, 0, Colour.Blue);
  drawn.coloured(NOW_ROW, TIME_COLUMN, Colour.Yellow, clock(start));
  drawn.coloured(NOW_ROW, TITLE_COLUMN, Colour.White, title);
  right(drawn, NOW_ROW, Colour.Yellow, "NOW ");
  const length = Math.max(end - start, 1);
  const filled = Math.floor((Math.max(0, minute - start) * BAR_WIDTH) / length);
  const bar = new Sixels(BAR_WIDTH, BAR_HEIGHT);
  for (let x = 0; x < BAR_WIDTH; x += 1) {
    for (let y = 0; y < BAR_HEIGHT; y += 1) {
      bar.set(x, y, x < filled || y === TRACK);
    }
  }
  bar.draw(drawn, BAR_ROW, BAR_COLUMN, Colour.Green);
  drawn.coloured(BAR_ROW + 1, TIME_COLUMN, Colour.White, `Ends ${clock(end)}, ${leftOf(Math.max(0, end - minute))}`);
  drawn.coloured(LATER_ROW, TIME_COLUMN, Colour.Yellow, "Later");
  for (let offset = 1; offset <= LATER_COUNT; offset += 1) {
    const [laterStart, laterTitle] = programme(now + offset);
    drawn.coloured(LATER_ROW + offset, TIME_COLUMN, Colour.Cyan, clock(laterStart));
    drawn.coloured(LATER_ROW + offset, TITLE_COLUMN, Colour.White, laterTitle);
  }
  demonstration(drawn);
  return drawn;
}

/** The guide; it has no state, the schedule is fixed and the time the server's. */
export class Guide implements Application {
  pages(): readonly PageEntry[] {
    return [{ page: GUIDE, title: "TV guide" }];
  }

  start(context: Context): void {
    this.wake(context);
  }

  wake(context: Context): void {
    const epochMs = context.epochMs();
    publish(context, GUIDE, screen(epochMs));
    // At the next whole minute of the real time.
    context.wakeIn(INTERVAL_MS - (epochMs % INTERVAL_MS));
  }
}
