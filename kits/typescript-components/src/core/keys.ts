// Keys the components share: the coloured keys, which a keyboard without
// them sends as F1 to F4, as the kit's navigation takes them (INPUT.md
// section 5.6); and what chooses an item or presses a control.

import { Key } from "../kit.ts";
import { isPress, type KeyEvent } from "./events.ts";

/** The coloured keys, red to cyan (EN 300 706 clause 9.6). */
export const COLOUR_KEYS = [Key.Red, Key.Green, Key.Yellow, Key.Blue] as const;
/** The keys a keyboard sends for them. */
export const FUNCTION_KEYS = [Key.F1, Key.F2, Key.F3, Key.F4] as const;
/** The coloured keys' names, red to cyan, as prompts say them. */
export const COLOUR_KEY_NAMES = ["red", "green", "yellow", "cyan"] as const;

/** Which coloured key `event` presses, 0 to 3, or null. */
export function colourKeyOf(event: KeyEvent): number | null {
  const index = COLOUR_KEYS.findIndex((key, place) => isPress(event, key) || isPress(event, FUNCTION_KEYS[place] ?? key));
  return index < 0 ? null : index;
}

/** Whether `event` chooses or presses: Enter or Space, everywhere. */
export function choosing(event: KeyEvent): boolean {
  return isPress(event, Key.Enter) || isPress(event, " ");
}

/** The digit 0 to 9 `event` presses, or null. */
export function digitOf(event: KeyEvent): number | null {
  if (event.action === "release" || !/^[0-9]$/u.test(event.key)) {
    return null;
  }
  return Number.parseInt(event.key, 10);
}
