// Page selection, which a channel does itself (INPUT.md section 9.4):
// while keys go to a channel every key goes, digits and colour keys
// included (section 9.1), so the receiver selects nothing. On a page that
// has no use of its own for them, three digits go to that page and the
// coloured keys follow the page's own Fastext links, as a television's
// remote does. Every page is sent its own clicks (section 6.3), so a
// click the page does not take follows what it is on, as the browser
// would: a Fastext prompt in row 24, or a page number.

import { type Input, Key, decimalDigit, isCharacterKey } from "./input.ts";
import { Colour, type FastextLinks, ROWS, isPageNumber, textColour } from "./screen.ts";
import type { Rows } from "./read-tti.ts";

/** Which keys a page uses itself, so navigation leaves them to it. */
export interface PageKeys {
  /** The page takes digits: typed text, a quiz's answers, a poll's votes. */
  readonly ownsDigits?: boolean;
  /** The page takes the coloured keys and F1 to F4. */
  readonly ownsColourKeys?: boolean;
}

/** What navigation made of an input: not its, or taken, with the page to go to if it chose one. */
export type Navigated = { readonly taken: false } | { readonly taken: true; readonly goto: number | null };

/** The keys that follow each Fastext link: F1 to F4 and the remote's coloured keys (INPUT.md section 5.6, codes 92 to 95). */
const COLOUR_KEYS: ReadonlyMap<string, number> = new Map([
  [Key.F1, 0],
  [Key.Red, 0],
  [Key.F2, 1],
  [Key.Green, 1],
  [Key.F3, 2],
  [Key.Yellow, 2],
  [Key.F4, 3],
  [Key.Blue, 3],
]);
/** Digits in a page number. */
const PAGE_DIGITS = 3;
/** A page number's first digit is its magazine, 1 to 8 (EN 300 706 clause 3.1). */
const FIRST_MAGAZINE = 1;
const LAST_MAGAZINE = 8;
/** Page numbers are written in hexadecimal; typed digits are the decimal ones of them. */
const PAGE_RADIX = 16;

/** The row of the Fastext prompts, the page's last (EN 300 706 clause 9.6). */
export const FASTEXT_ROW = ROWS - 1;
/** Each coloured key's prompt colour in row 24, red to cyan (the engine's REQ-NAV-011). */
const PROMPT_COLOURS: readonly number[] = [Colour.Red, Colour.Green, Colour.Yellow, Colour.Cyan].map(textColour);
/** Every alphanumeric colour code, any of which ends the prompt before it (EN 300 706 Table 26). */
const TEXT_COLOURS: ReadonlySet<number> = new Set(Object.values(Colour).map(textColour));
/** The codes of the digits 0 to 9 (EN 300 706 Table 36). */
const DIGIT_ZERO = 0x30;
const DIGIT_NINE = 0x39;

/** What a viewer sees of a page, to follow a click on it: its rows and its Fastext links. */
export interface PageView {
  readonly rows: Rows;
  readonly links: FastextLinks;
}

/** The page a click at a cell goes to: a Fastext prompt's link in row 24, or a page number it is on. */
export function clickedLink(view: PageView, column: number, row: number): number | null {
  const codes = view.rows[row];
  if (codes === undefined || column < 0 || column >= codes.length) {
    return null;
  }
  return row === FASTEXT_ROW ? promptLink(view, column) : numberAt(codes, column);
}

/** The Fastext link of the row 24 prompt at `column`. */
export function promptLink(view: PageView, column: number): number | null {
  const key = promptAt(view.rows[FASTEXT_ROW] ?? [], column);
  return key === null ? null : (view.links[key] ?? null);
}

/** Which coloured key's prompt in row 24 is at `column`, 0 (red) to 3 (cyan): a prompt runs from its colour code to the next colour code. */
export function promptAt(codes: readonly number[], column: number): number | null {
  for (let at = Math.min(column, codes.length - 1); at >= 0; at -= 1) {
    const code = codes[at] ?? 0;
    if (TEXT_COLOURS.has(code)) {
      const key = PROMPT_COLOURS.indexOf(code);
      return key === -1 ? null : key;
    }
  }
  return null;
}

/** The page whose three digits stand at `column` on their own, if any. */
export function numberAt(codes: readonly number[], column: number): number | null {
  const isDigit = (at: number): boolean => {
    const code = codes[at];
    return code !== undefined && code >= DIGIT_ZERO && code <= DIGIT_NINE;
  };
  if (!isDigit(column)) {
    return null;
  }
  let start = column;
  while (start > 0 && isDigit(start - 1)) {
    start -= 1;
  }
  let end = column;
  while (isDigit(end + 1)) {
    end += 1;
  }
  if (end - start + 1 !== PAGE_DIGITS) {
    return null;
  }
  const page = Number.parseInt(String.fromCharCode(...codes.slice(start, end + 1)), PAGE_RADIX);
  return isPageNumber(page) ? page : null;
}

const NOT_TAKEN: Navigated = { taken: false };
const TAKEN: Navigated = { taken: true, goto: null };

/** Each session's digits typed towards a page number. */
export class Navigation {
  readonly #serves: (page: number) => boolean;
  readonly #typed = new Map<number, number[]>();

  /** Navigation among the pages `serves` says the channel has. */
  constructor(serves: (page: number) => boolean) {
    this.#serves = serves;
  }

  /** What `input` from `session` does on a page that uses `keys` itself and shows `links`. */
  take(session: number, input: Input, keys: PageKeys, links: FastextLinks): Navigated {
    if (keys.ownsColourKeys !== true && input.kind === "key") {
      const link = COLOUR_KEYS.get(input.key);
      if (link !== undefined) {
        return { taken: true, goto: input.action === "press" ? (links[link] ?? null) : null };
      }
    }
    if (keys.ownsDigits === true) {
      return NOT_TAKEN;
    }
    if (input.kind === "text") {
      const digits = [...input.text].map(decimalDigit);
      if (digits.length === 0 || digits.some((digit) => digit === undefined)) {
        return NOT_TAKEN;
      }
      let goto: number | null = null;
      for (const digit of digits) {
        goto = this.#digit(session, digit ?? 0) ?? goto;
      }
      return { taken: true, goto };
    }
    // A digit's key, or the keypad's, whose text is its digit (INPUT.md section 5.7 rule 8).
    if (input.kind === "key" && (isCharacterKey(input.key) || input.text !== undefined)) {
      const digit = decimalDigit(input.text ?? input.key);
      if (digit === undefined) {
        return NOT_TAKEN;
      }
      return input.action === "press" ? { taken: true, goto: this.#digit(session, digit) } : TAKEN;
    }
    return NOT_TAKEN;
  }

  /** One more digit from `session`: the page, once three make one the channel has. */
  #digit(session: number, digit: number): number | null {
    const typed = this.#typed.get(session) ?? [];
    if (typed.length === 0 && (digit < FIRST_MAGAZINE || digit > LAST_MAGAZINE)) {
      return null;
    }
    typed.push(digit);
    if (typed.length < PAGE_DIGITS) {
      this.#typed.set(session, typed);
      return null;
    }
    this.#typed.delete(session);
    const page = Number.parseInt(typed.join(""), PAGE_RADIX);
    return this.#serves(page) ? page : null;
  }

  /** The session opened a page: what it had typed towards another is forgotten. */
  reset(session: number): void {
    this.#typed.delete(session);
  }

  /** The session ended. */
  forget(session: number): void {
    this.#typed.delete(session);
  }
}
