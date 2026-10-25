// Page 150, behind the scenes: the server's own figures, live, as
// Hackaday's page 150 shows them (docs/applications.md section 7). The
// server sends them every 10 seconds; the page is the channel's, drawn
// again each time and sent to every viewer with it open. The figures are
// counts only: nothing says who a viewer is.

import { page } from "../common.ts";
import {
  type Application,
  Colour,
  type Context,
  type Figures,
  type PageEntry,
  type Screen,
  type ServerStats,
  Sixels,
  figure,
  figuresIn,
  readPage,
} from "../kit.ts";
import { BODY_COLUMN, BODY_WIDTH, DIGIT_HEIGHT, DIGIT_WIDTH, FASTEXT_ROW, FIRST_ROW, band, digits, length, publish, right } from "./draw.ts";
import { BEHIND, navigation } from "./pages.ts";

/** The page's title. */
export const TITLE = "Behind the scenes";
/** The title band's colour. */
const BAND = Colour.Blue;
/** The most viewers the large digits show: four of them fill their canvas. */
const MOST_SHOWN = 9_999;
/** The large digits' canvas: four digits of five pixels and a gap, one sixel a pixel. */
const VIEWERS_WIDTH = 4 * (DIGIT_WIDTH + 1);
/** Their first column, after the label and their colour attribute. */
const VIEWERS_COLUMN = 22;
/** The row of the first figure under the large digits, which take three rows (seven sixels, three to a cell). */
const FIGURES_ROW = FIRST_ROW + 4;
/** The band over the most opened pages. */
const MOST_OPENED_ROW = 13;
/** The most opened pages listed: the rows down to the channels' line. */
export const MOST_OPENED_SHOWN = 6;
/** Where a listed page's title starts, after its number and the title's colour attribute. */
const TITLE_COLUMN = 5;
/** The title's width, short of its count. */
const TITLE_WIDTH = 25;
/** The row naming every channel's viewers. */
const CHANNELS_ROW = MOST_OPENED_ROW + 1 + MOST_OPENED_SHOWN;
/** The row saying what made all this. */
const MADE_ROW = CHANNELS_ROW + 1;
/** The row a page waiting for its first figures says so on. */
const WAITING_ROW = 10;
/** Seconds in an hour and a minute. */
const HOUR_SECONDS = 3_600;
const MINUTE_SECONDS = 60;
/** Bytes in a kilobyte, the server's memory being given in them. */
const KILOBYTE = 1_024;
/** Byte counts as a reader says them, the largest unit first. */
const UNITS: readonly (readonly [unit: string, size: number])[] = [
  ["GB", KILOBYTE ** 3],
  ["MB", KILOBYTE ** 2],
  ["KB", KILOBYTE],
];
/** The mark of text cut to fit. */
const CUT = "..";

if (MADE_ROW >= FASTEXT_ROW) {
  throw new Error("the page's last line is above the Fastext row");
}

/** `text` cut to `width` characters, with a mark when cut. */
export function fit(text: string, width: number): string {
  return length(text) <= width ? text : [...text].slice(0, width - CUT.length).join("") + CUT;
}

/** A count of bytes as a reader says it: "1.5 MB". */
export function amount(bytes: number): string {
  const unit = UNITS.find(([, size]) => bytes >= size);
  return unit === undefined ? `${bytes} B` : `${(bytes / unit[1]).toFixed(1)} ${unit[0]}`;
}

/** The figures' lines under the large digits. */
function figureLines(server: Figures, channel: Figures): readonly (readonly [Colour, string])[] {
  const uptime = figure(server, "uptime_s");
  const memoryKb = figure(server, "memory_kb");
  // The server says -1 where the system does not tell it (section 7).
  const memory = memoryKb > 0 ? amount(memoryKb * KILOBYTE) : "n/a";
  const minutes = Math.floor((uptime % HOUR_SECONDS) / MINUTE_SECONDS);
  return [
    [Colour.Cyan, `Peak ${figure(channel, "viewers_peak")}, ${figure(channel, "viewers_total")} visits since start`],
    [Colour.White, `Pages opened ${figure(channel, "opens")}, keys ${figure(channel, "keys")}`],
    [Colour.White, `Pages drawn ${figure(channel, "shared_published")} shared, ${figure(channel, "own_published")} own`],
    [Colour.White, `Sent ${amount(figure(server, "bytes_sent"))} in ${figure(server, "messages_sent")} messages`],
    [Colour.White, `Server up ${Math.floor(uptime / HOUR_SECONDS)}h ${minutes.toString().padStart(2, "0")}m, memory ${memory}`],
  ];
}

/** The page: the figures if they have come, or that they are awaited. `titleOf` names the channel's pages. */
export function draw(stats: ServerStats | null, titleOf: (page: number) => string | undefined): Screen {
  const screen = page(TITLE, BAND, navigation(BEHIND));
  if (stats === null) {
    screen.centred(WAITING_ROW, Colour.White, "Waiting for the server's figures");
    screen.centred(WAITING_ROW + 1, Colour.Cyan, "They come every 10 seconds");
    return screen;
  }
  const { server, channel } = stats;
  screen.coloured(FIRST_ROW, BODY_COLUMN, Colour.Yellow, "WATCHING");
  screen.coloured(FIRST_ROW + 1, BODY_COLUMN, Colour.Yellow, "THE SHOWCASE NOW");
  const viewers = new Sixels(VIEWERS_WIDTH, DIGIT_HEIGHT);
  digits(viewers, 0, 0, Math.min(MOST_SHOWN, Math.max(0, figure(channel, "viewers"))).toString(), 1);
  viewers.draw(screen, FIRST_ROW, VIEWERS_COLUMN, Colour.Green);
  figureLines(server, channel).forEach(([colour, text], offset) => {
    screen.coloured(FIGURES_ROW + offset, BODY_COLUMN, colour, fit(text, BODY_WIDTH));
  });
  band(screen, MOST_OPENED_ROW, Colour.Blue, Colour.Yellow, "Most opened pages");
  figuresIn(channel, "most_opened")
    .flatMap(([number, opens]) => {
      const opened = readPage(number);
      return opened === undefined ? [] : [{ number, opened, opens }];
    })
    .slice(0, MOST_OPENED_SHOWN)
    .forEach(({ number, opened, opens }, offset) => {
      const row = MOST_OPENED_ROW + 1 + offset;
      screen.coloured(row, 0, Colour.Yellow, number);
      screen.coloured(row, TITLE_COLUMN - 1, Colour.White, fit(titleOf(opened) ?? "", TITLE_WIDTH));
      right(screen, row, Colour.Cyan, opens.toString());
    });
  const watching = figuresIn(server, "viewers").map(([folder, count]) => `${folder} ${count}`);
  if (watching.length > 0) {
    screen.coloured(CHANNELS_ROW, BODY_COLUMN, Colour.Magenta, fit(`Watching: ${watching.join(", ")}`, BODY_WIDTH));
  }
  screen.coloured(MADE_ROW, BODY_COLUMN, Colour.Green, "A Rust server, a TypeScript channel");
  return screen;
}

/** The page 150 application. */
export class Behind implements Application {
  pages(): readonly PageEntry[] {
    return [{ page: BEHIND, title: TITLE }];
  }

  start(context: Context): void {
    publish(context, BEHIND, draw(null, (page) => context.titleOf(page)));
  }

  stats(stats: ServerStats, context: Context): void {
    publish(context, BEHIND, draw(stats, (page) => context.titleOf(page)));
  }
}
