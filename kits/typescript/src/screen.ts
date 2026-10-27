// A page built as a terminal program builds its screen: text and colour
// at a row and column, mosaic graphics from a sixel canvas, Fastext links,
// a cursor; and the page in TTI, the engine's page format,
// as the server's TTI reader takes it (docs/applications.md section 3.2).
// It mirrors crates/teletext-runtime/src/screen.rs.

import { SPACE_CODE, englishCodes } from "./english.ts";

/** Rows of a page: the header row 0 and rows 1 to 24 (EN 300 706 clause 9.3). */
export const ROWS = 25;
/** Columns of a row (EN 300 706 clause 9.3). */
export const COLUMNS = 40;
/** The first display column of the header, after the receiver's own page number (EN 300 706 clause 9.3.1). */
export const HEADER_TEXT_COLUMN = 8;
/** A mosaic cell's sixels across (EN 300 706 clause 15.7.1). */
export const SIXELS_ACROSS = 2;
/** A mosaic cell's sixels down. */
export const SIXELS_DOWN = 3;
/** The code of a space. */
export const SPACE = SPACE_CODE;
/** The seven bits of a code (EN 300 706 clause 8.2: the eighth is odd parity, the encoder's). */
const CODE_MASK = 0x7f;
/** The page every channel has, where a number outside the channel falls back to. */
export const FIRST_PAGE = 0x100;
/** The last page number a channel may have: magazine 8, page FF (EN 300 706 clause 3.1). */
export const LAST_PAGE = 0x8ff;
/** The most sub-pages one publication may hold (docs/applications.md section 3.2). */
export const MOST_SUBPAGES = 64;

/** The eight colours of Level 1 (EN 300 706 Table 26, spacing attributes). */
export const Colour = {
  Black: 0,
  Red: 1,
  Green: 2,
  Yellow: 3,
  Blue: 4,
  Magenta: 5,
  Cyan: 6,
  White: 7,
} as const;
/** A colour, 0 to 7. */
export type Colour = (typeof Colour)[keyof typeof Colour];

/** The bit that makes a colour's alphanumeric attribute its mosaic one (Table 26: 0x00-0x07 and 0x10-0x17). */
const MOSAIC_COLOUR_BIT = 0x10;

/** The spacing attribute for alphanumeric text in `colour`: 0x00 to 0x07 (Table 26). */
export function textColour(colour: Colour): number {
  return colour;
}

/** The spacing attribute for mosaic graphics in `colour`: 0x10 to 0x17 (Table 26). */
export function mosaicColour(colour: Colour): number {
  return MOSAIC_COLOUR_BIT | colour;
}

/** Spacing attributes of EN 300 706 Table 26. */
export const Attribute = {
  Flash: 0x08,
  Steady: 0x09,
  NormalHeight: 0x0c,
  DoubleHeight: 0x0d,
  Conceal: 0x18,
  Contiguous: 0x19,
  Separated: 0x1a,
  BlackBackground: 0x1c,
  NewBackground: 0x1d,
  HoldMosaics: 0x1e,
  ReleaseMosaics: 0x1f,
} as const;

/** The cursor record's flags (INPUT.md section 7.1; the engine's `neoteletext_input::cursor`). */
export const CursorFlag = {
  /** The receiver echoes what is typed at the cursor. */
  Echo: 1,
  /** It echoes `*` for each character. */
  Masked: 2,
  /** The cursor is a block, not a bar. */
  Block: 4,
} as const;

/** The input cursor and its echo (INPUT.md section 7.1): its cell, the cells it may echo into, and its flags. */
export interface Cursor {
  readonly row: number;
  readonly column: number;
  readonly width: number;
  readonly flags: number;
}

/** Fastext's four coloured links, red, green, yellow and cyan (EN 300 706 clause 9.6), each a page or null. */
export type FastextLinks = readonly [number | null, number | null, number | null, number | null];
/** No Fastext links. */
export const NO_LINKS: FastextLinks = [null, null, null, null];

/** The base of a mosaic character: bit 5 always set (EN 300 706 Table 47). */
const MOSAIC_BASE = 0x20;
/** The bit of the bottom-right sixel, which sits above bit 5 in the code (Table 47). */
const BOTTOM_RIGHT_SIXEL = 0x40;
/** The bits of a mosaic cell's other five sixels, top-left first, across then down (Table 47). */
const SIXEL_BITS = [0x01, 0x02, 0x04, 0x08, 0x10, BOTTOM_RIGHT_SIXEL] as const;

/** A page number written as the protocol and TTI write it: three hexadecimal digits, "1A0". */
export function pageText(page: number): string {
  return page.toString(16).toUpperCase().padStart(3, "0");
}

/** Whether `page` is a page number a channel may have: magazine 1 to 8 (EN 300 706 clause 3.1). */
export function isPageNumber(page: number): boolean {
  return Number.isInteger(page) && page >= FIRST_PAGE && page <= LAST_PAGE;
}

/** Decimal digits are written one to a hexadecimal digit in a sub-code. */
const DECIMAL = 10;
/** The bits of one hexadecimal digit. */
const NIBBLE_BITS = 4;

/**
 * A sub-page's number as a sub-code: its decimal digits, one to a
 * hexadecimal digit, as page numbers are written (sub-page 10 is 0010,
 * as TTI's SC line and receivers show it).
 */
export function decimalSubcode(subpage: number): number {
  const tens = Math.floor(subpage / DECIMAL) % DECIMAL;
  const units = subpage % DECIMAL;
  return (tens << NIBBLE_BITS) | units;
}

/** One sub-page of a publication: the screen and its number. */
export interface Subpage {
  readonly screen: Screen;
  readonly subpage: number;
}

/** A screen being drawn: 25 rows of 40 seven-bit codes, its links and cursor. */
export class Screen {
  readonly #rows: number[][];
  #fastext: FastextLinks = NO_LINKS;
  #index: number | null = null;
  #cursor: Cursor | null = null;
  #fastextRow = false;

  /** A blank screen. */
  constructor() {
    this.#rows = Array.from({ length: ROWS }, () => new Array<number>(COLUMNS).fill(SPACE));
  }

  /** A copy, drawn on apart from this one. */
  clone(): Screen {
    const copy = new Screen();
    this.#rows.forEach((line, row) => copy.codes(row, 0, line));
    copy.#fastext = this.#fastext;
    copy.#index = this.#index;
    copy.#cursor = this.#cursor;
    copy.#fastextRow = this.#fastextRow;
    return copy;
  }

  /** Writes `codes` at `row`, `column`, cut at the row's end; outside the page, nothing. */
  codes(row: number, column: number, codes: readonly number[]): void {
    const line = this.#rows[row];
    if (line === undefined || column < 0) {
      return;
    }
    codes.forEach((code, offset) => {
      const at = column + offset;
      if (at < COLUMNS) {
        line[at] = code & CODE_MASK;
      }
    });
  }

  /** Writes one code. */
  code(row: number, column: number, code: number): void {
    this.codes(row, column, [code]);
  }

  /** Writes `text` at `row`, `column` in the English option set, cut at the row's end. */
  text(row: number, column: number, text: string): void {
    this.codes(row, column, englishCodes(text));
  }

  /** Writes `text` in `colour`: the colour attribute takes the cell before it. */
  coloured(row: number, column: number, colour: Colour, text: string): void {
    this.code(row, column, textColour(colour));
    this.text(row, column + 1, text);
  }

  /** Writes `text` centred on `row` in `colour`. */
  centred(row: number, colour: Colour, text: string): void {
    const width = englishCodes(text).length + 1;
    const column = Math.floor(Math.max(0, COLUMNS - width) / 2);
    this.coloured(row, column, colour, text);
  }

  /** Writes a double-height line: the text on `row`, which also covers `row + 1` (EN 300 706 clause 12.3). */
  doubleHeight(row: number, column: number, colour: Colour, text: string): void {
    this.code(row, column, Attribute.DoubleHeight);
    this.coloured(row, column + 1, colour, text);
  }

  /** Sets a background colour for the rest of `row` from `column`: the colour and New Background take two cells. */
  background(row: number, column: number, colour: Colour): void {
    this.codes(row, column, [textColour(colour), Attribute.NewBackground]);
  }

  /** The header row's text, after the receiver's page number. */
  header(text: string): void {
    this.text(0, HEADER_TEXT_COLUMN, text);
  }

  /** Fills `count` cells of `row` from `column` with `code`. */
  fill(row: number, column: number, count: number, code: number): void {
    this.codes(row, column, new Array<number>(Math.max(0, count)).fill(code));
  }

  /** The code at `row`, `column`; a space outside the page. */
  at(row: number, column: number): number {
    return this.#rows[row]?.[column] ?? SPACE;
  }

  /** A row's codes, or undefined outside the page. */
  row(row: number): readonly number[] | undefined {
    return this.#rows[row];
  }

  /** The text of `row`, for tests: the codes as characters. */
  rowText(row: number): string {
    return String.fromCharCode(...(this.#rows[row] ?? []));
  }

  /** Sets the Fastext links: red, green, yellow, cyan (EN 300 706 clause 9.6). */
  fastext(links: FastextLinks): void {
    this.#fastext = links;
  }

  /** The Fastext links. */
  fastextLinks(): FastextLinks {
    return this.#fastext;
  }

  /** Sets the Index link, which the Index key follows (EN 300 706 clause 9.6, link 6). */
  indexLink(page: number): void {
    this.#index = page;
  }

  /** The Index link. */
  index(): number | null {
    return this.#index;
  }

  /** Shows row 24 although the page has no links (EN 300 706 Table 14, the link control byte's bit 4). */
  showFastextRow(): void {
    this.#fastextRow = true;
  }

  /** Places the cursor at `row`, `column`, with `width` cells the receiver may echo into and `flags` (INPUT.md section 7.1). */
  cursor(row: number, column: number, width: number, flags: number): void {
    this.#cursor = { row, column, width, flags };
  }

  /** The cursor, if placed. */
  cursorLine(): Cursor | null {
    return this.#cursor;
  }

  /** This screen as sub-page `subpage` of page `page`, in TTI. */
  toTti(page: number, subpage: number): string {
    const lines = [`PN,${pageText(page)}${subpage.toString().padStart(2, "0")}`, `SC,${hex(decimalSubcode(subpage), SUBCODE_DIGITS)}`];
    this.#rows.forEach((codes, row) => lines.push(`OL,${row},${rowText(codes)}`));
    const links = [...this.#fastext, null, this.#index];
    if (this.#fastextRow || links.some((link) => link !== null)) {
      lines.push(`FL,${links.map((link) => (link === null ? NO_LINK : pageText(link))).join(",")}`);
    }
    if (this.#cursor !== null) {
      const { row, column, width, flags } = this.#cursor;
      lines.push(`CU,${row},${column},${width},${flags}`);
    }
    return lines.map((line) => `${line}\n`).join("");
  }
}

/** The hexadecimal digits of a sub-code in TTI's SC line. */
const SUBCODE_DIGITS = 4;
/** TTI's FL value for no link (the engine's TTI_FL_NULL_LINK). */
const NO_LINK = "8FF";
/** ESC, which starts an escaped control code in an OL line (the engine's TTI reader, REQ-INP-013). */
const ESCAPE = "\u001b";
/** What is added to a control code after ESC: 0x00 to 0x1F become 0x40 to 0x5F. */
const ESCAPE_OFFSET = 0x40;
/** The last control code: the spacing attributes are 0x00 to 0x1F (Table 26). */
const LAST_CONTROL = 0x1f;

function hex(value: number, digits: number): string {
  return value.toString(16).toUpperCase().padStart(digits, "0");
}

/**
 * A row's codes as an OL line's text in the escape form: each control
 * code as ESC and the code plus 0x40, every other code as its character,
 * so the line is plain ASCII. Trailing spaces are left out; the reader
 * restores them.
 */
function rowText(codes: readonly number[]): string {
  let end = codes.length;
  while (end > 0 && codes[end - 1] === SPACE) {
    end -= 1;
  }
  return codes
    .slice(0, end)
    .map((code) => (code <= LAST_CONTROL ? ESCAPE + String.fromCharCode(code + ESCAPE_OFFSET) : String.fromCharCode(code)))
    .join("");
}

/** Sub-pages of page `page` as one TTI text, each from its PN line. */
export function tti(page: number, subpages: readonly Subpage[]): string {
  return subpages.map(({ screen, subpage }) => screen.toTti(page, subpage)).join("");
}

/** A canvas of sixels, drawn as mosaic graphics into a block of cells. */
export class Sixels {
  readonly width: number;
  readonly height: number;
  readonly #bits: boolean[];

  /** A clear canvas `width` by `height` sixels. */
  constructor(width: number, height: number) {
    this.width = Math.max(0, width);
    this.height = Math.max(0, height);
    this.#bits = new Array<boolean>(this.width * this.height).fill(false);
  }

  /** Sets the sixel at `x`, `y`; outside the canvas, nothing. */
  set(x: number, y: number, on: boolean): void {
    if (x >= 0 && x < this.width && y >= 0 && y < this.height) {
      this.#bits[y * this.width + x] = on;
    }
  }

  /** Whether the sixel at `x`, `y` is set; outside the canvas, not. */
  get(x: number, y: number): boolean {
    return x >= 0 && x < this.width && y >= 0 && y < this.height && this.#bits[y * this.width + x] === true;
  }

  /** Clears the canvas. */
  clear(): void {
    this.#bits.fill(false);
  }

  /** The mosaic character for the cell whose top-left sixel is `x`, `y` (EN 300 706 Table 47). */
  cell(x: number, y: number): number {
    let code = MOSAIC_BASE;
    SIXEL_BITS.forEach((bit, index) => {
      if (this.get(x + (index % SIXELS_ACROSS), y + Math.floor(index / SIXELS_ACROSS))) {
        code |= bit;
      }
    });
    return code;
  }

  /**
   * Draws the canvas as mosaic cells on `screen` from `row`, `column`:
   * each row of cells begins with the mosaic colour attribute, in the cell
   * before `column`.
   */
  draw(screen: Screen, row: number, column: number, colour: Colour): void {
    const cellsAcross = Math.ceil(this.width / SIXELS_ACROSS);
    const cellsDown = Math.ceil(this.height / SIXELS_DOWN);
    for (let cellRow = 0; cellRow < cellsDown; cellRow += 1) {
      if (column > 0) {
        screen.code(row + cellRow, column - 1, mosaicColour(colour));
      }
      for (let cellColumn = 0; cellColumn < cellsAcross; cellColumn += 1) {
        screen.code(row + cellRow, column + cellColumn, this.cell(cellColumn * SIXELS_ACROSS, cellRow * SIXELS_DOWN));
      }
    }
  }
}
