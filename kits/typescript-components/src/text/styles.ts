// Text in teletext's styles: a line, wrapped paragraphs, the display
// attributes (double height and width, flashing, concealed) and a band on a
// background of its own.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { cells, fit, headline, wrap } from "../core/text.ts";
import { Attribute, Colour } from "../kit.ts";

export interface TextProps {
  readonly text: string;
  readonly foreground?: Colour;
  /** Centred in its box. */
  readonly centre?: boolean;
}

/** One line of text in a colour: the colour takes the first cell, the text is cut with a mark at the box's edge. */
export class Text extends Component<TextProps> {
  override measure(space: Size): Size {
    return { width: Math.min(space.width, cells(this.props.text) + 1), height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const shown = fit(this.props.text, canvas.width - 1);
    const column = this.props.centre === true ? Math.max(0, Math.floor((canvas.width - cells(shown) - 1) / 2)) : 0;
    canvas.coloured(0, column, this.props.foreground ?? Colour.White, shown);
  }
}

/** EN 300 706 Table 26's Level 2.5 sizes, which the kit does not name. */
const DOUBLE_WIDTH = 0x0e;

export interface ParagraphProps {
  readonly text: string;
  readonly foreground?: Colour;
  /** At most this many lines; the last cut with an ellipsis. */
  readonly lines?: number;
}

/** Text wrapped to the box's width, a line a row. */
export class Paragraph extends Component<ParagraphProps> {
  #lines(width: number): string[] {
    const most = this.props.lines;
    return most === undefined ? wrap(this.props.text, width - 1) : headline(this.props.text, width - 1, most);
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.#lines(space.width).length, space.height) };
  }

  override paint(canvas: Canvas): void {
    this.#lines(canvas.width).forEach((line, row) => canvas.coloured(row, 0, this.props.foreground ?? Colour.White, line));
  }
}

export interface StyledProps {
  readonly text: string;
  readonly foreground?: Colour;
}

/** Double height: the text covers its row and the row below, across the whole page (clause 12.3). */
export class DoubleHeight extends Component<StyledProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(2, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.doubleHeight(0, 0, this.props.foreground ?? Colour.Yellow, fit(this.props.text, canvas.width - 2));
  }
}

/** Double width (Level 2.5): each letter covers the cell after it, so the letters go in every other cell. */
export class DoubleWidth extends Component<StyledProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const letters = [...fit(this.props.text, Math.floor((canvas.width - 2) / 2))].join(" ");
    canvas.attributed(0, 0, DOUBLE_WIDTH, this.props.foreground ?? Colour.Yellow, letters);
  }
}

/** Flashing text (Table 26), steady again after it. */
export class Flashing extends Component<StyledProps> {
  override measure(space: Size): Size {
    return { width: Math.min(space.width, cells(this.props.text) + 3), height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const shown = fit(this.props.text, canvas.width - 3);
    canvas.codes(0, 0, [this.props.foreground ?? Colour.Red, Attribute.Flash]);
    canvas.text(0, 2, shown);
    canvas.code(0, 2 + cells(shown), Attribute.Steady);
  }
}

/** Concealed text: shown when the viewer presses Reveal (Table 26). */
export class Concealed extends Component<StyledProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.codes(0, 0, [this.props.foreground ?? Colour.Magenta, Attribute.Conceal]);
    canvas.text(0, 2, fit(this.props.text, canvas.width - 2));
  }
}

export interface BandProps {
  readonly text: string;
  readonly background?: Colour;
  readonly foreground?: Colour;
}

/** A row on a background of its own colour, its text in another: "TOP STORY", "THE MAGAZINES". */
export class Band extends Component<BandProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.background(0, 0, this.props.background ?? Colour.Blue);
    canvas.coloured(0, 2, this.props.foreground ?? Colour.White, fit(this.props.text, canvas.width - 4));
  }
}
