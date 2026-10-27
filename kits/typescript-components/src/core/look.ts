// How every component looks in each state, so a chosen item, a focused
// control, a disabled one, an empty list and a notice's kind look the same
// on every page. Teletext has eight colours (EN 300 706 Table 26); each
// state is also told by a mark or a word, never by colour alone.

import { Colour } from "../kit.ts";
import type { Canvas } from "./canvas.ts";
import { fit } from "./text.ts";

/** The chosen item of the list that has the keys: blue on a yellow band. */
export const CHOSEN = { background: Colour.Yellow, foreground: Colour.Blue } as const;
/** The chosen item of a list that does not have the keys: cyan, after a mark. */
export const SELECTED = Colour.Cyan;
/** What marks a chosen item without the band, and a focused control's label. */
export const MARK = ">";
/** A label, and a label whose control has the keys. */
export const LABEL = Colour.White;
export const FOCUSED_LABEL = Colour.Yellow;
/** What cannot be used: blue, the dimmest foreground on black. */
export const DISABLED = Colour.Blue;
/** A band that cannot be used, a button's or a field's: no band, its text in DISABLED. */
export const DISABLED_BAND = { background: Colour.Black, foreground: DISABLED } as const;
/** A chip that cannot be used, a box's, a switch's, a step's: black on blue, as dim as a chip goes and still read. */
export const DISABLED_CHIP = { background: Colour.Blue, foreground: Colour.Black } as const;
/** What a component with nothing to show says it in. */
export const EMPTY = Colour.Cyan;
/** A control's label column, when it has a label and is not told its width. */
export const LABEL_CELLS = 12;
/** Where an item's text starts in a list: after the band's two cells (its colour and New Background) and the foreground's cell. */
export const ITEM_TEXT = 3;

/** What a notice, a toast or a dialog means. */
export type Kind = "info" | "success" | "warning" | "error";

/** Each kind's colours and the mark that says it without colour. */
export const KINDS: Readonly<Record<Kind, { readonly background: Colour; readonly foreground: Colour; readonly mark: string }>> = {
  info: { background: Colour.Cyan, foreground: Colour.Blue, mark: "i" },
  success: { background: Colour.Green, foreground: Colour.Black, mark: "OK" },
  warning: { background: Colour.Yellow, foreground: Colour.Black, mark: "!" },
  error: { background: Colour.Red, foreground: Colour.White, mark: "X" },
};

/** A control's state, as it paints. */
export interface ControlLook {
  readonly focused: boolean;
  readonly disabled: boolean;
}

/** A control's label in `width` cells: marked and yellow while it has the keys, blue while disabled. */
export function paintLabel(canvas: Canvas, row: number, label: string, width: number, look: ControlLook): void {
  const foreground = look.disabled ? DISABLED : look.focused ? FOCUSED_LABEL : LABEL;
  canvas.coloured(row, 0, foreground, fit(`${look.focused && !look.disabled ? MARK : " "}${label}`, width - 1));
}

/** What a component with nothing to show says, on its first row. */
export function paintEmpty(canvas: Canvas, text: string): void {
  canvas.coloured(0, 0, EMPTY, fit(`(${text})`, canvas.width - 1));
}
