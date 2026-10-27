// A grid: children left to right, then down, a cell each.

import { type Box, Component, type LayoutChild, type Size } from "../core/component.ts";

export interface GridProps {
  readonly columns: number;
}

/** Its children in a grid, left to right then down, each cell a row high and the box's width over the columns. */
export class Grid extends Component<GridProps> {
  override measure(space: Size, children: readonly LayoutChild[]): Size {
    return { width: space.width, height: Math.min(Math.ceil(children.length / Math.max(1, this.props.columns)), space.height) };
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    const columns = Math.max(1, this.props.columns);
    const width = Math.floor(box.width / columns);
    children.forEach((child, index) => {
      const row = Math.floor(index / columns);
      if (row < box.height) {
        child.place({ row: box.row + row, column: box.column + (index % columns) * width, width, height: 1 });
      }
    });
  }
}
