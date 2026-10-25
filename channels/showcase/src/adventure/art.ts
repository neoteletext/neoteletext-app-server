// Mosaic pictures in colour, drawn as text: each character of a row is
// one sixel, a colour's letter or `.` for none, and each row of cells may
// have a background colour (EN 300 706 clause 15.7.1: a cell is two sixels
// across and three down) (crates/teletext-showcase/src/adventure/art.rs).
//
// A colour changes with a spacing attribute, which takes the cell before
// the change. That cell shows a space, or in Hold Mosaics (clause 12.2,
// Table 26) the mosaic before it; each row takes whichever shows it
// better. A picture reads best when its colours meet across a blank cell,
// or across a run of the same mosaic.

import { Attribute, Colour, SIXELS_ACROSS, SIXELS_DOWN, SPACE, type Screen, mosaicColour, textColour } from "../kit.ts";

/** The character of a sixel that is off. */
export const OFF = ".";
/** The colours of Level 1 (Table 26), counted in a cell. */
const COLOURS = 8;
/** The base of a mosaic character: bit 5 always set (Table 47). */
const MOSAIC_BASE = 0x20;
/**
 * Each sixel's bit in a mosaic character, top-left first, row by row
 * (Table 47: bits 0 to 4, then bit 6 for the bottom right).
 */
const SIXEL_BITS: readonly (readonly number[])[] = [
  [0x01, 0x02],
  [0x04, 0x08],
  [0x10, 0x40],
];
/** The order a cell's colours are weighed in: the first of equal counts wins. */
const WEIGHED = [Colour.White, Colour.Yellow, Colour.Cyan, Colour.Green, Colour.Magenta, Colour.Red, Colour.Blue] as const;
/** The cells before the picture Hold Mosaics takes: two before `column`. */
const HOLD_OFFSET = 2;

/** The colour a letter names. */
export function colourOf(letter: string | undefined): Colour | undefined {
  switch (letter) {
    case "r":
      return Colour.Red;
    case "g":
      return Colour.Green;
    case "y":
      return Colour.Yellow;
    case "b":
      return Colour.Blue;
    case "m":
      return Colour.Magenta;
    case "c":
      return Colour.Cyan;
    case "w":
      return Colour.White;
    default:
      return undefined;
  }
}

/** Whether a picture's character is one the format knows. */
export function known(letter: string): boolean {
  return letter === OFF || colourOf(letter) !== undefined;
}

/** A cell: its mosaic character and the colour most of its sixels have, if any is set. */
type Cell = readonly [code: number, colour: Colour | undefined];

/** A picture: its sixel rows and its cell rows' backgrounds. */
export class Art {
  /** Rows of sixels, each character one sixel. */
  readonly sixels: readonly string[];
  /** One character a cell row: a colour's letter for its background, or `.` for black. */
  readonly backgrounds: string;

  constructor(sixels: readonly string[], backgrounds: string) {
    this.sixels = sixels;
    this.backgrounds = backgrounds;
  }

  /** Its size in sixels: the widest row, and the rows. */
  size(): readonly [across: number, down: number] {
    return [Math.max(0, ...this.sixels.map((row) => [...row].length)), this.sixels.length];
  }

  /** Its size in cells. */
  cells(): readonly [across: number, down: number] {
    const [across, down] = this.size();
    return [Math.ceil(across / SIXELS_ACROSS), Math.ceil(down / SIXELS_DOWN)];
  }

  /** The colour of the sixel at `x`, `y`, if it is on. */
  #sixel(x: number, y: number): Colour | undefined {
    const row = this.sixels[y];
    return row === undefined ? undefined : colourOf([...row][x]);
  }

  /** The cell whose top-left sixel is `x`, `y`: its mosaic character and the colour most of its sixels have, or none when it is empty. */
  #cell(x: number, y: number): Cell {
    let code = MOSAIC_BASE;
    const counts = new Array<number>(COLOURS).fill(0);
    SIXEL_BITS.forEach((bits, down) => {
      bits.forEach((bit, across) => {
        const colour = this.#sixel(x + across, y + down);
        if (colour !== undefined) {
          code |= bit;
          counts[colour] = (counts[colour] ?? 0) + 1;
        }
      });
    });
    let best: readonly [Colour, number] | undefined;
    for (const colour of WEIGHED) {
      const count = counts[colour] ?? 0;
      if (count > 0 && (best === undefined || count > best[1])) {
        best = [colour, count];
      }
    }
    return [code, best?.[0]];
  }

  /**
   * Draws the picture on `screen` with its top-left cell at `row`,
   * `column`. Each row's attributes go in the cells before `column`: the
   * first colour, Hold Mosaics where it serves the row better, and a
   * background at the row's start, so a background is a band across the
   * page. `column` must leave them room: four cells with a background, two
   * without.
   */
  draw(screen: Screen, row: number, column: number): void {
    const [across, down] = this.cells();
    const backgrounds = [...this.backgrounds];
    for (let cellRow = 0; cellRow < down; cellRow += 1) {
      const screenRow = row + cellRow;
      const background = colourOf(backgrounds[cellRow]);
      if (background !== undefined) {
        screen.codes(screenRow, 0, [textColour(background), Attribute.NewBackground]);
      }
      const y = cellRow * SIXELS_DOWN;
      const cells = Array.from({ length: across }, (_, cell) => this.#cell(cell * SIXELS_ACROSS, y));
      const wanted = cells.map(([code]) => code);
      const codes = rowCodes(cells);
      if (mismatches(codes, wanted, true) < mismatches(codes, wanted, false)) {
        screen.code(screenRow, Math.max(0, column - HOLD_OFFSET), Attribute.HoldMosaics);
      }
      screen.codes(screenRow, Math.max(0, column - 1), codes);
    }
  }
}

/** A row's codes from the cell before the picture: the first colour, then each cell, a change of colour taking the cell before it. */
function rowCodes(cells: readonly Cell[]): number[] {
  const first = cells.find(([, colour]) => colour !== undefined)?.[1] ?? Colour.White;
  const codes = [mosaicColour(first)];
  let current: Colour = first;
  for (const [code, colour] of cells) {
    if (colour !== undefined && colour !== current) {
      codes[codes.length - 1] = mosaicColour(colour);
      current = colour;
    }
    codes.push(code);
  }
  return codes;
}

/**
 * How many cells of a row show other than `wanted` (which starts a cell
 * after `codes`), with Hold Mosaics or without: an attribute's cell shows
 * the last mosaic held, or a space (EN 300 706 clause 12.2).
 */
function mismatches(codes: readonly number[], wanted: readonly number[], hold: boolean): number {
  let lastMosaic = SPACE;
  let count = 0;
  codes.slice(1).forEach((code, index) => {
    const want = wanted[index];
    if (want === undefined) {
      return;
    }
    let shown: number;
    if (code < SPACE) {
      shown = hold ? lastMosaic : SPACE;
    } else {
      lastMosaic = code;
      shown = code;
    }
    if (shown !== want) {
      count += 1;
    }
  });
  return count;
}
