// Page selection, which a channel does itself (INPUT.md section 9.4):
// while keys go to a channel every key goes, digits and colour keys
// included (section 9.1), so the receiver selects nothing. On a page that
// has no use of its own for them, three digits go to that page and the
// coloured keys follow the page's own Fastext links, as a television's
// remote does.

import { type Input, Key, decimalDigit, isCharacterKey } from "./input.ts";
import type { FastextLinks } from "./screen.ts";

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
    if (input.kind === "key" && isCharacterKey(input.key)) {
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
