// A status bar: a row on a background of its own, its segments each in
// their colour (a colour's cell is the gap between them), and text on the
// right, such as the time. An alarm flashes.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { cells, fit } from "../core/text.ts";
import { Attribute, Colour, textColour } from "../kit.ts";

export interface Segment {
  readonly text: string;
  readonly foreground?: Colour;
}

export interface StatusBarProps {
  readonly segments: readonly Segment[];
  /** Text on the right. */
  readonly right?: string;
  readonly background?: Colour;
  /** The bar flashes: something is wrong. */
  readonly alarm?: boolean;
}

/** The cells the background takes before the first segment: its colour and New Background. */
const LEAD = 2;
/** The space kept between segments, beyond each one's colour cell. */
const SEGMENT_GAP = 1;

export class StatusBar extends Component<StatusBarProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.background(0, 0, this.props.background ?? Colour.Blue);
    let column = LEAD;
    if (this.props.alarm === true) {
      canvas.code(0, column, Attribute.Flash);
      column += 1;
    }
    const right = this.props.right === undefined ? "" : this.props.right;
    const rightAt = canvas.width - cells(right) - 2;
    for (const segment of this.props.segments) {
      const room = rightAt - column - 2;
      if (room <= 0) {
        break;
      }
      const shown = fit(segment.text, room);
      canvas.codes(0, column, [textColour(segment.foreground ?? Colour.White)]);
      canvas.text(0, column + 1, shown);
      column += 1 + cells(shown) + SEGMENT_GAP;
    }
    if (right !== "") {
      canvas.coloured(0, rightAt, Colour.Yellow, right);
    }
  }
}
