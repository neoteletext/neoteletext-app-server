// The layouts every page is built from: a stack of rows, columns side by
// side, a place given by row and column, a stack centred across, layers
// drawn over one another, and a gap.

import { type Box, Component, type LayoutChild, type Size, stack, stackSize } from "../core/component.ts";

/** Children top to bottom (the default layout, named). */
export class Rows extends Component {}

export interface ColumnsProps {
  /** The columns' widths; one without a width shares what is left. */
  readonly widths?: readonly (number | null)[];
}

/** Children side by side, each as tall as the tallest. Each column's first cell is its own: its colour attribute (canvas.ts). */
export class Columns extends Component<ColumnsProps> {
  #widths(width: number, children: readonly LayoutChild[]): number[] {
    const given = children.map((child, index) => this.props.widths?.[index] ?? child.props.width ?? null);
    const fixed = given.reduce<number>((sum, value) => sum + (value ?? 0), 0);
    const free = given.filter((value) => value === null).length;
    const share = free > 0 ? Math.floor(Math.max(0, width - fixed) / free) : 0;
    return given.map((value) => value ?? share);
  }

  override measure(space: Size, children: readonly LayoutChild[]): Size {
    const widths = this.#widths(space.width, children);
    const height = Math.max(0, ...children.map((child, index) => child.measure({ width: widths[index] ?? 0, height: space.height }).height));
    return { width: space.width, height: Math.min(height, space.height) };
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    const widths = this.#widths(box.width, children);
    let column = box.column;
    children.forEach((child, index) => {
      const width = Math.min(widths[index] ?? 0, box.column + box.width - column);
      child.place({ row: box.row, column, width: Math.max(0, width), height: box.height });
      column += Math.max(0, width);
    });
  }
}

export interface AtProps {
  readonly row: number;
  readonly column: number;
  readonly rows?: number;
  readonly columns?: number;
}

/** Its children at a fixed place inside its parent's box, for what cannot flow: an emblem beside a title, a wheel in the middle. */
export class At extends Component<AtProps> {
  /** As tall as its place's row and what is placed there. */
  override measure(space: Size, children: readonly LayoutChild[]): Size {
    const inner = { width: this.props.columns ?? Math.max(0, space.width - this.props.column), height: this.props.rows ?? Math.max(0, space.height - this.props.row) };
    const content = this.props.rows ?? stackSize(inner, children).height;
    return { width: space.width, height: Math.min(space.height, this.props.row + content) };
  }

  /** Only what it places is reached: laid over another layer, it would otherwise take every click on the cells round what it places. */
  override pointerCells(): null {
    return null;
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    const inner: Box = {
      row: box.row + this.props.row,
      column: box.column + this.props.column,
      width: this.props.columns ?? Math.max(0, box.width - this.props.column),
      height: this.props.rows ?? Math.max(0, box.height - this.props.row),
    };
    stack(inner, children);
  }
}

/** Children top to bottom, each as wide as it measures and in the middle of the width: a stack of buttons, a title. */
export class Centred extends Component {
  override measure(space: Size, children: readonly LayoutChild[]): Size {
    return { width: space.width, height: Math.min(space.height, children.reduce((rows, child) => rows + child.measure(space).height, 0)) };
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    let row = box.row;
    for (const child of children) {
      const size = child.measure({ width: box.width, height: Math.max(0, box.row + box.height - row) });
      child.place({ row, column: box.column + Math.floor((box.width - size.width) / 2), width: size.width, height: size.height });
      row += size.height;
    }
  }
}

/** Layered children: each in the whole box, later ones drawn over earlier ones; as tall as the tallest. */
export class Layers extends Component {
  override measure(space: Size, children: readonly LayoutChild[]): Size {
    return { width: space.width, height: Math.min(space.height, Math.max(0, ...children.map((child) => child.measure(space).height))) };
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    for (const child of children) {
      child.place(box);
    }
  }
}

export interface GapProps {
  readonly rows?: number;
}

/** Empty rows; with `grow`, what is left over. */
export class Gap extends Component<GapProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.props.rows ?? 1, space.height) };
  }
}
