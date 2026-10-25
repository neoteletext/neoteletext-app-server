// What the service pages draw alike (crates/teletext-showcase/src/
// services/draw.rs): word-wrapped text, mosaic pictures from text art,
// large mosaic digits, the demonstration-data banner, and publishing with
// the Fastext row shown.

import { FASTEXT_ROW } from "../common.ts";
import { COLUMNS, Colour, type Context, Screen, Sixels } from "../kit.ts";

export { FASTEXT_ROW } from "../common.ts";
/** The row above it, where the demonstration-data banner goes. */
export const BANNER_ROW = FASTEXT_ROW - 1;
/** The first row below the title band (rows 1 and 2). */
export const FIRST_ROW = 3;
/** The column a body line's colour attribute takes, leaving column 0 as a margin. */
export const BODY_COLUMN = 1;
/** The characters of a body line after its colour attribute. */
export const BODY_WIDTH = 37;
/** The title band's row, where the sub-page counter goes. */
const TITLE_ROW = 1;
/** The column of a band's text, after its background's two cells. */
const BAND_TEXT_COLUMN = 2;

/** The number of characters in `text`. */
export function length(text: string): number {
  return [...text].length;
}

/** `text` broken at spaces into lines of at most `width` characters; a word longer than a line is cut. */
export function wrap(text: string, width: number): string[] {
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(/\s+/u).filter((piece) => piece.length > 0)) {
    const needed = length(line) + length(word) + (line.length > 0 ? 1 : 0);
    if (needed > width && line.length > 0) {
      lines.push(line);
      line = "";
    }
    if (line.length > 0) {
      line += " ";
    }
    line += [...word].slice(0, width).join("");
  }
  if (line.length > 0) {
    lines.push(line);
  }
  return lines;
}

/** Writes `text` wrapped to the body's width from `row` in `colour`, and answers the row after it. */
export function paragraph(screen: Screen, row: number, colour: Colour, text: string): number {
  let next = row;
  for (const line of wrap(text, BODY_WIDTH)) {
    screen.coloured(next, BODY_COLUMN, colour, line);
    next += 1;
  }
  return next;
}

/** A band across `row`: `text` in `colour` on `background`. */
export function band(screen: Screen, row: number, background: Colour, colour: Colour, text: string): void {
  screen.background(row, 0, background);
  screen.coloured(row, BAND_TEXT_COLUMN, colour, text);
}

/** The banner every simulated page carries: its figures are made up. */
export function demonstration(screen: Screen): void {
  band(screen, BANNER_ROW, Colour.Red, Colour.Yellow, "DEMONSTRATION DATA: all simulated");
}

/** The band above the banner naming what the carousel shows next. */
export function next(screen: Screen, what: string): void {
  band(screen, BANNER_ROW - 1, Colour.Blue, Colour.Yellow, `Next: ${what}`);
}

/** Writes `text` in `colour` so that it ends in the last column. */
export function right(screen: Screen, row: number, colour: Colour, text: string): void {
  // The colour attribute takes the cell before the text.
  const width = length(text) + 1;
  screen.coloured(row, Math.max(0, COLUMNS - width), colour, text);
}

/** The sub-page counter, "2/7", at the right of the title band. */
export function counter(screen: Screen, number: number, of: number): void {
  right(screen, TITLE_ROW, Colour.White, `${number}/${of}`);
}

/** Sets the sixels of text art at `x`, `y`: `#` is set, anything else clear. */
export function art(canvas: Sixels, x: number, y: number, lines: readonly string[]): void {
  lines.forEach((line, down) => {
    [...line].forEach((character, across) => {
      if (character === "#") {
        canvas.set(x + across, y + down, true);
      }
    });
  });
}

/** A large digit's width in font pixels. */
export const DIGIT_WIDTH = 5;
/** Its height in font pixels. */
export const DIGIT_HEIGHT = 7;
/** The pixels between characters. */
const DIGIT_GAP = 1;
/**
 * The digits 0 to 9, five pixels by seven, each row's leftmost pixel in
 * bit 4: the dot-matrix shapes of character displays.
 */
const DIGITS: readonly (readonly number[])[] = [
  [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
  [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
  [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
  [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
  [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
  [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
  [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
  [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
  [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
  [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
];
/** The colon, one pixel wide. */
const COLON = [0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00] as const;
/** The colon's width in font pixels. */
const COLON_WIDTH = 1;
const ASCII_DIGIT = /^[0-9]$/u;

/**
 * Draws `text` (digits and colons) in large digits on `canvas` at `x`,
 * `y`, each font pixel `scale` sixels square; answers the width drawn in
 * sixels.
 */
export function digits(canvas: Sixels, x: number, y: number, text: string, scale: number): number {
  let left = x;
  for (const character of text) {
    const [rows, width] = ASCII_DIGIT.test(character) ? [DIGITS[Number(character)] ?? COLON, DIGIT_WIDTH] : [COLON, COLON_WIDTH];
    rows.forEach((bits, down) => {
      for (let across = 0; across < width; across += 1) {
        if (((bits >> (width - 1 - across)) & 1) === 1) {
          square(canvas, left + across * scale, y + down * scale, scale);
        }
      }
    });
    left += (width + DIGIT_GAP) * scale;
  }
  return left - x;
}

/** Sets a square of `size` sixels. */
function square(canvas: Sixels, x: number, y: number, size: number): void {
  for (let down = 0; down < size; down += 1) {
    for (let across = 0; across < size; across += 1) {
      canvas.set(x + across, y + down, true);
    }
  }
}

/**
 * `screen` with its Fastext row shown: a receiver hides row 24 unless
 * X/27/0's "display row 24" flag is set (EN 300 706 clause 9.6.2).
 */
export function withFastextRow(screen: Screen): Screen {
  const shown = screen.clone();
  shown.showFastextRow();
  return shown;
}

/** Publishes `screen` as the channel's page `page`. */
export function publish(context: Context, page: number, screen: Screen): void {
  context.publish("channel", page, withFastextRow(screen));
}

/** Publishes `screens` as the channel's page `page`, sub-pages 1 on, for the receiver to show in turn. */
export function publishCarousel(context: Context, page: number, screens: readonly Screen[]): void {
  context.publishSubpages(
    "channel",
    page,
    screens.map((screen, index) => ({ screen: withFastextRow(screen), subpage: index + 1 })),
  );
}
