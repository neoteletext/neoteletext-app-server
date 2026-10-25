// The live services' pages and their Fastext (crates/teletext-showcase/
// src/services.rs): red the service before, green the one after, yellow
// the news, and cyan the index.

import { INDEX, type Prompt, type Prompts } from "../common.ts";

/** The news page. */
export const NEWS = 0x101;
/** The weather page. */
export const WEATHER = 0x102;
/** The world clock's page. */
export const WORLD_CLOCK = 0x103;
/** The markets page. */
export const MARKETS = 0x104;
/** The server clock's page. */
export const CLOCK = 0x105;
/** The sport page. */
export const SPORT = 0x106;
/** The TV guide's page. */
export const GUIDE = 0x107;
/** The page about NeoTeletext. */
export const ABOUT = 0x108;
/** The teletext facts page. */
export const FACTS = 0x109;
/** The long read, scrolled within the page. */
export const LONG_READ = 0x110;

/** A service: its page and its Fastext label (at most nine characters, a quarter of row 24 after its colour). */
type Service = readonly [page: number, label: string];

/** The services in page order. */
export const SERVICES: readonly Service[] = [
  [NEWS, "News"],
  [WEATHER, "Weather"],
  [WORLD_CLOCK, "World"],
  [MARKETS, "Markets"],
  [CLOCK, "Clock"],
  [SPORT, "Sport"],
  [GUIDE, "TV guide"],
  [ABOUT, "About"],
  [FACTS, "Facts"],
  [LONG_READ, "Long read"],
];

/** The yellow link's choices, in order: the first that is not the page itself nor already red or green. */
const YELLOW_CHOICES: readonly Service[] = [
  [NEWS, "News"],
  [ABOUT, "About"],
  [WEATHER, "Weather"],
];

/**
 * A service page's Fastext: red the service before, green the one after
 * (wrapping round), yellow the news (or the next choice where the news is
 * already linked), and cyan the index.
 */
export function navigation(page: number): Prompts {
  const found = SERVICES.findIndex(([number]) => number === page);
  const position = found === -1 ? 0 : found;
  const count = SERVICES.length;
  const before = SERVICES[(position + count - 1) % count];
  const after = SERVICES[(position + 1) % count];
  const taken = (number: number): boolean => number === page || before?.[0] === number || after?.[0] === number;
  const third = YELLOW_CHOICES.find(([number]) => !taken(number));
  const link = (service: Service | undefined): Prompt => (service === undefined ? ["", null] : [service[1], service[0]]);
  return [link(before), link(after), link(third), ["Index", INDEX]];
}
