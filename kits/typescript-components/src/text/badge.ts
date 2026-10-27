// Badges: short labels on a background of their own colour, in a line of
// text ("LIVE", "3 NEW", "BETA"). Each is a chip (core/canvas.ts): its
// background starts a cell before the text and ends after it.

import { CHIP_CELLS, type Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { cells, fit } from "../core/text.ts";
import { Attribute, Colour } from "../kit.ts";

export interface BadgeSpec {
  readonly text: string;
  readonly background?: Colour;
  readonly foreground?: Colour;
  /** Flashing, as LIVE and NEWSFLASH are. */
  readonly flash?: boolean;
}

export interface BadgesProps {
  readonly badges: readonly BadgeSpec[];
  /** Text after the badges, in white. */
  readonly text?: string;
  readonly textForeground?: Colour;
}

/** One or more badges in a row, then text. */
export class Badges extends Component<BadgesProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    let column = 0;
    for (const badge of this.props.badges) {
      const room = canvas.width - column - CHIP_CELLS - (badge.flash === true ? 2 : 0);
      if (room <= 0) {
        return;
      }
      const shown = fit(badge.text, room);
      if (badge.flash === true) {
        // Flash and Steady each take a cell: one before the chip, one where its Black Background was.
        canvas.code(0, column, Attribute.Flash);
        column = canvas.chip(0, column + 1, badge.background ?? Colour.Red, badge.foreground ?? Colour.White, shown);
        canvas.code(0, column, Attribute.Steady);
        column += 1;
      } else {
        column = canvas.chip(0, column, badge.background ?? Colour.Red, badge.foreground ?? Colour.White, shown);
      }
    }
    if (this.props.text !== undefined && column < canvas.width - 1) {
      canvas.coloured(0, column, this.props.textForeground ?? Colour.White, fit(this.props.text, canvas.width - column - 1));
    }
  }

  /** The cells `badges` take, for laying out beside them. */
  static widthOf(badges: readonly BadgeSpec[]): number {
    return badges.reduce((sum, badge) => sum + cells(badge.text) + CHIP_CELLS + (badge.flash === true ? 2 : 0), 0);
  }
}
