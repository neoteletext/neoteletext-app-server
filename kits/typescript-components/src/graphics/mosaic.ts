// Bitmaps of sixels, written as rows of text, drawn in mosaic graphics.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { Colour, Sixels, SIXELS_ACROSS, SIXELS_DOWN } from "../kit.ts";

/** A bitmap as rows of text, "#" for a set sixel. */
export type Bitmap = readonly string[];

/** A bitmap's sixels. */
export function sixelsOf(bitmap: Bitmap): Sixels {
  const width = Math.max(0, ...bitmap.map((line) => line.length));
  const sixels = new Sixels(width, bitmap.length);
  bitmap.forEach((line, y) => [...line].forEach((sixel, x) => sixels.set(x, y, sixel === "#")));
  return sixels;
}

export interface MosaicProps {
  readonly bitmap: Bitmap;
  readonly foreground?: Colour;
}

/** A bitmap drawn in mosaic graphics; its colour takes the cell before it. */
export class Mosaic extends Component<MosaicProps> {
  override measure(space: Size): Size {
    const sixels = sixelsOf(this.props.bitmap);
    return {
      width: Math.min(space.width, Math.ceil(sixels.width / SIXELS_ACROSS) + 1),
      height: Math.min(space.height, Math.ceil(sixels.height / SIXELS_DOWN)),
    };
  }

  override paint(canvas: Canvas): void {
    canvas.sixels(0, 1, this.props.foreground ?? Colour.White, sixelsOf(this.props.bitmap));
  }
}
