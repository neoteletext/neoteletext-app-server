// Input as a channel's program receives it (docs/applications.md section
// 2): the INPUT.md events the server decodes, each checked whole as it
// arrives, since what the server sends is read like any other input. It
// mirrors crates/teletext-runtime/src/input.rs.

/** Key names the showcase uses: W3C `KeyboardEvent.key` names, as INPUT.md section 5.6 names the functional keys. */
export const Key = {
  Up: "ArrowUp",
  Down: "ArrowDown",
  Left: "ArrowLeft",
  Right: "ArrowRight",
  Enter: "Enter",
  Escape: "Escape",
  Backspace: "Backspace",
  Delete: "Delete",
  Tab: "Tab",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  F1: "F1",
  F2: "F2",
  F3: "F3",
  F4: "F4",
  Red: "ColorF0Red",
  Green: "ColorF1Green",
  Yellow: "ColorF2Yellow",
  Blue: "ColorF3Blue",
} as const;

/** What a key did. */
export type KeyAction = "press" | "repeat" | "release";

/** What the pointer did at a control: came onto it or went off it. */
export type HoverAction = "enter" | "leave";

/** One thing the viewer did on the page open with the keys. */
export type Input =
  /** Text typed: whole characters. */
  | { readonly kind: "text"; readonly text: string }
  /** A key pressed, repeating or released: every one is sent, with the `text` it produced and its `base` when that differs (INPUT.md sections 5.5 and 6.3). */
  | { readonly kind: "key"; readonly action: KeyAction; readonly key: string; readonly text?: string; readonly base?: string }
  /** A mouse button pressed or released on a cell. */
  | { readonly kind: "mouse"; readonly action: "press" | "release"; readonly button: number; readonly column: number; readonly row: number }
  /** The pointer moved over the page, a button held or not (INPUT.md section 6.3); `buttons` are those held, a bit each ({@link Held}). */
  | { readonly kind: "mouse"; readonly action: "move"; readonly buttons: number; readonly column: number; readonly row: number }
  /** The wheel turned over a cell, in 1/120ths of a line, positive right and down. */
  | { readonly kind: "wheel"; readonly dx: number; readonly dy: number; readonly column: number; readonly row: number }
  /** The pointer entered a control of the page or left it (INPUT.md section 5.4, HOVER): `column` and `row` are the control's first cell, its top-left, as the page declared it; a move from one control to the next is a leave, then an enter. */
  | { readonly kind: "hover"; readonly action: HoverAction; readonly column: number; readonly row: number }
  /** A paste: data, never keystrokes. */
  | { readonly kind: "paste"; readonly text: string }
  /** The keys came to the page. */
  | { readonly kind: "focus" }
  /** The keys left it: release what is held. */
  | { readonly kind: "blur" };

/** The mouse's buttons (INPUT.md section 5.4). */
export const Button = { Left: 0, Middle: 1, Right: 2 } as const;
/** The highest button number INPUT.md section 5.4 sends. */
const LAST_BUTTON = Button.Right;
/** The buttons a move says are held, a bit each (docs/applications.md section 2). */
export const Held = { Left: 1 << Button.Left, Middle: 1 << Button.Middle, Right: 1 << Button.Right } as const;
/** Every button held at once. */
const ALL_HELD = Held.Left | Held.Middle | Held.Right;
/** The longest text one event may carry: the server's line bound (docs/applications.md section 1). */
const LONGEST_TEXT = 16 * 1024 * 1024;
/** The longest key name INPUT.md section 5.6 has ("BrowserFavorites" and the like), with room. */
const LONGEST_KEY_NAME = 64;
/** The largest cell coordinate: a page is 40 by 25, and INPUT.md bounds a position by a 32-bit field. */
const LARGEST_COORDINATE = 0xffff_ffff;
/** The largest wheel turn either way in one event (INPUT.md section 5.4: a signed 32-bit count). */
const LARGEST_TURN = 0x7fff_ffff;

/** Whether `key` names a character key: one character, the layout's, rather than a functional key's name. */
export function isCharacterKey(key: string): boolean {
  return [...key].length === 1;
}

/** Whether this is a press (or a repeat) of `key`. */
export function pressed(input: Input, key: string): boolean {
  return input.kind === "key" && (input.action === "press" || input.action === "repeat") && input.key === key;
}

/** Whether this is a release of `key`. */
export function released(input: Input, key: string): boolean {
  return input.kind === "key" && input.action === "release" && input.key === key;
}

/** Whether this is the main button pressed: a click. */
export function clicked(input: Input): boolean {
  return input.kind === "mouse" && input.action === "press" && input.button === Button.Left;
}

/** Control characters, C0, DEL and C1 (Unicode's `Cc`): never text (INPUT.md section 5.3). */
export const CONTROL_CHARACTER = /\p{Cc}/u;

/** The text a key's press or repeat produced, if it typed any: a release types nothing, and a key whose text is a control character (Enter, Tab) is a key, not typing. */
export function keyText(action: KeyAction, text: string | undefined): string | undefined {
  return action === "release" || text === undefined || text.length === 0 || CONTROL_CHARACTER.test(text) ? undefined : text;
}

/**
 * The text this input typed, if any. Typing comes two ways (INPUT.md
 * sections 5.3 and 6.3): a key's press or repeat carries the text it
 * produced, and raw text carries what no key did, an input method's or a
 * phone's. A paste is data, never typing.
 */
export function typed(input: Input): string | undefined {
  if (input.kind === "text") {
    return input.text;
  }
  return input.kind === "key" ? keyText(input.action, input.text) : undefined;
}

/** The character this input typed, if it typed exactly one: from raw text or a key's press or repeat; a key that produced no text (Control and a letter) types nothing. */
export function character(input: Input): string | undefined {
  const text = typed(input);
  const characters = text === undefined ? [] : [...text];
  return characters.length === 1 ? characters[0] : undefined;
}

type Fields = Readonly<Record<string, unknown>>;

function isFields(value: unknown): value is Fields {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function text(fields: Fields, name: string, longest: number): string | undefined {
  const value = fields[name];
  return typeof value === "string" && value.length <= longest ? value : undefined;
}

function whole(fields: Fields, name: string, least: number, most: number): number | undefined {
  const value = fields[name];
  return typeof value === "number" && Number.isInteger(value) && value >= least && value <= most ? value : undefined;
}

/** An event as the server writes it, checked; or why it is not one. */
export function readInput(value: unknown): Input | string {
  if (!isFields(value)) {
    return "event: an object is needed";
  }
  switch (value["kind"]) {
    case "text":
    case "paste": {
      const typed = text(value, "text", LONGEST_TEXT);
      if (typed === undefined) {
        return `event: ${String(value["kind"])} needs text`;
      }
      return value["kind"] === "text" ? { kind: "text", text: typed } : { kind: "paste", text: typed };
    }
    case "key": {
      const action = value["action"];
      const key = text(value, "key", LONGEST_KEY_NAME);
      if ((action !== "press" && action !== "repeat" && action !== "release") || key === undefined || key.length === 0) {
        return "event: a key needs an action and a key";
      }
      const typed = value["text"] === undefined ? undefined : text(value, "text", LONGEST_TEXT);
      const base = value["base"] === undefined ? undefined : text(value, "base", LONGEST_KEY_NAME);
      if ((value["text"] !== undefined && typed === undefined) || (value["base"] !== undefined && base === undefined)) {
        return "event: a key's text and base are text";
      }
      return { kind: "key", action, key, ...(typed === undefined ? {} : { text: typed }), ...(base === undefined ? {} : { base }) };
    }
    case "mouse": {
      const action = value["action"];
      const column = whole(value, "column", 0, LARGEST_COORDINATE);
      const row = whole(value, "row", 0, LARGEST_COORDINATE);
      if (column === undefined || row === undefined) {
        return "event: a mouse event needs a column and a row";
      }
      if (action === "move") {
        const buttons = whole(value, "buttons", 0, ALL_HELD);
        return buttons === undefined ? "event: a move needs the buttons held" : { kind: "mouse", action, buttons, column, row };
      }
      const button = whole(value, "button", 0, LAST_BUTTON);
      if ((action !== "press" && action !== "release") || button === undefined) {
        return "event: a mouse event needs an action, press, release or move, and a press or release a button";
      }
      return { kind: "mouse", action, button, column, row };
    }
    case "wheel": {
      const dx = whole(value, "dx", -LARGEST_TURN, LARGEST_TURN);
      const dy = whole(value, "dy", -LARGEST_TURN, LARGEST_TURN);
      const column = whole(value, "column", 0, LARGEST_COORDINATE);
      const row = whole(value, "row", 0, LARGEST_COORDINATE);
      if (dx === undefined || dy === undefined || column === undefined || row === undefined) {
        return "event: a wheel event needs dx, dy, column and row";
      }
      return { kind: "wheel", dx, dy, column, row };
    }
    case "hover": {
      const action = value["action"];
      const column = whole(value, "column", 0, LARGEST_COORDINATE);
      const row = whole(value, "row", 0, LARGEST_COORDINATE);
      if ((action !== "enter" && action !== "leave") || column === undefined || row === undefined) {
        return "event: a hover needs an action, enter or leave, a column and a row";
      }
      return { kind: "hover", action, column, row };
    }
    case "focus":
      return { kind: "focus" };
    case "blur":
      return { kind: "blur" };
    default:
      return `event: unknown kind ${JSON.stringify(value["kind"]) ?? "undefined"}`;
  }
}

/** Code points around a decimal digit walked in search of its run's zero: the longest run of `Nd` characters, Unicode's five mathematical digit runs at U+1D7CE, is 50. */
const LONGEST_DIGIT_RANGE = 50;
/** Digits in a run (Unicode section 4.6, "Numeric Value"). */
const RADIX = 10;
const DECIMAL_DIGIT = /^\p{Nd}$/u;

/**
 * The value of a decimal digit of any script, Unicode's general category
 * Nd (INPUT.md section 5.7 rule 8), or undefined for any other
 * character. Every Nd character is one of a run of ten, zero to nine in
 * order, and runs that touch begin at a zero, so a digit's value is its
 * distance from the start of its range, modulo ten.
 */
export function decimalDigit(character: string): number | undefined {
  if (!DECIMAL_DIGIT.test(character)) {
    return undefined;
  }
  const point = character.codePointAt(0) ?? 0;
  let start = point;
  while (point - start < LONGEST_DIGIT_RANGE && DECIMAL_DIGIT.test(String.fromCodePoint(start - 1))) {
    start -= 1;
  }
  return (point - start) % RADIX;
}
