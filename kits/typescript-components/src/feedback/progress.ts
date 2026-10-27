// How far along something is, or how full: a progress bar of mosaic blocks
// to the half cell, solid or in separated graphics (EN 300 706 clause
// 12.2), whose colour may follow thresholds as a gauge's does; and a line
// gauge, a thin line whose filled part is in one colour and the rest in
// another. A line changes colour in a cell of its own; with Hold Mosaics
// that cell shows the last mosaic again (clause 12.2, Table 26), so the line
// has no gap.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { FULL_BLOCK, LEFT_HALF, MIDDLE_ROW, MOSAIC_BASE, SIXEL } from "../core/mosaics.ts";
import { fit, padStart } from "../core/text.ts";
import { Attribute, Colour, mosaicColour } from "../kit.ts";

/** A colour from a share upward, as a gauge goes green, yellow, red. */
export type Threshold = readonly [from: number, foreground: Colour];

export interface ProgressProps {
  readonly label?: string;
  readonly value: number;
  readonly max: number;
  readonly foreground?: Colour;
  /** Separated graphics: the bar in blocks with gaps, as segments. */
  readonly separated?: boolean;
  /** Colours by share, the highest whose `from` the share reaches. */
  readonly thresholds?: readonly Threshold[];
  /** The label's column. */
  readonly labelCells?: number;
  /** The share after the bar. */
  readonly percent?: boolean;
}

/** The label's column when there is a label and no width is given. */
const LABEL_CELLS = 12;
/** The share's cells after the bar: its colour and "100%". */
const PERCENT_CELLS = 5;
/** The empty part of the track: its bottom sixels, a floor to fill along. */
const TRACK = MOSAIC_BASE | SIXEL.bottomLeft | SIXEL.bottomRight;
/** Thresholds a gauge has when it is given only a colour: green, then yellow past 60%, red past 85%. */
export const GAUGE: readonly Threshold[] = [
  [0, Colour.Green],
  [0.6, Colour.Yellow],
  [0.85, Colour.Red],
];

function share(value: number, max: number): number {
  return max <= 0 ? 0 : Math.max(0, Math.min(1, value / max));
}

function colourFor(fraction: number, given: Colour, thresholds: readonly Threshold[] | undefined): Colour {
  if (thresholds === undefined) {
    return given;
  }
  return [...thresholds].reverse().find(([from]) => fraction >= from)?.[1] ?? given;
}

export class ProgressBar extends Component<ProgressProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const fraction = share(this.props.value, this.props.max);
    const labelCells = this.props.label === undefined ? 0 : (this.props.labelCells ?? LABEL_CELLS);
    if (this.props.label !== undefined) {
      canvas.coloured(0, 0, Colour.White, fit(this.props.label, labelCells - 1));
    }
    const lead = this.props.separated === true ? [Attribute.Separated] : [];
    const after = this.props.percent === false ? 0 : PERCENT_CELLS;
    const cells = Math.max(1, canvas.width - labelCells - 1 - lead.length - after);
    const halves = Math.round(2 * cells * fraction);
    const codes = Array.from({ length: cells }, (_, cell) => (2 * cell + 2 <= halves ? FULL_BLOCK : 2 * cell + 1 === halves ? LEFT_HALF | TRACK : TRACK));
    const foreground = colourFor(fraction, this.props.foreground ?? Colour.Green, this.props.thresholds);
    canvas.codes(0, labelCells, [mosaicColour(foreground), ...lead, ...codes]);
    if (this.props.percent !== false) {
      canvas.coloured(0, labelCells + 1 + lead.length + cells, Colour.White, padStart(`${Math.round(fraction * 100)}%`, PERCENT_CELLS - 1));
    }
  }
}

export interface LineGaugeProps {
  readonly label?: string;
  readonly value: number;
  readonly max: number;
  readonly foreground?: Colour;
  /** The rest of the line's colour. */
  readonly rest?: Colour;
  readonly labelCells?: number;
}

export class LineGauge extends Component<LineGaugeProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const fraction = share(this.props.value, this.props.max);
    const labelCells = this.props.label === undefined ? 0 : (this.props.labelCells ?? LABEL_CELLS);
    if (this.props.label !== undefined) {
      canvas.coloured(0, 0, Colour.White, fit(this.props.label, labelCells - 1));
    }
    // Hold Mosaics, the filled colour, the filled line, the rest's colour (showing the held line), the rest.
    const cells = Math.max(2, canvas.width - labelCells - 3 - PERCENT_CELLS);
    const filled = Math.round(cells * fraction);
    const codes = [
      Attribute.HoldMosaics,
      mosaicColour(this.props.foreground ?? Colour.Cyan),
      ...new Array<number>(filled).fill(MIDDLE_ROW),
      mosaicColour(this.props.rest ?? Colour.Blue),
      ...new Array<number>(Math.max(0, cells - filled)).fill(MIDDLE_ROW),
      Attribute.ReleaseMosaics,
    ];
    canvas.codes(0, labelCells, codes);
    canvas.coloured(0, labelCells + codes.length, Colour.White, padStart(`${Math.round(fraction * 100)}%`, PERCENT_CELLS - 1));
  }
}
