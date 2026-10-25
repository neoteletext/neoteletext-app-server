// Page 200: the input area's menu, one shared page
// (crates/teletext-showcase/src/interactive/menu.rs).

import { INDEX, page } from "../common.ts";
import { Colour, type Screen, pageText } from "../kit.ts";
import { CALCULATOR, FORM, GUESTBOOK, PAINT, POLL, QUIZ } from "./pages.ts";

/** The menu's lines: a page, its name and what it shows. */
const ENTRIES: readonly (readonly [page: number, name: string, about: string])[] = [
  [FORM, "A form", "fields that echo, one masked"],
  [GUESTBOOK, "Guestbook", "post a line every viewer sees"],
  [POLL, "Live poll", "vote with 1 to 4, results live"],
  [QUIZ, "Teletext quiz", "ten questions of TV history"],
  [CALCULATOR, "Calculator", "type a sum, Enter works it out"],
  [PAINT, "Mouse paint", "click the cells to draw"],
];
/** The first entry's row. */
const FIRST_ROW = 4;
/** Rows between entries: the name, then what it does, then a gap. */
const ROW_STEP = 3;
/** The column of each entry's page number, and of its name and what it does. */
const NUMBER_COLUMN = 1;
const NAME_COLUMN = 5;
/** The row naming what the pages take. */
const NOTE_ROW = 22;

/** The menu. */
export function screen(): Screen {
  const drawn = page("User input", Colour.Green, [
    ["Form", FORM],
    ["Guestbook", GUESTBOOK],
    ["Poll", POLL],
    ["Index", INDEX],
  ]);
  ENTRIES.forEach(([number, name, about], index) => {
    const row = FIRST_ROW + index * ROW_STEP;
    drawn.coloured(row, NUMBER_COLUMN, Colour.Yellow, pageText(number));
    drawn.coloured(row, NAME_COLUMN, Colour.White, name);
    drawn.coloured(row + 1, NAME_COLUMN, Colour.Cyan, about);
  });
  drawn.centred(NOTE_ROW, Colour.Green, "Type on these pages: keys go to us");
  return drawn;
}
