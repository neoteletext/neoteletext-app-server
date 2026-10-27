// Moving through a list, the same in every list: Up and Down an item,
// Page Up and Page Down a screen, Home and End to the ends; the chosen item
// kept in view, moving the view no more than it must; the wheel a line a
// notch, its part turns kept for the next.

import { Key } from "../kit.ts";
import { type KeyEvent, NOTCH } from "./events.ts";

/** How far a key moves the choice in a list showing `shown`, or null for a key that does not. */
export function listStep(event: KeyEvent, shown: number): number | null {
  if (event.action === "release") {
    return null;
  }
  const screen = Math.max(1, shown - 1);
  const steps: Readonly<Record<string, number>> = {
    [Key.Down]: 1,
    [Key.Up]: -1,
    [Key.PageDown]: screen,
    [Key.PageUp]: -screen,
    [Key.Home]: Number.NEGATIVE_INFINITY,
    [Key.End]: Number.POSITIVE_INFINITY,
  };
  return steps[event.key] ?? null;
}

/** The item `step` from `chosen` among `count`, passing over those `can` refuses. */
export function moveChoice(chosen: number, step: number, count: number, can: (index: number) => boolean = () => true): number {
  if (count <= 0) {
    return 0;
  }
  const target = Math.max(0, Math.min(count - 1, Number.isFinite(step) ? chosen + step : step > 0 ? count - 1 : 0));
  const direction = step >= 0 ? 1 : -1;
  for (let index = target; index >= 0 && index < count; index += direction) {
    if (can(index)) {
      return index;
    }
  }
  for (let index = target - direction; index >= 0 && index < count; index -= direction) {
    if (can(index)) {
      return index;
    }
  }
  return chosen;
}

/** The first item shown so `chosen` is in view of `shown`, from `top`. */
export function keepInView(top: number, chosen: number, shown: number, count: number): number {
  const kept = chosen < top ? chosen : chosen >= top + shown ? chosen - shown + 1 : top;
  return Math.max(0, Math.min(kept, Math.max(0, count - shown)));
}

/** A wheel's turns as whole lines, keeping what is left of a notch for the next turn. */
export class WheelLines {
  #turned = 0;

  /** The lines `dy` turns, down positive. */
  lines(dy: number): number {
    this.#turned += dy;
    const lines = Math.trunc(this.#turned / NOTCH);
    this.#turned -= lines * NOTCH;
    return lines;
  }
}
