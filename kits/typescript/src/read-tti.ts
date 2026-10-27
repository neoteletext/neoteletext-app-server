// TTI read back as the server's reader takes what Screen.toTti writes:
// each sub-page's rows, links and cursor, for tests and for the Fastext
// links of a page published from TTI. It reads the PN, OL, FL and CU
// lines in the escape form, and the packed OL,27 line of links
// that name a sub-page (packed-links.ts), and ignores every other line;
// the server reads TTI with the engine's reader.

import { PACKED_LINKS_ROW, readPackedLinks } from "./packed-links.ts";
import { COLUMNS, type Cursor, ROWS, SPACE } from "./screen.ts";

/** A page as a receiver holds it: each row's codes. */
export type Rows = readonly (readonly number[])[];

/** One sub-page read back from TTI. */
export interface ReadPage {
  readonly subpage: number;
  readonly rows: Rows;
  /** The six links of the FL or packed OL,27 line: red, green, yellow, cyan, link 5 and the index; null for none. */
  readonly links: readonly (number | null)[];
  /** The sub-code each of those links names, null for any sub-page: an FL line names none. */
  readonly linkSubcodes: readonly (number | null)[];
  /** Whether row 24 is shown: an FL line shows it, a packed line says. */
  readonly fastextRow: boolean;
  readonly cursor: Cursor | null;
}

/** TTI's FL value for no link. */
const NO_LINK = 0x8ff;
/** What follows ESC is the control code plus this (the escape form of OL lines). */
const ESCAPE_OFFSET = 0x40;
const ESCAPE = 0x1b;
/** Digits of the sub-page in a PN line, after the page's three. */
const PAGE_DIGITS = 3;
const HEXADECIMAL = 16;
const DECIMAL = 10;

/** Reads what {@link Screen.toTti} writes: PN, SC, OL, FL and CU lines. */
export function readTti(tti: string): ReadPage[] {
  const pages: ReadPage[] = [];
  let rows: number[][] = [];
  let page: {
    address: number;
    subpage: number;
    links: (number | null)[];
    linkSubcodes: (number | null)[];
    fastextRow: boolean;
    cursor: Cursor | null;
  } | null = null;
  const finish = (): void => {
    if (page !== null) {
      const { address: _address, ...read } = page;
      pages.push({ ...read, rows });
    }
  };
  for (const line of tti.split("\n")) {
    const comma = line.indexOf(",");
    const command = line.slice(0, comma);
    const rest = line.slice(comma + 1);
    if (command === "PN") {
      finish();
      rows = Array.from({ length: ROWS }, () => new Array<number>(COLUMNS).fill(SPACE));
      page = {
        address: Number.parseInt(rest.slice(0, PAGE_DIGITS), HEXADECIMAL),
        subpage: Number.parseInt(rest.slice(PAGE_DIGITS), DECIMAL),
        links: [],
        linkSubcodes: [],
        fastextRow: false,
        cursor: null,
      };
    } else if (page === null) {
      continue;
    } else if (command === "OL") {
      const separator = rest.indexOf(",");
      const number = Number.parseInt(rest.slice(0, separator), DECIMAL);
      const row = rows[number];
      const text = rest.slice(separator + 1);
      if (number === PACKED_LINKS_ROW) {
        const read = readPackedLinks(page.address, text);
        if (read !== undefined) {
          page.links = read.links.map((link) => link?.page ?? null);
          page.linkSubcodes = read.links.map((link) => link?.subcode ?? null);
          page.fastextRow = read.displayRow24;
        }
        continue;
      }
      let column = 0;
      for (let at = 0; at < text.length && row !== undefined; at += 1) {
        let code = text.charCodeAt(at);
        if (code === ESCAPE) {
          at += 1;
          code = text.charCodeAt(at) - ESCAPE_OFFSET;
        }
        row[column] = code;
        column += 1;
      }
    } else if (command === "FL") {
      page.links = rest.split(",").map((field) => {
        const link = Number.parseInt(field, HEXADECIMAL);
        return link === NO_LINK ? null : link;
      });
      page.linkSubcodes = page.links.map(() => null);
      page.fastextRow = true;
    } else if (command === "CU") {
      const [row = 0, column = 0, width = 0, flags = 0] = rest.split(",").map((field) => Number.parseInt(field, DECIMAL));
      page.cursor = { row, column, width, flags };
    }
  }
  finish();
  return pages;
}

/** A row's codes as text. */
export function text(rows: Rows, row: number): string {
  return String.fromCharCode(...(rows[row] ?? []));
}

/** The first row holding `wanted`, or undefined. */
export function rowOf(rows: Rows, wanted: string): number | undefined {
  for (let row = 0; row < ROWS; row += 1) {
    if (text(rows, row).includes(wanted)) {
      return row;
    }
  }
  return undefined;
}

/** Whether any row holds `wanted`. */
export function shows(rows: Rows, wanted: string): boolean {
  return rowOf(rows, wanted) !== undefined;
}

