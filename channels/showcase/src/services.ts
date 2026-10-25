// Live services (pages 101 to 199; crates/teletext-showcase/src/
// services.rs): news, weather, a world clock, markets, the server clock,
// sport, a TV guide, pages about NeoTeletext and teletext, and a long read
// that scrolls. Every page but the long read is the channel's: drawn once
// per change and sent to every viewer (decision 7), each application
// waking only as often as its page changes. The long read is each
// reader's own once they scroll.
//
// Weather, markets, sport and the TV guide are simulated, with made-up
// places, companies, teams and programmes, and say so on the page.

import type { Area, Section } from "./area.ts";
import { page } from "./common.ts";
import { type Application, Colour, type Context, type PageEntry, Sixels } from "./kit.ts";
import { About } from "./services/about.ts";
import { SECOND_MS, two } from "./services/calendar.ts";
import { DIGIT_HEIGHT, digits, publish } from "./services/draw.ts";
import { Guide } from "./services/guide.ts";
import { LongRead } from "./services/long-read.ts";
import { Markets } from "./services/markets.ts";
import { News } from "./services/news.ts";
import { ABOUT, CLOCK, NEWS, WORLD_CLOCK, navigation } from "./services/pages.ts";
import { Sport } from "./services/sport.ts";
import { Weather } from "./services/weather.ts";
import { WorldClock } from "./services/world-clock.ts";

/** The server clock's digits: one font pixel to a sixel. */
const CLOCK_SCALE = 1;
/** The canvas the time is drawn on, in sixels: "HH:MM:SS" is six digits six sixels wide and two colons two wide. */
const CLOCK_CANVAS_WIDTH = 40;
/** The digits' first column: the canvas's 20 cells centred. */
const CLOCK_COLUMN = 10;
/** The digits' first row. */
const CLOCK_ROW = 9;
/** Seconds in a minute, an hour and a day. */
const MINUTE_SECONDS = 60;
const HOUR_SECONDS = 3_600;
const DAY_SECONDS = 86_400;

/**
 * A clock of the time since the program started, in hours, minutes and
 * seconds: the program's clock, which applications are given. The server
 * starts the program when it starts, so the two agree but for a restart.
 */
export class Clock implements Application {
  pages(): readonly PageEntry[] {
    return [{ page: CLOCK, title: "Server clock" }];
  }

  #draw(context: Context): void {
    const elapsed = Math.floor(context.now() / SECOND_MS);
    const days = Math.floor(elapsed / DAY_SECONDS);
    const hours = Math.floor((elapsed % DAY_SECONDS) / HOUR_SECONDS);
    const minutes = Math.floor(elapsed / MINUTE_SECONDS) % MINUTE_SECONDS;
    const seconds = elapsed % MINUTE_SECONDS;
    const screen = page("Server clock", Colour.Magenta, navigation(CLOCK));
    screen.centred(6, Colour.Cyan, "Time since the server started");
    const canvas = new Sixels(CLOCK_CANVAS_WIDTH, DIGIT_HEIGHT);
    digits(canvas, 0, 0, `${two(hours)}:${two(minutes)}:${two(seconds)}`, CLOCK_SCALE);
    canvas.draw(screen, CLOCK_ROW, CLOCK_COLUMN, Colour.Yellow);
    const daysText = days === 0 ? "hours, minutes and seconds" : days === 1 ? "and 1 day" : `and ${days} days`;
    screen.centred(13, Colour.Yellow, daysText);
    screen.centred(17, Colour.White, "One shared page, changed once a");
    screen.centred(18, Colour.White, "second, sent to every viewer");
    publish(context, CLOCK, screen);
    context.wakeAt(context.now() + SECOND_MS - (context.now() % SECOND_MS));
  }

  start(context: Context): void {
    this.#draw(context);
  }

  wake(context: Context): void {
    this.#draw(context);
  }
}

/** The live services. */
export const services: Area = {
  applications: () => [new News(), new Weather(), new WorldClock(), new Markets(), new Clock(), new Sport(), new Guide(), new About(), new LongRead()],
  // Three entries, each naming the pages it leads to, so the index keeps room for the other areas.
  sections: (): readonly Section[] => [
    { page: NEWS, name: "News and live services", about: "weather 102 markets 104 sport 106" },
    { page: WORLD_CLOCK, name: "Clocks and TV guide", about: "world 103, server 105, TV 107" },
    { page: ABOUT, name: "About NeoTeletext", about: "facts 109 and a long read on 110" },
  ],
};
