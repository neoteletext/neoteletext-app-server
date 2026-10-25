// TTI read back as the server's reader takes what Screen.toTti writes:
// each sub-page's rows, links, cursor and modes, for tests and for the
// Fastext links of a page published from TTI. It reads the PN, OL, FL,
// CU and IM lines in the escape form and ignores every other line; the
// server reads TTI with the engine's reader.

import { COLUMNS, type Cursor, ROWS, SPACE } from "./screen.ts";

/** A page as a receiver holds it: each row's codes. */
export type Rows = readonly (readonly number[])[];

/** One sub-page read back from TTI. */
export interface ReadPage {
  readonly subpage: number;
  readonly rows: Rows;
  /** The six links of the FL line: red, green, yellow, cyan, link 5 and the index; null for none. */
  readonly links: readonly (number | null)[];
  /** Whether an FL line was present, which shows row 24. */
  readonly fastextRow: boolean;
  readonly cursor: Cursor | null;
  readonly modes: number;
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

/** Reads what {@link Screen.toTti} writes: PN, SC, OL, FL, CU and IM lines. */
export function readTti(tti: string): ReadPage[] {
  const pages: ReadPage[] = [];
  let rows: number[][] = [];
  let page: { subpage: number; links: (number | null)[]; fastextRow: boolean; cursor: Cursor | null; modes: number } | null = null;
  const finish = (): void => {
    if (page !== null) {
      pages.push({ ...page, rows });
    }
  };
  for (const line of tti.split("\n")) {
    const comma = line.indexOf(",");
    const command = line.slice(0, comma);
    const rest = line.slice(comma + 1);
    if (command === "PN") {
      finish();
      rows = Array.from({ length: ROWS }, () => new Array<number>(COLUMNS).fill(SPACE));
      page = { subpage: Number.parseInt(rest.slice(PAGE_DIGITS), DECIMAL), links: [], fastextRow: false, cursor: null, modes: 0 };
    } else if (page === null) {
      continue;
    } else if (command === "OL") {
      const separator = rest.indexOf(",");
      const row = rows[Number.parseInt(rest.slice(0, separator), DECIMAL)];
      const text = rest.slice(separator + 1);
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
      page.fastextRow = true;
    } else if (command === "CU") {
      const [row = 0, column = 0, width = 0, flags = 0] = rest.split(",").map((field) => Number.parseInt(field, DECIMAL));
      page.cursor = { row, column, width, flags };
    } else if (command === "IM") {
      page.modes = Number.parseInt(rest, DECIMAL);
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

