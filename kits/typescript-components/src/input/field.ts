// A form's field: its label on the left and its control on the right. A
// group of fields goes in a Panel (layout/panel.ts) with the group's name;
// Tab goes from control to control in the order they are drawn
// (core/mount.ts).

import type { Canvas } from "../core/canvas.ts";
import { type Box, Component, type LayoutChild, type Size, stack, stackSize } from "../core/component.ts";
import { fit } from "../core/text.ts";
import { Colour } from "../kit.ts";

export interface FieldProps {
  readonly label: string;
  /** The cells the label's column takes. */
  readonly labelCells?: number;
  /** A word under the label: what is wanted, or that it must be given. */
  readonly note?: string;
}

/** The label's column when none is given. */
const LABEL_CELLS = 12;

/** A label and, beside it, its control: the children. */
export class Field extends Component<FieldProps> {
  get #labelCells(): number {
    return this.props.labelCells ?? LABEL_CELLS;
  }

  #inner(box: Box): Box {
    return { row: box.row, column: box.column + this.#labelCells, width: Math.max(0, box.width - this.#labelCells), height: box.height };
  }

  override measure(space: Size, children: readonly LayoutChild[]): Size {
    const inner = stackSize({ width: Math.max(0, space.width - this.#labelCells), height: space.height }, children);
    return { width: space.width, height: Math.min(space.height, Math.max(this.props.note === undefined ? 1 : 2, inner.height)) };
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    stack(this.#inner(box), children);
  }

  override paint(canvas: Canvas): void {
    canvas.coloured(0, 0, Colour.White, fit(this.props.label, this.#labelCells - 1));
    if (this.props.note !== undefined) {
      canvas.coloured(1, 0, Colour.Blue, fit(this.props.note, this.#labelCells - 1));
    }
  }
}
