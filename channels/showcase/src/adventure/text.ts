// Marked-up text, word-wrapped onto a page in two colours: a span in
// braces, `{like this}`, is highlighted. A colour change costs no cell of
// its own, because the attribute takes the space between two words (EN 300
// 706 clause 12.2: a spacing attribute shows as a space)
// (crates/teletext-showcase/src/adventure/text.rs).

import { type Colour, SPACE, type Screen, textColour } from "../kit.ts";

/** The column text starts at on a full row: after its colour attribute in column 0. */
export const TEXT_COLUMN = 1;
/** The mark that opens a highlighted span. */
const OPEN = "{";
/** The mark that closes it. */
const CLOSE = "}";

/** A word, without its marks, and whether it is highlighted. */
export interface Word {
  /** Its characters. */
  readonly text: string;
  /** Whether it is inside braces. */
  readonly highlight: boolean;
}

/** The characters of `text`, as Rust's `chars().count()` counts them. */
function length(text: string): number {
  return [...text].length;
}

/**
 * The words of `marked`, split at spaces. A word is highlighted when it
 * begins inside a span or with its opening brace, so punctuation after a
 * closing brace takes the word's colour rather than a cell of its own.
 */
export function words(marked: string): Word[] {
  let inside = false;
  const list: Word[] = [];
  for (const token of marked.split(" ").filter((piece) => piece.length > 0)) {
    const highlight = inside || token.startsWith(OPEN);
    for (const character of token) {
      if (character === OPEN) {
        inside = true;
      } else if (character === CLOSE) {
        inside = false;
      }
    }
    const text = [...token].filter((character) => character !== OPEN && character !== CLOSE).join("");
    if (text.length > 0) {
      list.push({ text, highlight });
    }
  }
  return list;
}

/** `word` cut into pieces of at most `width` characters, for a word longer than its line. */
function pieces(word: Word, width: number): Word[] {
  const size = Math.max(1, width);
  const characters = [...word.text];
  const cut: Word[] = [];
  for (let start = 0; start < characters.length; start += size) {
    cut.push({ text: characters.slice(start, start + size).join(""), highlight: word.highlight });
  }
  return cut;
}

/**
 * `marked` laid out into lines, line `n` at most `widthOf(n)` characters
 * wide, and at most `maxLines` of them; what does not fit is left out.
 */
export function wrap(marked: string, widthOf: (line: number) => number, maxLines: number): Word[][] {
  const lines: Word[][] = [];
  let line: Word[] = [];
  let used = 0;
  // A stack: the next word is the last.
  const pending = words(marked).reverse();
  for (let word = pending.pop(); word !== undefined; word = pending.pop()) {
    if (lines.length >= maxLines) {
      break;
    }
    const width = widthOf(lines.length);
    const wordLength = length(word.text);
    const needed = line.length === 0 ? wordLength : used + 1 + wordLength;
    if (needed <= width) {
      used = needed;
      line.push(word);
    } else if (line.length === 0) {
      // Longer than a whole line: cut it, the rest going on.
      pending.push(...pieces(word, width).reverse());
    } else {
      lines.push(line);
      line = [];
      used = 0;
      pending.push(word);
    }
  }
  if (line.length > 0 && lines.length < maxLines) {
    lines.push(line);
  }
  return lines;
}

/** The two colours text is drawn in. */
export interface Style {
  /** Plain words. */
  readonly plain: Colour;
  /** Highlighted words. */
  readonly highlight: Colour;
}

/**
 * Where wrapped text goes: rows from `firstRow`, text from `column` (its
 * colour attribute in the cell before), the first `narrowRows` rows
 * `narrowWidth` wide (beside a picture) and the rest `width`.
 */
export interface Block {
  /** The first row. */
  readonly firstRow: number;
  /** How many rows it may take. */
  readonly rows: number;
  /** The first column of text. */
  readonly column: number;
  /** Rows at the top that are narrower. */
  readonly narrowRows: number;
  /** Their width. */
  readonly narrowWidth: number;
  /** The width of the rest. */
  readonly width: number;
}

/** Draws `marked` into `block` on `screen` in `style`; answers how many rows it took. */
export function draw(screen: Screen, marked: string, block: Block, style: Style): number {
  const widthOf = (line: number): number => (line < block.narrowRows ? block.narrowWidth : block.width);
  const lines = wrap(marked, widthOf, block.rows);
  lines.forEach((line, offset) => {
    const row = block.firstRow + offset;
    let column = block.column;
    let colour: Colour | undefined;
    for (const word of line) {
      const wanted = word.highlight ? style.highlight : style.plain;
      // The cell before the word: the line's first attribute, or the space
      // between words, which becomes the attribute when the colour changes.
      const before = Math.max(0, column - 1);
      if (colour === wanted) {
        screen.code(row, before, SPACE);
      } else {
        screen.code(row, before, textColour(wanted));
        colour = wanted;
      }
      screen.text(row, column, word.text);
      column += length(word.text) + 1;
    }
  });
  return lines.length;
}
