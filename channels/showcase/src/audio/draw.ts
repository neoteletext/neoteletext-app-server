// Drawing the sound pages' parts (crates/teletext-showcase/src/audio/
// draw.rs): labels on a coloured background, and mosaic bars, in Level 1
// attributes (EN 300 706 Table 26).

import { Attribute, type Colour, SIXELS_ACROSS, SIXELS_DOWN, type Screen, Sixels, englishCodes, textColour } from "../kit.ts";

/** The cells a badge's attributes take before its text: the background colour, New Background and the text's colour. */
const BADGE_ATTRIBUTES = 3;

/**
 * A label on a coloured background from `column`: the background colour,
 * New Background, the text's colour, the text, then Black Background, so
 * the label takes its text's length and four cells.
 */
export function badge(screen: Screen, row: number, column: number, background: Colour, foreground: Colour, text: string): void {
  screen.codes(row, column, [textColour(background), Attribute.NewBackground, textColour(foreground)]);
  const after = column + BADGE_ATTRIBUTES;
  screen.text(row, after, text);
  screen.code(row, after + englishCodes(text).length, Attribute.BlackBackground);
}

/** The sixels a bar of `cells` cells and `rows` rows has, across and down. */
export function sixels(cells: number, rows: number): [width: number, height: number] {
  return [cells * SIXELS_ACROSS, rows * SIXELS_DOWN];
}

/**
 * A vertical bar `size[0]` cells wide and `size[1]` rows tall at `row`,
 * `column`, filled from the bottom to `height` sixels, its colour in the
 * cell before.
 */
export function columnBar(screen: Screen, row: number, column: number, size: readonly [number, number], height: number, colour: Colour): void {
  const [width, tall] = sixels(size[0], size[1]);
  const canvas = new Sixels(width, tall);
  for (let y = tall - Math.min(height, tall); y < tall; y += 1) {
    for (let x = 0; x < width; x += 1) {
      canvas.set(x, y, true);
    }
  }
  canvas.draw(screen, row, column, colour);
}

/** The middle sixel row of a cell: the thin track a bar has yet to fill. */
const TRACK_SIXEL_ROW = 1;

/**
 * A horizontal bar `cells` wide at `row`, `column`, filled from the left
 * to `filled` sixels, the rest drawn as a thin track.
 */
export function trackBar(screen: Screen, row: number, column: number, cells: number, filled: number, colour: Colour): void {
  const [width, tall] = sixels(cells, 1);
  const canvas = new Sixels(width, tall);
  for (let x = 0; x < width; x += 1) {
    if (x < filled) {
      for (let y = 0; y < tall; y += 1) {
        canvas.set(x, y, true);
      }
    } else {
      canvas.set(x, TRACK_SIXEL_ROW, true);
    }
  }
  canvas.draw(screen, row, column, colour);
}
