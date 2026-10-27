// A panel: its children inside a frame, with a title. Teletext has no box
// drawing characters, so the frame is drawn either as a line of mosaic
// sixels round the children or as a background of its own colour behind
// them. Either way each row inside spends cells on attributes (Table 26):
// a mosaic colour and the edge on the left, a mosaic colour and the edge on
// the right, or a background's colour and New Background.

import type { Canvas } from "../core/canvas.ts";
import { type Box, Component, type LayoutChild, type Size, stack, stackSize } from "../core/component.ts";
import { MOSAIC_BASE, SIXEL } from "../core/mosaics.ts";
import { cells, fit } from "../core/text.ts";
import { Colour, mosaicColour } from "../kit.ts";

/** A line round the children, or a background behind them. */
export type PanelStyle = "line" | "band";

export interface PanelProps {
  readonly title?: string;
  readonly style?: PanelStyle;
  /** The line's colour, or the background's. */
  readonly foreground?: Colour;
  /** The title's colour. */
  readonly titleForeground?: Colour;
}

/** The line's characters: a frame whose top runs along the bottom of its first row and whose sides run down the inner halves of its edge cells. */
const LINE = {
  top: MOSAIC_BASE | SIXEL.bottomLeft | SIXEL.bottomRight,
  bottom: MOSAIC_BASE | SIXEL.topLeft | SIXEL.topRight,
  left: MOSAIC_BASE | SIXEL.topRight | SIXEL.middleRight | SIXEL.bottomRight,
  right: MOSAIC_BASE | SIXEL.topLeft | SIXEL.middleLeft | SIXEL.bottomLeft,
  topLeft: MOSAIC_BASE | SIXEL.bottomRight,
  topRight: MOSAIC_BASE | SIXEL.bottomLeft,
  bottomLeft: MOSAIC_BASE | SIXEL.topRight,
  bottomRight: MOSAIC_BASE | SIXEL.topLeft,
} as const;

/** The cells a line frame takes on the left of each row (the mosaic colour, the edge) and on the right (the same). */
const LINE_SIDE = 2;
/** The cells a band takes on the left of each row (its colour, New Background) and on the right (Black Background). */
const BAND_LEFT = 2;
const BAND_RIGHT = 1;
/** The rows a line frame takes above and below the children. */
const LINE_ROWS = 2;
/** The row a band's title takes. */
const BAND_TITLE_ROWS = 1;

export class Panel extends Component<PanelProps> {
  get #style(): PanelStyle {
    return this.props.style ?? "line";
  }

  /** The children's box inside `box`. */
  #inner(box: Box): Box {
    if (this.#style === "line") {
      return { row: box.row + 1, column: box.column + LINE_SIDE, width: Math.max(0, box.width - 2 * LINE_SIDE), height: Math.max(0, box.height - LINE_ROWS) };
    }
    const top = this.props.title === undefined ? 0 : BAND_TITLE_ROWS;
    return { row: box.row + top, column: box.column + BAND_LEFT, width: Math.max(0, box.width - BAND_LEFT - BAND_RIGHT), height: Math.max(0, box.height - top) };
  }

  override measure(space: Size, children: readonly LayoutChild[]): Size {
    const inner = this.#inner({ row: 0, column: 0, ...space });
    const frame = space.height - inner.height;
    return { width: space.width, height: Math.min(space.height, stackSize(inner, children).height + frame) };
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    stack(this.#inner(box), children);
  }

  override paint(canvas: Canvas): void {
    const foreground = this.props.foreground ?? Colour.Blue;
    const { width, height } = canvas;
    if (this.#style === "band") {
      for (let row = 0; row < height; row += 1) {
        canvas.background(row, 0, foreground);
      }
      if (this.props.title !== undefined) {
        canvas.coloured(0, BAND_LEFT, this.props.titleForeground ?? Colour.White, fit(this.props.title, width - BAND_LEFT - BAND_RIGHT - 1));
      }
      return;
    }
    const across = Math.max(0, width - 3);
    canvas.mosaics(0, 0, foreground, [LINE.topLeft, ...new Array<number>(across).fill(LINE.top), LINE.topRight]);
    if (this.props.title !== undefined) {
      // The title sits in the top line: its colour's cell and the mosaic colour's after it are gaps either side of it.
      const title = fit(this.props.title, Math.max(0, width - 8));
      canvas.coloured(0, 2, this.props.titleForeground ?? Colour.Yellow, title);
      canvas.code(0, 3 + cells(title), mosaicColour(foreground));
    }
    for (let row = 1; row < height - 1; row += 1) {
      canvas.mosaics(row, 0, foreground, [LINE.left]);
      canvas.mosaics(row, width - LINE_SIDE, foreground, [LINE.right]);
    }
    if (height >= LINE_ROWS) {
      canvas.mosaics(height - 1, 0, foreground, [LINE.bottomLeft, ...new Array<number>(across).fill(LINE.bottom), LINE.bottomRight]);
    }
  }
}
