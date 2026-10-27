// Page 206: a paint box for the mouse, each viewer's own canvas
// (crates/teletext-showcase/src/interactive/paint.rs). A click on it
// comes to the channel as a cell, as every click does (INPUT.md section
// 6.3); a click toggles that block of the canvas in the
// colour chosen on the palette row, and C clears.
//
// Each block of the canvas has its own colour although a colour is a
// spacing attribute that takes a cell (EN 300 706 clause 12.2): a row
// alternates colour attributes and blocks under Hold Mosaics, so each
// attribute's cell shows the block before it again, in that block's
// colour (colours are Set-After, Hold Mosaics Set-At, Table 26). A block
// is two cells wide, the second the held copy. The canvas is blue, set by
// New Background before the blocks, so blue is not on the palette.

import { INDEX, page } from "../common.ts";
import { Attribute, Button, Colour, type Context, type Input, type Screen, character, mosaicColour } from "../kit.ts";
import { Effect, type Sounds } from "../sound.ts";
import { CALCULATOR, MENU, PAINT } from "./pages.ts";

/** Blocks across: from column 4 to 39, two cells each. */
export const ACROSS = 18;
/** Rows of blocks. */
export const DOWN = 17;
/** The canvas's top row. */
export const TOP = 3;
/** The first block's column, after the canvas's colour, New Background, Hold Mosaics and the block's colour attribute. */
export const FIRST_COLUMN = 4;
/** The canvas's colour. */
const CANVAS = Colour.Blue;
/** Cells each block takes: itself and its held copy. */
const BLOCK_CELLS = 2;
/** The palette's row. */
export const PALETTE_ROW = 21;
/** The row that marks the colour chosen. */
const MARK_ROW = 22;
/** The palette's first column: the palette centred. */
export const PALETTE_COLUMN = 5;
/** Cells each colour of the palette takes: its attribute, three blocks and a gap. */
export const SWATCH_CELLS = 5;
/** Blocks in each swatch. */
const SWATCH_BLOCKS = 3;
/** The colours to paint with: all but the canvas's, and black, which Level 1 has no mosaic attribute for (Table 26). */
export const PALETTE: readonly Colour[] = [Colour.Red, Colour.Green, Colour.Yellow, Colour.Magenta, Colour.Cyan, Colour.White];
/** A mosaic cell with all six sixels set (EN 300 706 Table 47). */
export const SOLID = 0x7f;
/** A mosaic cell with none set. */
const EMPTY = 0x20;
/** The row of the page's instructions. */
const HINT_ROW = 23;

/** One viewer's canvas and colour. */
class Canvas {
  readonly blocks: (Colour | null)[][] = Canvas.#empty();
  colour: Colour = Colour.Yellow;

  static #empty(): (Colour | null)[][] {
    return Array.from({ length: DOWN }, () => new Array<Colour | null>(ACROSS).fill(null));
  }

  /** A press of `button` at `column`, `row`: the sound, if it did anything. */
  press(button: number, column: number, row: number): Effect | null {
    if (row === PALETTE_ROW) {
      if (column < PALETTE_COLUMN) {
        return null;
      }
      const chosen = PALETTE[Math.floor((column - PALETTE_COLUMN) / SWATCH_CELLS)];
      if (chosen === undefined) {
        return null;
      }
      this.colour = chosen;
      return Effect.Coin;
    }
    // A block's colour attribute's cell shows the block before it, so it is that block's.
    if (column < FIRST_COLUMN || row < TOP) {
      return null;
    }
    const line = this.blocks[row - TOP];
    const across = Math.floor((column - FIRST_COLUMN) / BLOCK_CELLS);
    if (line === undefined || across >= ACROSS) {
      return null;
    }
    if (button === Button.Left) {
      line[across] = line[across] === this.colour ? null : this.colour;
    } else if (button === Button.Right) {
      line[across] = null;
    } else {
      return null;
    }
    return Effect.Click;
  }

  clear(): void {
    for (const line of this.blocks) {
      line.fill(null);
    }
  }

  screen(): Screen {
    const screen = page("Mouse paint", Colour.Green, [
      ["Input", MENU],
      ["Sums", CALCULATOR],
      ["", null],
      ["Index", INDEX],
    ]);
    this.blocks.forEach((blocks, offset) => {
      const row = TOP + offset;
      screen.codes(row, 0, [mosaicColour(CANVAS), Attribute.NewBackground, Attribute.HoldMosaics]);
      // A colour attribute is needed before every block; an empty block
      // keeps the colour before it, which nothing shows.
      let colour: Colour = Colour.White;
      let column = FIRST_COLUMN - 1;
      for (const block of blocks) {
        colour = block ?? colour;
        screen.code(row, column, mosaicColour(colour));
        screen.code(row, column + 1, block === null ? EMPTY : SOLID);
        column += BLOCK_CELLS;
      }
      // The last block's held copy.
      screen.code(row, column, mosaicColour(colour));
    });
    PALETTE.forEach((colour, index) => {
      const column = PALETTE_COLUMN + index * SWATCH_CELLS;
      screen.code(PALETTE_ROW, column, mosaicColour(colour));
      screen.fill(PALETTE_ROW, column + 1, SWATCH_BLOCKS, SOLID);
      if (colour === this.colour) {
        screen.coloured(MARK_ROW, column, Colour.White, "↑↑↑");
      }
    });
    screen.coloured(HINT_ROW, 1, Colour.White, "Left paints, right rubs out, C clears");
    return screen;
  }
}

/** Every viewer's canvas. */
export class Paint {
  readonly #canvases = new Map<number, Canvas>();

  /** The page as a viewer first sees it, which is also the channel's copy, for a receiver that shows the page before saying it opened it. */
  static firstScreen(): Screen {
    return new Canvas().screen();
  }

  #canvas(session: number): Canvas {
    const canvas = this.#canvases.get(session) ?? new Canvas();
    this.#canvases.set(session, canvas);
    return canvas;
  }

  opened(session: number, context: Context): void {
    context.publish(session, PAINT, this.#canvas(session).screen());
  }

  /** Answers whether the page took `input`: a click off the canvas, such as on a Fastext prompt, is the kit's to follow. */
  input(session: number, input: Input, sounds: Sounds, context: Context): boolean {
    const canvas = this.#canvas(session);
    let effect: Effect | null = null;
    if (input.kind === "mouse") {
      effect = input.action === "press" ? canvas.press(input.button, input.column, input.row) : null;
    } else {
      const typed = character(input);
      if (typed === "c" || typed === "C") {
        canvas.clear();
        effect = Effect.Explosion;
      }
    }
    if (effect === null) {
      return false;
    }
    context.publish(session, PAINT, canvas.screen());
    sounds.play(session, effect, context);
    return true;
  }

  ended(session: number): void {
    this.#canvases.delete(session);
  }
}
