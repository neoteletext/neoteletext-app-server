// A sparkline: a series in one or more rows of mosaic sixels, a value to
// each column of sixels (two to a cell), scaled from its least to its
// most; its label before it and its last value after.

import type { Canvas } from "../core/canvas.ts";
import { paintEmpty } from "../core/look.ts";
import { Component, type Size } from "../core/component.ts";
import { fit, padStart } from "../core/text.ts";
import { Colour, Sixels, SIXELS_ACROSS, SIXELS_DOWN } from "../kit.ts";

export interface SparklineProps {
  readonly values: readonly number[];
  readonly label?: string;
  readonly labelCells?: number;
  /** One row has three levels; each row more, three more. */
  readonly plotRows?: number;
  readonly empty?: string;
  readonly foreground?: Colour;
  /** The last value after it. */
  readonly last?: boolean;
}

const LABEL_CELLS = 8;
/** The last value's cells, its colour's included. */
const LAST_CELLS = 6;

export class Sparkline extends Component<SparklineProps> {
  get #rows(): number {
    return this.props.plotRows ?? 1;
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, this.#rows) };
  }

  override paint(canvas: Canvas): void {
    if (this.props.values.length === 0) {
      paintEmpty(canvas, this.props.empty ?? "no data");
      return;
    }
    const labelCells = this.props.label === undefined ? 0 : (this.props.labelCells ?? LABEL_CELLS);
    if (this.props.label !== undefined) {
      canvas.coloured(this.#rows - 1, 0, Colour.White, fit(this.props.label, labelCells - 1));
    }
    const lastCells = this.props.last === false ? 0 : LAST_CELLS;
    const cells = Math.max(1, canvas.width - labelCells - 1 - lastCells);
    const shown = this.props.values.slice(-cells * SIXELS_ACROSS);
    const height = this.#rows * SIXELS_DOWN;
    const least = Math.min(...shown);
    const span = Math.max(...shown) - least;
    const sixels = new Sixels(cells * SIXELS_ACROSS, height);
    shown.forEach((value, x) => {
      // Every value shows at least its bottom sixel, so a low one is not mistaken for none.
      const level = span <= 0 ? 1 : 1 + Math.round(((value - least) / span) * (height - 1));
      for (let y = height - level; y < height; y += 1) {
        sixels.set(x, y, true);
      }
    });
    canvas.sixels(0, labelCells + 1, this.props.foreground ?? Colour.Green, sixels);
    const last = shown[shown.length - 1];
    if (this.props.last !== false && last !== undefined) {
      canvas.coloured(this.#rows - 1, labelCells + 1 + cells, Colour.Cyan, padStart(String(last), LAST_CELLS - 1));
    }
  }
}
