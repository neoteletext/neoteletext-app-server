// A component's view of the screen while it paints: its own box, in
// coordinates from the box's top-left, clipped to it. Teletext's spacing
// attributes take a cell each (EN 300 706 clause 12.2), so a coloured text
// spends its first cell on its colour, as the kit's `coloured` does, and a
// background, which runs to the row's end, is ended in the box's last cell
// when the box does not reach the row's end. A component that answers a
// press marks the cells that look pressable as a control (controls.ts):
// a band from its New Background cell to the cell before its Black
// Background, or a text from its first character. Given an id, a control
// is also a region of those cells, so the component acts on a click on
// exactly what the browser shows as pressable and nowhere else. A
// component that needs the pointer's moves with no button held declares a
// pointer area; each move in one is a message to the server, so only where
// a drag or a control will not do.

import { Attribute, type Colour, COLUMNS, englishCodes, mosaicColour, type Screen, type Sixels, SIXELS_ACROSS, SIXELS_DOWN, textColour } from "../kit.ts";
import type { Box } from "./component.ts";

/** The cells a chip takes beyond its text: its background's colour, New Background, its ink, and Black Background after. */
export const CHIP_CELLS = 4;

/** A band's New Background is the cell after its colour's (EN 300 706 Table 26). */
export const NEW_BACKGROUND_CELL = 1;
/** The cell Black Background takes after a band. */
const BLACK_BACKGROUND_CELLS = 1;

/** A region a component marks as it paints, for knowing what a click is on: its name and its cells, on the page. */
export interface Region {
  readonly id: string;
  readonly row: number;
  readonly column: number;
  readonly width: number;
  readonly height: number;
}

/** What painting asks of the page as a whole: its cursor. */
export interface PageRequests {
  cursor: { row: number; column: number; width: number; flags: number } | null;
}

/** What a component declares as it paints, in cells of the page: its controls and pointer areas, and whether it wants the whole page as one. */
export interface Declared {
  readonly controls: Box[];
  readonly pointerAreas: Box[];
  wholePage: boolean;
}

/** Nothing declared yet. */
export function nothingDeclared(): Declared {
  return { controls: [], pointerAreas: [], wholePage: false };
}

export class Canvas {
  readonly box: Box;
  readonly #screen: Screen;
  readonly #requests: PageRequests;
  readonly #regions: Region[];
  readonly #declared: Declared;

  constructor(screen: Screen, box: Box, requests: PageRequests, regions: Region[] = [], declared: Declared = nothingDeclared()) {
    this.#screen = screen;
    this.box = box;
    this.#requests = requests;
    this.#regions = regions;
    this.#declared = declared;
  }

  /**
   * Marks `width` by `height` cells from `row`, `column` of the box as the
   * region `id`: a click there reaches the component with the region's id
   * (events.ts), so it need not lay itself out again to find what was hit.
   */
  region(id: string, row: number, column: number, width: number, height = 1): void {
    this.#regions.push({ id, row: this.box.row + row, column: this.box.column + column, width, height });
  }

  /** `width` by `height` cells from `row`, `column` of the box cut to the box, in cells of the page; null when nothing is left. */
  #inBox(row: number, column: number, width: number, height: number): Box | null {
    const top = Math.max(0, row);
    const left = Math.max(0, column);
    const across = Math.min(this.box.width, column + width) - left;
    const down = Math.min(this.box.height, row + height) - top;
    return across > 0 && down > 0 ? { row: this.box.row + top, column: this.box.column + left, width: across, height: down } : null;
  }

  /**
   * Declares `width` by `height` cells from `row`, `column` of the box, cut
   * to the box, as a control: cells a press acts on, which the browser
   * shows as it shows a link and lights under the pointer itself. With an
   * `id`, the same cells are the region `id`.
   */
  control(row: number, column: number, width: number, height = 1, id?: string): void {
    const cells = this.#inBox(row, column, width, height);
    if (cells === null) {
      return;
    }
    this.#declared.controls.push(cells);
    if (id !== undefined) {
      this.#regions.push({ id, ...cells });
    }
  }

  /** Declares the band `background` sets from `column` on `height` rows from `row` as a control, and with an `id` a region: from its New Background cell to the cell before the Black Background that ends it. */
  bandControl(row: number, column: number, height = 1, id?: string): void {
    this.control(row, column + NEW_BACKGROUND_CELL, this.bandEnd - column - NEW_BACKGROUND_CELL, height, id);
  }

  /** Declares the chip drawn from `column`, which `chip` answered `after` for, as a control, and with an `id` a region: its band, from New Background to the cell before Black Background. */
  chipControl(row: number, column: number, after: number, id?: string): void {
    this.control(row, column + NEW_BACKGROUND_CELL, after - column - NEW_BACKGROUND_CELL - BLACK_BACKGROUND_CELLS, 1, id);
  }

  /**
   * Declares `width` by `height` cells from `row`, `column` of the box, cut
   * to the box, as a pointer area (INPUT.md section 7.4): every move over
   * them comes to this component as onPointer, button held or not. Areas
   * may overlap each other and the controls; a page keeps eight.
   */
  pointerArea(row: number, column: number, width: number, height = 1): void {
    const cells = this.#inBox(row, column, width, height);
    if (cells !== null) {
      this.#declared.pointerAreas.push(cells);
    }
  }

  /** Declares the whole page a pointer area: every move over it comes to this component as onPointer, wherever it is drawn. */
  wholePagePointerArea(): void {
    this.#declared.wholePage = true;
  }

  /** The column a band `background` sets ends before: its Black Background in the box's last cell when the box stops short of the row's end, else the box's end. */
  get bandEnd(): number {
    return this.box.column + this.box.width < COLUMNS ? this.box.width - BLACK_BACKGROUND_CELLS : this.box.width;
  }

  get width(): number {
    return this.box.width;
  }

  get height(): number {
    return this.box.height;
  }

  /** Writes `codes` at `row`, `column` of the box, cut at its edge. */
  codes(row: number, column: number, codes: readonly number[]): void {
    if (row < 0 || row >= this.box.height) {
      return;
    }
    codes.forEach((code, offset) => {
      const at = column + offset;
      if (at >= 0 && at < this.box.width) {
        this.#screen.code(this.box.row + row, this.box.column + at, code);
      }
    });
  }

  code(row: number, column: number, code: number): void {
    this.codes(row, column, [code]);
  }

  /** Writes `text` as it is, no attribute before it. */
  text(row: number, column: number, text: string): void {
    this.codes(row, column, englishCodes(text));
  }

  /** Writes `text` in `colour`: the colour takes the cell at `column`, the text follows. */
  coloured(row: number, column: number, foreground: Colour, text: string): void {
    this.codes(row, column, [textColour(foreground), ...englishCodes(text)]);
  }

  /** Writes the attribute `code`, then `text` in `colour`. */
  attributed(row: number, column: number, code: number, foreground: Colour, text: string): void {
    this.codes(row, column, [code, textColour(foreground), ...englishCodes(text)]);
  }

  /** Sets `background` from `column` across the box on `row` (its colour and New Background take two cells), ending it in the box's last cell when the box stops short of the row's end. */
  background(row: number, column: number, background: Colour): void {
    this.codes(row, column, [textColour(background), Attribute.NewBackground]);
    if (this.box.column + this.box.width < COLUMNS) {
      this.code(row, this.bandEnd, Attribute.BlackBackground);
    }
  }

  /**
   * Writes `text` in `ink` on a `background` of its own from `column`, and
   * ends the background after it: the background's colour, New Background,
   * the ink and Black Background each take a cell (Table 26), so it takes
   * CHIP_CELLS more than the text, the first a cell of space before it.
   * Answers the column after it.
   */
  chip(row: number, column: number, background: Colour, foreground: Colour, text: string): number {
    const codes = englishCodes(text);
    this.codes(row, column, [textColour(background), Attribute.NewBackground, textColour(foreground), ...codes, Attribute.BlackBackground]);
    return column + codes.length + CHIP_CELLS;
  }

  /** Writes mosaic `codes` in `colour` from `column`: the colour takes the cell at `column`. Answers the column after them. */
  mosaics(row: number, column: number, foreground: Colour, codes: readonly number[]): number {
    this.codes(row, column, [mosaicColour(foreground), ...codes]);
    return column + 1 + codes.length;
  }

  /** Writes double-height `text`: the row below is covered across the whole page (clause 12.3). */
  doubleHeight(row: number, column: number, foreground: Colour, text: string): void {
    this.attributed(row, column, Attribute.DoubleHeight, foreground, text);
  }

  /** Fills `count` cells from `column` with `code`. */
  fill(row: number, column: number, count: number, code: number): void {
    this.codes(row, column, new Array<number>(Math.max(0, count)).fill(code));
  }

  /** Draws `sixels` in `colour` with its top-left cell at `row`, `column`: the mosaic colour takes the cell before. */
  sixels(row: number, column: number, foreground: Colour, sixels: Sixels): void {
    const across = Math.ceil(sixels.width / SIXELS_ACROSS);
    const down = Math.ceil(sixels.height / SIXELS_DOWN);
    for (let cellRow = 0; cellRow < down; cellRow += 1) {
      if (column > 0) {
        this.code(row + cellRow, column - 1, mosaicColour(foreground));
      }
      for (let cellColumn = 0; cellColumn < across; cellColumn += 1) {
        this.code(row + cellRow, column + cellColumn, sixels.cell(cellColumn * SIXELS_ACROSS, cellRow * SIXELS_DOWN));
      }
    }
  }

  /** Places the input cursor at `row`, `column` of the box, with `width` cells to echo into. */
  cursor(row: number, column: number, width: number, flags: number): void {
    this.#requests.cursor = { row: this.box.row + row, column: this.box.column + column, width, flags };
  }

  /** The code at `row`, `column` of the box. */
  at(row: number, column: number): number {
    return this.#screen.at(this.box.row + row, this.box.column + column);
  }
}
