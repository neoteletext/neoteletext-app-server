// A picture or a QR code made by the engine's tools, as TTI rows placed in
// a box.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { readTti } from "../kit.ts";

export interface PictureProps {
  /** A page in TTI, such as the engine's converter or `neottx qr` writes. */
  readonly tti: string;
  /** The first of its rows to show. */
  readonly firstRow?: number;
}

/** Rows of a TTI page drawn into the box, from `firstRow`: a converted picture, a QR code. */
export class Picture extends Component<PictureProps> {
  #rows(): readonly (readonly number[])[] {
    // A tool that ends its lines CR LF (the host's neottx does) would leave each CR in the row as code 0x0D,
    // Double Height, which hides the row below across the whole page.
    return readTti(this.props.tti.replace(/\r/gu, ""))[0]?.rows ?? [];
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, Math.max(0, this.#rows().length - (this.props.firstRow ?? 0))) };
  }

  override paint(canvas: Canvas): void {
    const first = this.props.firstRow ?? 0;
    this.#rows()
      .slice(first)
      .forEach((codes, row) => canvas.codes(row, 0, codes));
  }
}
