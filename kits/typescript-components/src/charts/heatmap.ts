// A heat map: rows of values, each a block whose colour, or whose density
// of sixels, says how high it is. A block changes colour in a cell of its
// own; with Hold Mosaics that cell shows the last block again (EN 300 706
// clause 12.2), so each value is two cells wide with no gap. A density map
// needs no colour changes, so each value is one cell, and it reads without
// colour.

import type { Canvas } from "../core/canvas.ts";
import { paintEmpty } from "../core/look.ts";
import { Component, type Size } from "../core/component.ts";
import { FULL_BLOCK, MOSAIC_BASE, SIXEL } from "../core/mosaics.ts";
import { fit } from "../core/text.ts";
import { Attribute, Colour, mosaicColour } from "../kit.ts";

export interface HeatRow {
  readonly label: string;
  readonly values: readonly number[];
}

export interface HeatmapProps {
  readonly rows: readonly HeatRow[];
  /** By colour, or by how many sixels are set. */
  readonly scale?: "colour" | "density";
  readonly labelCells?: number;
  /** The highest value; the most in the rows when absent. */
  readonly max?: number;
  /** A row under it saying what the colours or densities mean. */
  readonly legend?: boolean;
  /** The density scale's colour. */
  readonly foreground?: Colour;
  readonly empty?: string;
}

/** Cold to hot. */
const COLOURS = [Colour.Blue, Colour.Cyan, Colour.Green, Colour.Yellow, Colour.Red, Colour.Magenta, Colour.White] as const;
/** None to all six sixels, spread so each step looks denser. */
const DENSITIES = [
  MOSAIC_BASE,
  MOSAIC_BASE | SIXEL.middleLeft,
  MOSAIC_BASE | SIXEL.topLeft | SIXEL.bottomRight,
  MOSAIC_BASE | SIXEL.topLeft | SIXEL.middleRight | SIXEL.bottomLeft,
  MOSAIC_BASE | SIXEL.topLeft | SIXEL.topRight | SIXEL.bottomLeft | SIXEL.bottomRight,
  MOSAIC_BASE | SIXEL.topLeft | SIXEL.topRight | SIXEL.middleLeft | SIXEL.bottomLeft | SIXEL.bottomRight,
  FULL_BLOCK,
] as const;
const LABEL_CELLS = 6;
/** The cells a value takes on a colour scale: its colour's and its block. */
const COLOUR_CELLS = 2;

function level(value: number, max: number, levels: number): number {
  return max <= 0 ? 0 : Math.max(0, Math.min(levels - 1, Math.round((value / max) * (levels - 1))));
}

export class Heatmap extends Component<HeatmapProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, Math.max(1, this.props.rows.length + (this.props.legend === true ? 1 : 0))) };
  }

  #codes(values: readonly number[], max: number, cells: number): number[] {
    if (this.props.scale === "density") {
      return [mosaicColour(this.props.foreground ?? Colour.Yellow), ...values.slice(0, cells - 1).map((value) => DENSITIES[level(value, max, DENSITIES.length)] ?? MOSAIC_BASE)];
    }
    const shown = values.slice(0, Math.floor((cells - 2) / COLOUR_CELLS));
    // Hold Mosaics first; then each value's colour and block; a last block so the last value is two cells too.
    const codes = [Attribute.HoldMosaics, ...shown.flatMap((value) => [mosaicColour(COLOURS[level(value, max, COLOURS.length)] ?? Colour.Blue), FULL_BLOCK])];
    return shown.length === 0 ? codes : [...codes, FULL_BLOCK, Attribute.ReleaseMosaics];
  }

  override paint(canvas: Canvas): void {
    if (this.props.rows.length === 0) {
      paintEmpty(canvas, this.props.empty ?? "no data");
      return;
    }
    const labelCells = this.props.labelCells ?? LABEL_CELLS;
    const max = this.props.max ?? Math.max(0, ...this.props.rows.flatMap((row) => row.values));
    const cells = canvas.width - labelCells;
    this.props.rows.forEach((row, index) => {
      canvas.coloured(index, 0, Colour.White, fit(row.label, labelCells - 1));
      canvas.codes(index, labelCells, this.#codes(row.values, max, cells));
    });
    if (this.props.legend === true) {
      const row = this.props.rows.length;
      const steps = this.props.scale === "density" ? DENSITIES.length : COLOURS.length;
      const scale = Array.from({ length: steps }, (_, step) => (step * max) / Math.max(1, steps - 1));
      canvas.coloured(row, 0, Colour.White, "Low");
      canvas.codes(row, labelCells, this.#codes(scale, max, cells));
      const after = labelCells + this.#codes(scale, max, cells).length;
      canvas.coloured(row, after, Colour.White, "High");
    }
  }
}
