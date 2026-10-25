// Adventure games (pages 400 to 499): the menu (400), Station Zero, a
// typed text adventure that is each viewer's own (401), and The Lost
// Crown, a story told on the coloured keys, each viewer's place their own
// (402) (crates/teletext-showcase/src/adventure.rs).

import type { Area, Section } from "./area.ts";
import { CASTLE } from "./adventure/crown/pictures.ts";
import { Crown } from "./adventure/crown.ts";
import { CROWN, MENU, STATION } from "./adventure/pages.ts";
import { Station } from "./adventure/station.ts";
import { INDEX, page } from "./common.ts";
import { type Application, Colour, type Context, type PageEntry, type Screen, pageText } from "./kit.ts";

/** The menu's title band colour. */
const BAND = Colour.Magenta;
/** The first row of the menu's picture. */
const PICTURE_ROW = 3;
/** The menu's first entry's row. */
const FIRST_ENTRY_ROW = 10;
/** Rows between entries. */
const ENTRY_STEP = 5;
/** The column of an entry's name, after its page number. */
const NAME_COLUMN = 5;
/** The picture's first column: centred. */
const PICTURE_COLUMN = 10;
/** The column of the lines about an entry: their colour under the name's colour, so the text lines up with the name. */
const ABOUT_COLUMN = NAME_COLUMN + 1;
/** Rows a double-height line covers: its own and the next (EN 300 706 clause 12.3). */
const DOUBLE_HEIGHT_ROWS = 2;
/** The row of the note at the foot. */
const NOTE_ROW = 21;

/** An entry of the menu: its page, its name and two lines about it. */
interface Entry {
  readonly page: number;
  readonly name: string;
  readonly about: readonly [string, string];
}

/** The menu's entries. */
const ENTRIES: readonly Entry[] = [
  { page: STATION, name: "Station Zero", about: ["A typed adventure in space:", "fourteen rooms, one way home"] },
  { page: CROWN, name: "The Lost Crown", about: ["A Fastext quest for a crown:", "eight endings, coloured keys"] },
];

/** The menu's page. */
export function menu(): Screen {
  const screen = page("Adventures", BAND, [
    ["Station", STATION],
    ["Crown", CROWN],
    ["", null],
    ["Index", INDEX],
  ]);
  CASTLE.draw(screen, PICTURE_ROW, PICTURE_COLUMN);
  let row = FIRST_ENTRY_ROW;
  for (const entry of ENTRIES) {
    screen.coloured(row, 0, Colour.Yellow, pageText(entry.page));
    screen.doubleHeight(row, NAME_COLUMN, Colour.White, entry.name);
    entry.about.forEach((line, offset) => {
      screen.coloured(row + DOUBLE_HEIGHT_ROWS + offset, ABOUT_COLUMN, Colour.Cyan, line);
    });
    row += ENTRY_STEP;
  }
  screen.centred(NOTE_ROW, Colour.Green, "Station Zero takes the keyboard");
  return screen;
}

/** Page 400: the adventures, published once. */
export class Menu implements Application {
  pages(): readonly PageEntry[] {
    return [{ page: MENU, title: "Adventures" }];
  }

  start(context: Context): void {
    context.publish("channel", MENU, menu());
  }
}

/** Adventure games. */
export const adventure: Area = {
  applications: () => [new Menu(), new Station(), new Crown()],
  sections: (): readonly Section[] => [{ page: MENU, name: "Adventures", about: "a typed adventure and a Fastext quest" }],
};
