// A field a viewer types into, what of their typing a page accepts, and
// the keys that edit it (crates/teletext-showcase/src/interactive/
// field.rs).

import { Attribute, COLUMNS, type Colour, CursorFlag, type Input, Key, type Screen, character, englishCodes, pressed, textColour } from "../kit.ts";

/**
 * The characters the English option set shows as themselves besides
 * letters, digits and the space (EN 300 706 Table 36): `[`, `\`, `]`,
 * `^`, `_`, `` ` ``, `{`, `|`, `}` and `~` are others there (arrows,
 * fractions, `#`, a dash), so they are not accepted. `#` is accepted,
 * since the screen writes it at its English code.
 */
const PUNCTUATION = "!\"#$%&'()*+,-./:;<=>?@£";
const ASCII_ALPHANUMERIC = /^[A-Za-z0-9]$/u;

/** `typed` if a teletext page in the English set shows it as typed: whatever a viewer sends, only these reach a page. */
export function printable(typed: string): string | undefined {
  return ASCII_ALPHANUMERIC.test(typed) || typed === " " || ([...typed].length === 1 && PUNCTUATION.includes(typed)) ? typed : undefined;
}

/** What a viewer's input does to a field: text typed or pasted, one character from a key press, or an editing key. */
export type Edit =
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "character"; readonly character: string }
  | { readonly kind: "backspace" }
  | { readonly kind: "enter" }
  | { readonly kind: "tab" }
  | { readonly kind: "escape" }
  | { readonly kind: "other" };

/**
 * The edit `input` asks for. Raw text, a paste and a character key's
 * press (in ALL_KEYS, INPUT.md section 6.3) all type; releases do nothing.
 */
export function editOf(input: Input): Edit {
  if (input.kind === "text" || input.kind === "paste") {
    return { kind: "text", text: input.text };
  }
  if (pressed(input, Key.Backspace)) {
    return { kind: "backspace" };
  }
  if (pressed(input, Key.Enter)) {
    return { kind: "enter" };
  }
  if (pressed(input, Key.Tab)) {
    return { kind: "tab" };
  }
  if (pressed(input, Key.Escape)) {
    return { kind: "escape" };
  }
  const typed = character(input);
  return typed === undefined ? { kind: "other" } : { kind: "character", character: typed };
}

/** A line of typing, at most `limit` characters, each one a page can show. */
export class Field {
  #text = "";
  readonly #limit: number;

  /** An empty field of at most `limit` characters. */
  constructor(limit: number) {
    this.#limit = limit;
  }

  /** What is typed. */
  text(): string {
    return this.#text;
  }

  /** How many characters are typed. */
  length(): number {
    return [...this.#text].length;
  }

  /** How many more characters the field takes. */
  room(): number {
    return Math.max(0, this.#limit - this.length());
  }

  /** Whether nothing but spaces is typed. */
  isBlank(): boolean {
    return this.#text.trim().length === 0;
  }

  /**
   * Types `typed`, each character through `accept`, which drops it or
   * gives the character to keep; it stops at the limit. The number of
   * characters kept.
   */
  typeText(typed: string, accept: (character: string) => string | undefined): number {
    let kept = 0;
    for (const typedCharacter of typed) {
      const accepted = accept(typedCharacter);
      if (accepted === undefined) {
        continue;
      }
      if (this.length() >= this.#limit) {
        break;
      }
      this.#text += accepted;
      kept += 1;
    }
    return kept;
  }

  /** Deletes the last character: whether there was one. */
  backspace(): boolean {
    if (this.#text.length === 0) {
      return false;
    }
    this.#text = [...this.#text].slice(0, -1).join("");
    return true;
  }

  /** Empties the field. */
  clear(): void {
    this.#text = "";
  }

  /**
   * Puts the cursor after the text at `row`, the text starting at
   * `column`, with the room left for echo: width 0 when full, so the
   * receiver echoes nothing the field would refuse. Echo requires a width
   * and MASKED requires echo (INPUT.md section 7.1, which also bounds
   * column + width by 40), so a full field's cursor has neither.
   */
  cursor(screen: Screen, row: number, column: number, flags: number): void {
    const at = Math.min(column + this.length(), COLUMNS - 1);
    const width = Math.min(this.room(), COLUMNS - at);
    screen.cursor(row, at, width, width === 0 ? flags & ~(CursorFlag.Echo | CursorFlag.Masked) : flags);
  }
}

/** The cells a field band takes before its text: the band's colour, New Background, and the text's colour (EN 300 706 Table 26). */
export const BAND_LEAD = 3;

/**
 * Draws a field's band on `row` from `column`: a `band` background under
 * `limit` characters and the cursor cell after them, `codes` in `ink`, and
 * Black Background after it. The text starts at `column +` {@link BAND_LEAD}.
 */
export function band(screen: Screen, row: number, column: number, limit: number, background: Colour, ink: Colour, codes: readonly number[]): void {
  screen.codes(row, column, [textColour(background), Attribute.NewBackground, textColour(ink)]);
  const start = column + BAND_LEAD;
  screen.codes(row, start, codes);
  // The cell after the last character holds the cursor of a full field.
  screen.code(row, start + limit + 1, Attribute.BlackBackground);
}

/** `text` as the English set's codes, as the screen writes them, for {@link band}. */
export function codesOf(text: string): number[] {
  return englishCodes(text).slice(0, COLUMNS);
}

const ASCII_DIGIT = /^[0-9]$/u;

/** The choice a digit key makes among `count`, numbered from 1: 0 for the first. */
export function choice(input: Input, count: number): number | undefined {
  const typed = character(input);
  if (typed === undefined || !ASCII_DIGIT.test(typed)) {
    return undefined;
  }
  const chosen = Number(typed) - 1;
  return chosen >= 0 && chosen < count ? chosen : undefined;
}
