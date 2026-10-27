// Check boxes. Teletext has no box character, so a box is a chip (core/
// canvas.ts): a white square of background holding "x" when ticked, "-"
// when mixed, nothing when not, so the state shows without colour. One on
// its own is pressed with Enter, Space or a click; a group goes Up and
// Down, or across in two columns, and each box is ticked as one is. A box
// and its text are one control, and a click ticks only there.

import { CHIP_CELLS, type Canvas, NEW_BACKGROUND_CELL } from "../core/canvas.ts";
import type { Size } from "../core/component.ts";
import { isPress, type KeyEvent, type PointerEvent } from "../core/events.ts";
import { choosing } from "../core/keys.ts";
import { CHOSEN, DISABLED, DISABLED_CHIP, LABEL } from "../core/look.ts";
import { cells, fit } from "../core/text.ts";
import { Colour, Key } from "../kit.ts";
import { type ValueProps, ValueControl, type ValueState } from "./control.ts";

export type Ticked = boolean | "mixed";

/** What the box holds for each state. */
const MARKS = { true: "x", false: " ", mixed: "-" } as const;

/** The region of a box on its own. */
const BOX = "box";

/**
 * Draws a box and its text from `column` in `width` cells: the text on the
 * band while `chosen`. A box that can be ticked is a control from its band
 * to the end of its text, and the region `id`.
 */
function drawBox(canvas: Canvas, row: number, column: number, ticked: Ticked, text: string, look: { readonly chosen: boolean; readonly disabled: boolean }, width: number, id: string): void {
  const after = canvas.chip(row, column, look.disabled ? DISABLED_CHIP.background : Colour.White, Colour.Black, MARKS[String(ticked) as keyof typeof MARKS]);
  const room = width - (after - column) - 1;
  let end: number;
  if (look.chosen && !look.disabled) {
    // Up to the label's Black Background.
    end = canvas.chip(row, after, CHOSEN.background, CHOSEN.foreground, fit(text, room - CHIP_CELLS)) - 1;
  } else {
    const shown = fit(text, room);
    canvas.coloured(row, after, look.disabled ? DISABLED : LABEL, shown);
    end = after + 1 + cells(shown);
  }
  if (!look.disabled) {
    canvas.control(row, column + NEW_BACKGROUND_CELL, end - column - NEW_BACKGROUND_CELL, 1, id);
  }
}

export interface CheckboxProps extends ValueProps<Ticked> {
  readonly label: string;
}

export class Checkbox extends ValueControl<Ticked, CheckboxProps> {
  protected empty(): Ticked {
    return false;
  }

  protected override keysHint(): string {
    return "Enter, Space or a click ticks";
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    drawBox(canvas, 0, 0, this.value, this.props.label, { chosen: this.focused, disabled: this.disabled }, canvas.width, BOX);
  }

  override onKey(event: KeyEvent): boolean {
    if (!choosing(event) || this.disabled) {
      return false;
    }
    this.change(this.value !== true, event.session);
    return true;
  }

  override onClick(event: PointerEvent): boolean {
    if (event.region === BOX) {
      this.change(this.value !== true, event.session);
    }
    return true;
  }
}

export interface CheckboxGroupProps extends ValueProps<readonly number[]> {
  readonly items: readonly string[];
  /** Items that cannot be changed. */
  readonly disabledItems?: readonly number[];
  /** Two columns, across then down. */
  readonly columns?: 1 | 2;
}

interface CheckboxGroupState extends ValueState<readonly number[]> {
  readonly chosen: number;
}

/** Several boxes, any of them ticked: a multiple choice. */
export class CheckboxGroup extends ValueControl<readonly number[], CheckboxGroupProps, CheckboxGroupState> {
  override state: CheckboxGroupState = { own: this.props.initial ?? [], chosen: 0 };

  protected empty(): readonly number[] {
    return [];
  }

  protected override keysHint(): string {
    return this.#columns > 1 ? "Arrows choose, Enter or Space ticks" : "Up Down choose, Enter or Space ticks";
  }

  get #columns(): number {
    return this.props.columns ?? 1;
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, Math.ceil(this.props.items.length / this.#columns)) };
  }

  override paint(canvas: Canvas): void {
    const across = Math.floor(canvas.width / this.#columns);
    this.props.items.forEach((item, index) => {
      const row = Math.floor(index / this.#columns);
      const column = (index % this.#columns) * across;
      const disabled = this.disabled || this.props.disabledItems?.includes(index) === true;
      drawBox(canvas, row, column, this.value.includes(index), item, { chosen: this.focused && index === this.state.chosen, disabled }, across, String(index));
    });
  }

  #toggle(index: number, session: number): void {
    if (this.props.disabledItems?.includes(index) === true || index < 0 || index >= this.props.items.length) {
      return;
    }
    this.setState({ chosen: index });
    this.change(this.value.includes(index) ? this.value.filter((item) => item !== index) : [...this.value, index].sort((a, b) => a - b), session);
  }

  override onKey(event: KeyEvent): boolean {
    if (this.disabled) {
      return false;
    }
    if (choosing(event)) {
      this.#toggle(this.state.chosen, event.session);
      return true;
    }
    const columns = this.#columns;
    const step = isPress(event, Key.Down) ? columns : isPress(event, Key.Up) ? -columns : isPress(event, Key.Right) && columns > 1 ? 1 : isPress(event, Key.Left) && columns > 1 ? -1 : 0;
    if (step === 0) {
      return false;
    }
    this.setState({ chosen: Math.max(0, Math.min(this.props.items.length - 1, this.state.chosen + step)) });
    this.play("move", event.session);
    return true;
  }

  override onClick(event: PointerEvent): boolean {
    if (event.region !== undefined && !this.disabled) {
      this.#toggle(Number(event.region), event.session);
    }
    return true;
  }
}
