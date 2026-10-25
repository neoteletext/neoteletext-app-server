// Page 503, the sound board (crates/teletext-showcase/src/audio/board.rs):
// the showcase's effects on the number keys 1 to 9, or a click on their
// buttons, each viewer's page lighting the last one played.

import { Colour, type Screen } from "../kit.ts";
import { Effect } from "../sound.ts";
import { badge } from "./draw.ts";

/** The board's buttons: each effect of the library and its label, keys 1 to 9 in order. */
export const BUTTONS: readonly (readonly [Effect, string])[] = [
  [Effect.Laser, "Laser"],
  [Effect.Explosion, "Boom"],
  [Effect.Hit, "Hit"],
  [Effect.Coin, "Coin"],
  [Effect.Jump, "Jump"],
  [Effect.Win, "Win"],
  [Effect.Lose, "Lose"],
  [Effect.Click, "Click"],
  [Effect.Bell, "Bell"],
];
/** Buttons in a row of the grid. */
const ACROSS = 3;
/** Columns from one button to the next: three attributes, the label's nine cells, and Black Background. */
const BUTTON_STEP = 13;
/** A label's width, padded. */
const LABEL_CELLS = 9;
/** The first row of buttons. */
const FIRST_ROW = 5;
/** Rows from one row of buttons to the next: three rows of button and a gap. */
const ROW_STEP = 4;
/** A button's rows. */
const BUTTON_ROWS = 3;
/** The keys 1 to 9, as Rust's `char::to_digit(10)` reads them: ASCII digits only. */
const DIGIT = /^[0-9]$/u;

/** The button for key `character`, `1` to `9`. */
export function buttonOf(character: string): number | undefined {
  if (!DIGIT.test(character)) {
    return undefined;
  }
  const index = Number(character) - 1;
  return index >= 0 && index < BUTTONS.length ? index : undefined;
}

/** The button at a cell, for the mouse. */
export function buttonAt(column: number, row: number): number | undefined {
  const down = row - FIRST_ROW;
  if (down < 0 || down % ROW_STEP >= BUTTON_ROWS) {
    return undefined;
  }
  const across = Math.floor(column / BUTTON_STEP);
  if (across >= ACROSS) {
    return undefined;
  }
  const index = Math.floor(down / ROW_STEP) * ACROSS + across;
  return index < BUTTONS.length ? index : undefined;
}

/** Draws the board, with `lit` the button last played. */
export function draw(screen: Screen, lit: number | null): void {
  screen.coloured(3, 1, Colour.Cyan, "Press 1 to 9, or click a button");
  BUTTONS.forEach(([, label], index) => {
    const column = (index % ACROSS) * BUTTON_STEP;
    const top = FIRST_ROW + Math.floor(index / ACROSS) * ROW_STEP;
    const [background, foreground] = lit === index ? [Colour.Yellow, Colour.Blue] : [Colour.Blue, Colour.White];
    const text = `${index + 1} ${label}`;
    for (let row = top; row < top + BUTTON_ROWS; row += 1) {
      const shown = row === top + 1 ? text : "";
      badge(screen, row, column, background, foreground, shown.padEnd(LABEL_CELLS));
    }
  });
  const played = (lit === null ? undefined : BUTTONS[lit])?.[1] ?? "nothing yet";
  screen.coloured(18, 1, Colour.White, `Last played: ${played}`);
  screen.coloured(20, 1, Colour.Cyan, "Each effect is sent to you once, then");
  screen.coloured(21, 1, Colour.Cyan, "each press is one 42-byte command.");
}
