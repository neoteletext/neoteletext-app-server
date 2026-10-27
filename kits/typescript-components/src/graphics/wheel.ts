// A roulette wheel: a ring of coloured blocks, one lit.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { FULL_BLOCK } from "../core/mosaics.ts";
import { Colour, mosaicColour } from "../kit.ts";

export interface WheelProps {
  /** The segment lit, which turns as the wheel spins. */
  readonly lit: number;
  readonly segments?: number;
}

/** A roulette wheel's colours round it. */
const WHEEL_COLOURS = [Colour.Red, Colour.Yellow, Colour.Green, Colour.Cyan, Colour.Blue, Colour.Magenta] as const;

/** The wheel's segments. */
const WHEEL_SEGMENTS = 12;

/** Its radius in rows and in columns: a cell is about 12 by 20 pixels, so 8 columns across is about 5 rows down, and it is round. */
const WHEEL_ROWS = 5;

const WHEEL_COLUMNS = 8;

/** A ring of coloured blocks, one lit white. */
export class Wheel extends Component<WheelProps> {
  override measure(space: Size): Size {
    return { width: Math.min(space.width, 2 * WHEEL_COLUMNS + 4), height: Math.min(space.height, 2 * WHEEL_ROWS + 1) };
  }

  override paint(canvas: Canvas): void {
    const segments = this.props.segments ?? WHEEL_SEGMENTS;
    for (let segment = 0; segment < segments; segment += 1) {
      const angle = (2 * Math.PI * segment) / segments;
      const row = WHEEL_ROWS - Math.round(WHEEL_ROWS * Math.cos(angle));
      const column = WHEEL_COLUMNS + 2 + Math.round(WHEEL_COLUMNS * Math.sin(angle));
      const lit = ((this.props.lit % segments) + segments) % segments === segment;
      const foreground = lit ? Colour.White : (WHEEL_COLOURS[segment % WHEEL_COLOURS.length] ?? Colour.White);
      // Two cells a segment, so a narrow cell does not look like a slit.
      canvas.codes(row, column - 1, [mosaicColour(foreground), FULL_BLOCK, FULL_BLOCK]);
    }
  }
}
