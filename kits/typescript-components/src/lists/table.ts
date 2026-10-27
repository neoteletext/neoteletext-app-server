// A table of labels and their values, in two columns.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { paintEmpty } from "../core/look.ts";
import { fit } from "../core/text.ts";
import { Colour } from "../kit.ts";

export interface TableProps {
  readonly rows: readonly (readonly [label: string, value: string])[];
  /** The labels' column; half the box when absent. */
  readonly labelCells?: number;
  readonly labelForeground?: Colour;
  readonly valueForeground?: Colour;
  readonly empty?: string;
}

export class Table extends Component<TableProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(Math.max(1, this.props.rows.length), space.height) };
  }

  override paint(canvas: Canvas): void {
    if (this.props.rows.length === 0) {
      paintEmpty(canvas, this.props.empty ?? "nothing to show");
      return;
    }
    const at = this.props.labelCells ?? Math.floor(canvas.width / 2);
    this.props.rows.forEach(([label, value], row) => {
      canvas.coloured(row, 0, this.props.labelForeground ?? Colour.White, fit(label, at - 1));
      canvas.coloured(row, at, this.props.valueForeground ?? Colour.Cyan, fit(value, canvas.width - at - 1));
    });
  }
}
