// A playfield of coloured sixels, drawn as Level 1 mosaics
// (crates/teletext-showcase/src/games/canvas.rs).
//
// At Level 1 a colour is a spacing attribute: it takes a cell of its own,
// and applies from the next cell to the end of the row or the next
// attribute (EN 300 706 clause 12.2, Table 26). The canvas puts each
// colour change in the empty cell before the first cell of the new
// colour; where there is none, that cell takes the colour already in
// force. Games keep their sprites a cell apart where colour matters.

import { Colour, SIXELS_ACROSS, SIXELS_DOWN, SPACE, type Screen, mosaicColour } from "../kit.ts";
import type { Sprite } from "./sprites.ts";

/** Bit 5 of a mosaic code, always set in a block mosaic (EN 300 706 Table 47). */
export const MOSAIC_BIT5 = 0x20;
/** Bit 6 of a mosaic code, the bottom-right sixel's (Table 47). */
const MOSAIC_BIT6 = 0x40;
/** Each sixel's bit in a mosaic character, top-left first, row by row (Table 47: bits 0 to 4, then bit 6; bit 5 is always set). */
export const SIXEL_BITS: readonly number[] = [0x01, 0x02, 0x04, 0x08, 0x10, MOSAIC_BIT6];

/** The colours, by number (Table 26). */
export const COLOURS: readonly Colour[] = [Colour.Black, Colour.Red, Colour.Green, Colour.Yellow, Colour.Blue, Colour.Magenta, Colour.Cyan, Colour.White];

/** The colour numbered `index`. */
function colourOf(index: number): Colour {
  return COLOURS[index] ?? Colour.White;
}

/** A canvas of sixels, each unlit or lit in a colour. */
export class Canvas {
  readonly width: number;
  readonly height: number;
  /** Each sixel: 0 unlit, else its colour's number plus one. */
  readonly #pixels: Uint8Array;

  /** A clear canvas `width` by `height` sixels. */
  constructor(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.#pixels = new Uint8Array(width * height);
  }

  #slot(x: number, y: number): number | undefined {
    return x >= 0 && y >= 0 && x < this.width && y < this.height ? y * this.width + x : undefined;
  }

  /** Lights the sixel at `x`, `y` in `colour`; outside, nothing. */
  plot(x: number, y: number, colour: Colour): void {
    const slot = this.#slot(x, y);
    if (slot !== undefined) {
      this.#pixels[slot] = colour + 1;
    }
  }

  /** Lights a rectangle. */
  rectangle(x: number, y: number, width: number, height: number, colour: Colour): void {
    for (let row = y; row < y + height; row += 1) {
      for (let column = x; column < x + width; column += 1) {
        this.plot(column, row, colour);
      }
    }
  }

  /** Draws `sprite` with its top-left at `x`, `y` in `colour`. */
  sprite(sprite: Sprite, x: number, y: number, colour: Colour): void {
    for (let row = 0; row < sprite.height; row += 1) {
      for (let column = 0; column < sprite.width; column += 1) {
        if (sprite.lit(column, row)) {
          this.plot(x + column, y + row, colour);
        }
      }
    }
  }

  /** The mosaic code of the cell at `cellX`, `cellY` and the colour most of its lit sixels have; undefined for an empty cell. */
  #cell(cellX: number, cellY: number): readonly [code: number, colour: number] | undefined {
    const [x, y] = [cellX * SIXELS_ACROSS, cellY * SIXELS_DOWN];
    let code = MOSAIC_BIT5;
    const counts = new Array<number>(COLOURS.length).fill(0);
    SIXEL_BITS.forEach((bit, index) => {
      const [sx, sy] = [x + (index % SIXELS_ACROSS), y + Math.floor(index / SIXELS_ACROSS)];
      const pixel = sx < this.width ? (this.#pixels[sy * this.width + sx] ?? 0) : 0;
      if (pixel !== 0) {
        code |= bit;
        counts[pixel - 1] = (counts[pixel - 1] ?? 0) + 1;
      }
    });
    // The first colour with the most sixels.
    let [colour, most] = [0, 0];
    counts.forEach((count, index) => {
      if (count > most) {
        [colour, most] = [index, count];
      }
    });
    return most > 0 ? [code, colour] : undefined;
  }

  /**
   * Draws the canvas on `screen` from `row`, `column`, which must be at
   * least 1: the cell before each row is where its first colour goes.
   * The rows' cells it covers are overwritten, empty ones as spaces.
   */
  draw(screen: Screen, row: number, column: number): void {
    const cellsAcross = Math.ceil(this.width / SIXELS_ACROSS);
    const cellsDown = Math.ceil(this.height / SIXELS_DOWN);
    for (let cellY = 0; cellY < cellsDown; cellY += 1) {
      const screenRow = row + cellY;
      screen.code(screenRow, Math.max(0, column - 1), SPACE);
      let current: number | undefined;
      let previousEmpty = true;
      for (let cellX = 0; cellX < cellsAcross; cellX += 1) {
        const screenColumn = column + cellX;
        const cell = this.#cell(cellX, cellY);
        if (cell === undefined) {
          screen.code(screenRow, screenColumn, SPACE);
          previousEmpty = true;
          continue;
        }
        const [code, colour] = cell;
        // A row starts in alphanumerics, so its first lit cell always has
        // an empty cell (or the row's first) before it.
        if (current !== colour && previousEmpty) {
          screen.code(screenRow, Math.max(0, screenColumn - 1), mosaicColour(colourOf(colour)));
          current = colour;
        }
        screen.code(screenRow, screenColumn, code);
        previousEmpty = false;
      }
    }
  }
}
