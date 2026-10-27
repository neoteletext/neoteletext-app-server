// Charts on axes, drawn in mosaic sixels: a line chart of a series (its
// points joined, or the area under it filled), and a scatter plot of
// points. Every plot row is one colour, as a row of mosaics is after its
// colour's cell; the axes are text before and under the plot.

import type { Canvas } from "../core/canvas.ts";
import { paintEmpty } from "../core/look.ts";
import { Component, type Size } from "../core/component.ts";
import { fit, padStart } from "../core/text.ts";
import { Colour, Sixels, SIXELS_ACROSS, SIXELS_DOWN } from "../kit.ts";

/** The cells the values' axis takes on the left: a figure and its colour's cell. */
const AXIS_CELLS = 5;
/** The labels' row under the plot. */
const LABEL_ROWS = 1;

/** A span to divide by: a series that does not vary is drawn level. */
function span(low: number, high: number): number {
  return high > low ? high - low : 1;
}

/** Draws a line of sixels from one point to another (Bresenham's). */
function line(sixels: Sixels, from: readonly [number, number], to: readonly [number, number]): void {
  let [x, y] = from;
  const [x1, y1] = to;
  const dx = Math.abs(x1 - x);
  const dy = -Math.abs(y1 - y);
  const sx = x < x1 ? 1 : -1;
  const sy = y < y1 ? 1 : -1;
  let error = dx + dy;
  for (;;) {
    sixels.set(x, y, true);
    if (x === x1 && y === y1) {
      return;
    }
    const twice = 2 * error;
    if (twice >= dy) {
      error += dy;
      x += sx;
    }
    if (twice <= dx) {
      error += dx;
      y += sy;
    }
  }
}

interface Plot {
  readonly sixels: Sixels;
  readonly cells: number;
}

/** A plot of `rows` rows filling the rest of `width` after the axis. */
function plot(width: number, rows: number): Plot {
  const cells = Math.max(1, width - AXIS_CELLS - 1);
  return { sixels: new Sixels(cells * SIXELS_ACROSS, rows * SIXELS_DOWN), cells };
}

/** The values' axis, the most at the top and the least at the bottom, and the labels under the plot. */
function axes(canvas: Canvas, rows: number, least: number, most: number, labels: readonly string[] | undefined, cells: number): void {
  canvas.coloured(0, 0, Colour.Cyan, padStart(String(most), AXIS_CELLS - 1));
  canvas.coloured(rows - 1, 0, Colour.Cyan, padStart(String(least), AXIS_CELLS - 1));
  if (labels !== undefined && labels.length > 0) {
    const pitch = Math.max(1, Math.floor(cells / labels.length));
    labels.forEach((label, index) => canvas.coloured(rows, AXIS_CELLS + index * pitch, Colour.White, fit(label, pitch - 1)));
  }
}

export interface LineChartProps {
  readonly values: readonly number[];
  readonly plotRows: number;
  readonly empty?: string;
  readonly foreground?: Colour;
  /** The area under the line filled. */
  readonly area?: boolean;
  /** Labels spread along the bottom. */
  readonly labels?: readonly string[];
}

export class LineChart extends Component<LineChartProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, this.props.plotRows + LABEL_ROWS) };
  }

  override paint(canvas: Canvas): void {
    if (this.props.values.length === 0) {
      paintEmpty(canvas, this.props.empty ?? "no data");
      return;
    }
    const { sixels, cells } = plot(canvas.width, this.props.plotRows);
    const values = this.props.values;
    const least = Math.min(...values);
    const most = Math.max(...values);
    const range = span(least, most);
    const point = (index: number): [number, number] => {
      const x = values.length <= 1 ? 0 : Math.round((index * (sixels.width - 1)) / (values.length - 1));
      const y = Math.round(((most - (values[index] ?? least)) / range) * (sixels.height - 1));
      return [x, y];
    };
    for (let index = 0; index < values.length; index += 1) {
      const here = point(index);
      if (index > 0) {
        line(sixels, point(index - 1), here);
      }
    }
    if (this.props.area === true) {
      for (let x = 0; x < sixels.width; x += 1) {
        let found = false;
        for (let y = 0; y < sixels.height; y += 1) {
          found = found || sixels.get(x, y);
          if (found) {
            sixels.set(x, y, true);
          }
        }
      }
    }
    canvas.sixels(0, AXIS_CELLS + 1, this.props.foreground ?? Colour.Green, sixels);
    axes(canvas, this.props.plotRows, least, most, this.props.labels, cells);
  }
}

export interface ScatterPlotProps {
  readonly points: readonly (readonly [x: number, y: number])[];
  readonly plotRows: number;
  readonly empty?: string;
  readonly foreground?: Colour;
  readonly labels?: readonly string[];
}

/** A point is a block of four sixels, so a single one stands out. */
const POINT_SIXELS = 2;

export class ScatterPlot extends Component<ScatterPlotProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, this.props.plotRows + LABEL_ROWS) };
  }

  override paint(canvas: Canvas): void {
    if (this.props.points.length === 0) {
      paintEmpty(canvas, this.props.empty ?? "no data");
      return;
    }
    const { sixels, cells } = plot(canvas.width, this.props.plotRows);
    const xs = this.props.points.map(([x]) => x);
    const ys = this.props.points.map(([, y]) => y);
    const [left, right, least, most] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    for (const [x, y] of this.props.points) {
      const across = Math.round(((x - left) / span(left, right)) * (sixels.width - POINT_SIXELS));
      const down = Math.round(((most - y) / span(least, most)) * (sixels.height - POINT_SIXELS));
      for (let dy = 0; dy < POINT_SIXELS; dy += 1) {
        for (let dx = 0; dx < POINT_SIXELS; dx += 1) {
          sixels.set(across + dx, down + dy, true);
        }
      }
    }
    canvas.sixels(0, AXIS_CELLS + 1, this.props.foreground ?? Colour.Yellow, sixels);
    axes(canvas, this.props.plotRows, least, most, this.props.labels, cells);
  }
}
