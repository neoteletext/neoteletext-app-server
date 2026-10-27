// A select: one of a list, shown in a field between arrows. Shut, Left and
// Right turn through the items, as a receiver's menus do; Enter opens the
// list under the field, where Up and Down choose, Enter takes and Escape
// shuts. A click on an arrow turns, on the field opens or shuts, on an item
// takes; each is a control, and open, the arrow on the right is not drawn
// and is none. Teletext cannot draw over what is below, so the open list
// pushes it down.

import { CHIP_CELLS, type Canvas } from "../core/canvas.ts";
import type { Box, LayoutChild, Size } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import { isPress, type KeyEvent, type PointerEvent } from "../core/events.ts";
import { DISABLED, DISABLED_BAND } from "../core/look.ts";
import { listStep, moveChoice } from "../core/list-cursor.ts";
import { pad } from "../core/text.ts";
import { Colour, Key } from "../kit.ts";
import { ListRows } from "../lists/list-row.ts";
import { type ValueProps, ValueControl, type ValueState } from "./control.ts";

export interface SelectProps extends ValueProps<number> {
  readonly label: string;
  readonly items: readonly string[];
  /** The most items the open list shows. */
  readonly shown?: number;
}

interface SelectState extends ValueState<number> {
  readonly open: boolean;
  /** The item chosen in the open list. */
  readonly chosen: number;
}

/** The items the open list shows when not told. */
const SHOWN = 6;
/** The arrows either side of the field, each after its colour's cell. */
const ARROW_CELLS = 2;

export class Select extends ValueControl<number, SelectProps, SelectState> {
  override state: SelectState = { own: this.props.initial ?? 0, open: false, chosen: this.props.initial ?? 0 };

  protected empty(): number {
    return 0;
  }

  protected override keysHint(): string {
    return this.state.open ? "Up Down choose, Enter takes, Escape shuts" : "Left Right turn, Enter opens";
  }

  get #shown(): number {
    return Math.min(this.props.shown ?? SHOWN, this.props.items.length);
  }

  /** The field's text cells: what is left between the arrows. */
  #fieldCells(width: number): number {
    return Math.max(1, width - this.labelCells - 2 * ARROW_CELLS - CHIP_CELLS - 1);
  }

  /** The first item the open list shows: the chosen one kept in the middle where it can be. */
  get #top(): number {
    return Math.max(0, Math.min(this.state.chosen - Math.floor(this.#shown / 2), this.props.items.length - this.#shown));
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, 1 + (this.state.open ? this.#shown : 0)) };
  }

  override paint(canvas: Canvas): void {
    const field = this.#fieldCells(canvas.width);
    const start = this.paintLabel(canvas);
    const arrows = this.disabled ? DISABLED : Colour.Cyan;
    const look = this.disabled ? DISABLED_BAND : { background: Colour.Blue, foreground: Colour.White };
    canvas.coloured(0, start, arrows, "←");
    const after = canvas.chip(0, start + ARROW_CELLS, look.background, look.foreground, pad(this.props.items[this.value] ?? "", field));
    canvas.coloured(0, after, arrows, this.state.open ? " " : "→");
    if (!this.disabled && this.props.items.length > 0) {
      // Each arrow after its colour's cell, and the field's band.
      canvas.control(0, start + 1, ARROW_CELLS - 1, 1, "back");
      canvas.chipControl(0, start + ARROW_CELLS, after, "field");
      if (!this.state.open) {
        canvas.control(0, after + 1, ARROW_CELLS - 1, 1, "on");
      }
    }
  }

  /** Open, the list's rows under the field, from the label's column; the item it has now in cyan. */
  override render(): Child {
    if (!this.state.open) {
      return null;
    }
    return h(ListRows<string>, {
      items: this.props.items,
      top: this.#top,
      shown: this.#shown,
      chosen: this.state.chosen,
      focused: this.focused,
      indent: () => this.labelCells,
      foreground: (_item: string, index: number) => (index === this.value ? Colour.Cyan : Colour.White),
      disabled: () => this.disabled,
      onPick: (index: number, session: number) => this.#take(index, session),
    });
  }

  /** The open list goes under the field's row. */
  override arrange(box: Box, children: readonly LayoutChild[]): void {
    for (const child of children) {
      child.place({ ...box, row: box.row + 1, height: Math.max(0, box.height - 1) });
    }
  }

  #take(index: number, session: number): void {
    this.setState({ chosen: index, open: false });
    this.change(index, session);
    this.activate(index, session);
  }

  override onKey(event: KeyEvent): boolean {
    const count = this.props.items.length;
    if (this.disabled || count === 0) {
      return false;
    }
    if (this.state.open) {
      if (isPress(event, Key.Enter) || isPress(event, " ")) {
        this.#take(this.state.chosen, event.session);
        return true;
      }
      if (isPress(event, Key.Escape)) {
        this.setState({ open: false, chosen: this.value });
        this.cancel(event.session);
        return true;
      }
      const step = listStep(event, this.#shown);
      if (step === null) {
        return false;
      }
      this.setState({ chosen: moveChoice(this.state.chosen, step, count) });
      this.play("move", event.session);
      return true;
    }
    if (isPress(event, Key.Enter) || isPress(event, " ")) {
      this.setState({ open: true, chosen: this.value });
      return true;
    }
    const turn = isPress(event, Key.Right) ? 1 : isPress(event, Key.Left) ? -1 : 0;
    if (turn === 0) {
      return false;
    }
    this.change((this.value + turn + count) % count, event.session);
    return true;
  }

  override onClick(event: PointerEvent): boolean {
    const count = this.props.items.length;
    const region = event.region ?? "";
    if (region === "back") {
      this.change((this.value + count - 1) % count, event.session);
    } else if (region === "on") {
      this.change((this.value + 1) % count, event.session);
    } else if (region === "field") {
      this.setState({ open: !this.state.open, chosen: this.value });
    }
    return true;
  }
}
