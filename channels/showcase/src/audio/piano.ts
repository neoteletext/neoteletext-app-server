// Page 502, the piano (crates/teletext-showcase/src/audio/piano.rs): an
// octave from middle C on the letter keys, by position, so it plays the
// same on any layout (INPUT.md's `base`, the United States layout's
// character of the key pressed). Each viewer's page shows the keys they
// hold until they let go.

import { Colour, type Screen, mosaicColour } from "../kit.ts";
import { NOTES } from "../sound.ts";
import { badge } from "./draw.ts";
import { asciiLower, noteName } from "./tracker.ts";

/**
 * The keys of the octave, C to C, by their United States characters: the
 * home row is the white keys, the row above it the black, as on a
 * tracker's keyboard.
 */
export const KEYS = ["a", "w", "s", "e", "d", "f", "t", "g", "y", "h", "u", "j", "k"] as const;
if (KEYS.length !== NOTES) {
  throw new Error("a key for each note of the library's octave");
}
/** The notes of the octave that are white keys. */
const WHITE = [0, 2, 4, 5, 7, 9, 11, 12] as const;
/** Middle C's MIDI number: the library's first note. */
const MIDDLE_C = 60;
/** Semitones in an octave. */
const OCTAVE = 12;
/** The black keys' places in the octave. */
const BLACK_IN_OCTAVE: ReadonlySet<number> = new Set([1, 3, 6, 8, 10]);

/** The note of `key`, 0 to 12, by its character on a United States layout. */
export function noteOf(key: string): number | undefined {
  const index = (KEYS as readonly string[]).indexOf(asciiLower(key));
  return index === -1 ? undefined : index;
}

/** Whether `note` is a black key. */
function isBlack(note: number): boolean {
  return BLACK_IN_OCTAVE.has(note % OCTAVE);
}

/** What a viewer holds. */
export interface Held {
  /** A bit for each note held down by a key. */
  keys: number;
  /** The note held down by the mouse. */
  mouse: number | null;
  /** The last note played. */
  last: number | null;
}

/** Nothing held, `last` the last note played. */
export function released(last: number | null = null): Held {
  return { keys: 0, mouse: null, last };
}

/** Whether `note` is held, by a key or the mouse. */
export function holds(held: Held, note: number): boolean {
  return (held.keys & (1 << note)) !== 0 || held.mouse === note;
}

/** The first white key's colour cell. */
const KEYS_COLUMN = 4;
/** Columns from one white key to the next: a colour cell and three mosaic cells. */
const KEY_STEP = 4;
/** The row of the black keys' letters. */
const BLACK_LABELS = 6;
/** The black keys' rows, above the white keys' lower part: 7 to 10. */
const UPPER_ROWS = [7, 11] as const;
/** The white keys' lower part: 11 and 12. */
const LOWER_ROWS = [11, 13] as const;
/** The row of the white keys' letters. */
const WHITE_LABELS = 13;
/** The row of the notes' names. */
const NOTE_NAMES = 14;
/** A full mosaic cell: every sixel set (EN 300 706 Table 47). */
const SOLID = 0x7f;
/** The cell of a white key that holds the black key to its right: its last. */
const LAST_CELL = 3;

/** Whether `row` is in the half-open range `rows`. */
function within(rows: readonly [number, number], row: number): boolean {
  return row >= rows[0] && row < rows[1];
}

/** The colour of `note`'s key, held or not. */
function keyColour(note: number, held: Held): Colour {
  if (holds(held, note)) {
    return Colour.Red;
  }
  return isBlack(note) ? Colour.Blue : Colour.White;
}

/** The first column of white key `index`, its colour cell. */
function whiteColumn(index: number): number {
  return KEYS_COLUMN + index * KEY_STEP;
}

/** The key's letter, upper case, for the page. */
function keyLabel(note: number): string {
  return (KEYS[note] ?? " ").toUpperCase();
}

/** Draws the keyboard with what is held. */
export function draw(screen: Screen, held: Held): void {
  screen.coloured(3, 1, Colour.Cyan, "Play with the keys, or click them");
  WHITE.forEach((note, index) => {
    const column = whiteColumn(index);
    const colour = keyColour(note, held);
    const blackLeft = note > 0 && isBlack(note - 1);
    const blackRight = note < NOTES - 1 && isBlack(note + 1);
    for (let row = UPPER_ROWS[0]; row < UPPER_ROWS[1]; row += 1) {
      // A black key takes the cell either side of the white keys'
      // boundary: its colour in the left key's last cell, its mosaic in
      // the right key's colour cell.
      if (blackLeft) {
        screen.codes(row, column, [SOLID, mosaicColour(colour), SOLID]);
      } else {
        screen.codes(row, column, [mosaicColour(colour), SOLID, SOLID]);
      }
      const last = blackRight ? mosaicColour(keyColour(note + 1, held)) : SOLID;
      screen.code(row, column + LAST_CELL, last);
    }
    for (let row = LOWER_ROWS[0]; row < LOWER_ROWS[1]; row += 1) {
      screen.codes(row, column, [mosaicColour(colour), SOLID, SOLID, SOLID]);
    }
    const middle = column + 2;
    screen.coloured(WHITE_LABELS, middle - 1, Colour.Yellow, keyLabel(note));
    const name = noteName(MIDDLE_C + note);
    screen.coloured(NOTE_NAMES, middle - 1, Colour.White, name.slice(0, 1));
    if (blackRight) {
      screen.coloured(BLACK_LABELS, column + LAST_CELL, Colour.Yellow, keyLabel(note + 1));
    }
  });
  const playing = Array.from({ length: NOTES }, (_, note) => note)
    .filter((note) => holds(held, note))
    .map((note) => noteName(MIDDLE_C + note));
  if (playing.length === 0) {
    const last = held.last === null ? "none yet" : noteName(MIDDLE_C + held.last);
    screen.coloured(16, 1, Colour.White, `Last note: ${last}`);
  } else {
    badge(screen, 16, 0, Colour.Red, Colour.White, ` Playing ${playing.join(" ")} `);
  }
  screen.coloured(18, 1, Colour.Green, "White keys");
  screen.coloured(18, 13, Colour.Yellow, "A S D F G H J K");
  screen.coloured(19, 1, Colour.Green, "Black keys");
  screen.coloured(19, 13, Colour.Yellow, "W E   T Y U");
  screen.coloured(21, 1, Colour.Cyan, "Keys go by where they are, so every");
  screen.coloured(22, 1, Colour.Cyan, "keyboard layout plays the same notes.");
}

/** The note of the key at a cell, for the mouse. */
export function noteAt(column: number, row: number): number | undefined {
  const first = KEYS_COLUMN;
  const last = whiteColumn(WHITE.length);
  if (column < first || column >= last || !(within(UPPER_ROWS, row) || within(LOWER_ROWS, row))) {
    return undefined;
  }
  const offset = column - first;
  const index = Math.floor(offset / KEY_STEP);
  const inKey = offset % KEY_STEP;
  const note = WHITE[index];
  if (note === undefined) {
    return undefined;
  }
  if (within(UPPER_ROWS, row)) {
    // The black key's two cells: the left key's last, the right key's
    // first.
    if (inKey === LAST_CELL && note < NOTES - 1 && isBlack(note + 1)) {
      return note + 1;
    }
    if (inKey === 0 && note > 0 && isBlack(note - 1)) {
      return note - 1;
    }
  }
  return note;
}
