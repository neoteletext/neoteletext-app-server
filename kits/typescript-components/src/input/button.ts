// A button: its label on a band, pressed with Enter, Space or a click. It
// sends activate up the tree (core/component-events.ts) and calls onPress.
// It fits its label with a cell of padding each side unless told its width
// or its padding, so buttons stacked together can be one width. Its band
// is a control (core/controls.ts), which the browser lights as the pointer
// passes, and only a click on it presses: a stack gives the button the
// whole row, and the cells after the band are not the button's to show. A
// disabled button declares none.

import { type Canvas, NEW_BACKGROUND_CELL } from "../core/canvas.ts";
import type { Size } from "../core/component.ts";
import type { KeyEvent, PointerEvent } from "../core/events.ts";
import { choosing } from "../core/keys.ts";
import { CHOSEN, DISABLED_BAND } from "../core/look.ts";
import { cells, fit } from "../core/text.ts";
import { Attribute, Colour } from "../kit.ts";
import { Control, type ControlProps } from "./control.ts";

export interface ButtonProps extends ControlProps {
  readonly label: string;
  readonly onPress: (session: number) => void;
  /** Its band's colour while it does not have the keys. */
  readonly background?: Colour;
  /** Its whole width in cells, the label centred in it; fitted to the label and its padding when absent. */
  readonly width?: number;
  /** Spaces before and after the label inside the band: one each when absent. */
  readonly paddingLeft?: number;
  readonly paddingRight?: number;
}

/** The band's two cells (its colour and New Background) and the foreground's cell before the label. */
const LEAD_CELLS = 3;
/** The cell that ends the band: Black Background (EN 300 706 Table 26). */
const TAIL_CELLS = 1;
/** Padding each side when not given. */
const PADDING = 1;
/** The region its band is. */
const BAND = "band";

export class Button extends Control<ButtonProps> {
  /** The width a button with `label` and this padding fits: so several can be given the widest's. */
  static widthOf(label: string, paddingLeft = PADDING, paddingRight = PADDING): number {
    return LEAD_CELLS + paddingLeft + cells(label) + paddingRight + TAIL_CELLS;
  }

  get #width(): number {
    return this.props.width ?? Button.widthOf(this.props.label, this.props.paddingLeft, this.props.paddingRight);
  }

  override measure(space: Size): Size {
    return { width: Math.min(space.width, this.#width), height: Math.min(1, space.height) };
  }

  protected override keysHint(): string {
    return "Enter or a click presses; Tab moves on";
  }

  override paint(canvas: Canvas): void {
    const look = this.disabled ? DISABLED_BAND : this.focused ? CHOSEN : { background: this.props.background ?? Colour.Cyan, foreground: Colour.Blue };
    // Its own width, in a box as wide as a stack's row may be: the band ends there.
    const width = Math.min(canvas.width, this.#width);
    canvas.background(0, 0, look.background);
    if (width < canvas.width) {
      canvas.code(0, width - TAIL_CELLS, Attribute.BlackBackground);
      if (!this.disabled) {
        canvas.control(0, NEW_BACKGROUND_CELL, width - TAIL_CELLS - NEW_BACKGROUND_CELL, 1, BAND);
      }
    } else if (!this.disabled) {
      canvas.bandControl(0, 0, 1, BAND);
    }
    const room = Math.max(0, width - LEAD_CELLS - TAIL_CELLS);
    const label = fit(this.props.label, room);
    // A fixed width centres the label; otherwise the padding places it.
    const before = this.props.width === undefined ? (this.props.paddingLeft ?? PADDING) : Math.floor((room - cells(label)) / 2);
    canvas.coloured(0, LEAD_CELLS - 1, look.foreground, fit(`${" ".repeat(Math.max(0, before))}${label}`, room).padEnd(room));
  }

  #press(session: number): boolean {
    if (this.disabled) {
      return false;
    }
    this.activate(this.props.label, session);
    this.props.onPress(session);
    return true;
  }

  override onClick(event: PointerEvent): boolean {
    return event.region === BAND && this.#press(event.session);
  }

  override onKey(event: KeyEvent): boolean {
    return choosing(event) && this.#press(event.session);
  }
}
