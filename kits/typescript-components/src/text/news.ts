// A news service's text: headlines with their page numbers, bylines, page
// numbers that can be clicked, a placeholder while something comes, a ticker
// and a newsflash. What goes to a page is a control, from its number to the
// end of what it says, and only a click there goes.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import type { PointerEvent } from "../core/events.ts";
import { cells, fit, headline, pad } from "../core/text.ts";
import { Attribute, Colour } from "../kit.ts";
import { DATE_FORMATS, formatDate } from "../time/date-text.ts";

/** A page number as a page shows it: three digits, hexadecimal ones capital. */
export function pageNumber(page: number): string {
  return page.toString(16).toUpperCase().padStart(3, "0");
}

export interface HeadlineProps {
  readonly title: string;
  /** The page it is on, written before it in yellow. */
  readonly page?: number;
  readonly foreground?: Colour;
  /** The most lines it takes; one or two, as everywhere on the channel. */
  readonly lines?: number;
}

/** The width a headline's page number takes: three digits and a space, after its colour. */
const NUMBER_CELLS = 4;

/** A headline's lines, everywhere: two, the second cut with an ellipsis. */
export const HEADLINE_LINES = 2;

/** The region of what goes to a page. */
const LINK = "link";

/** A post's title on one or two lines, after its page number when it has one. */
export class Headline extends Component<HeadlineProps> {
  #lines(width: number): string[] {
    const indent = this.props.page === undefined ? 0 : NUMBER_CELLS;
    return headline(this.props.title, width - indent - 1, this.props.lines ?? HEADLINE_LINES);
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.#lines(space.width).length, space.height) };
  }

  override paint(canvas: Canvas): void {
    const indent = this.props.page === undefined ? 0 : NUMBER_CELLS;
    if (this.props.page !== undefined) {
      canvas.coloured(0, 0, Colour.Yellow, pageNumber(this.props.page));
    }
    const lines = this.#lines(canvas.width);
    lines.forEach((line, row) => canvas.coloured(row, indent, this.props.foreground ?? Colour.White, line));
    if (this.props.page !== undefined) {
      // From its number, after its colour's cell, over its lines.
      canvas.control(0, 1, canvas.width - 1, lines.length, LINK);
    }
  }

  /** A click on a headline with a page goes there. */
  override onClick(event: PointerEvent): boolean {
    if (this.props.page === undefined || event.region !== LINK) {
      return false;
    }
    this.services.goto(event.session, this.props.page);
    return true;
  }
}

export interface BylineProps {
  readonly writer: string;
  /** Milliseconds since the Unix epoch. */
  readonly date: number;
  readonly labels?: readonly string[];
}

/** Who wrote it and when, in cyan; its labels under it in green. */
export class Byline extends Component<BylineProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.props.labels === undefined ? 1 : 2, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.coloured(0, 0, Colour.Cyan, fit(`by ${this.props.writer}, ${formatDate(this.props.date, DATE_FORMATS.stamp)}`, canvas.width - 1));
    if (this.props.labels !== undefined) {
      canvas.coloured(1, 0, Colour.Green, fit(this.props.labels.join(", "), canvas.width - 1));
    }
  }
}


export interface PlaceholderProps {
  readonly text: string;
}

/** What shows while something is still coming: a line centred in the box. */
export class Placeholder extends Component<PlaceholderProps> {
  override measure(space: Size): Size {
    return space;
  }

  override paint(canvas: Canvas): void {
    const shown = fit(this.props.text, canvas.width - 1);
    canvas.coloured(Math.floor(canvas.height / 2), Math.max(0, Math.floor((canvas.width - cells(shown) - 1) / 2)), Colour.White, shown);
  }
}

export interface PageLinkProps {
  readonly page: number;
  readonly label: string;
  readonly foreground?: Colour;
}

/** "120 Lists": a page number and what is there; clicking it goes there. */
export class PageLink extends Component<PageLinkProps> {
  override measure(space: Size): Size {
    return { width: Math.min(space.width, cells(this.props.label) + NUMBER_CELLS + 2), height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.coloured(0, 0, Colour.Yellow, pageNumber(this.props.page));
    const label = fit(this.props.label, canvas.width - NUMBER_CELLS - 1);
    canvas.coloured(0, NUMBER_CELLS, this.props.foreground ?? Colour.White, label);
    // From its number to the end of its label, after their colours' cells.
    canvas.control(0, 1, NUMBER_CELLS + cells(label), 1, LINK);
  }

  override onClick(event: PointerEvent): boolean {
    if (event.region !== LINK) {
      return false;
    }
    this.services.goto(event.session, this.props.page);
    return true;
  }
}

export interface TickerProps {
  /** The items it shows in turn. */
  readonly items: readonly string[];
  /** How long each shows. */
  readonly everyMs: number;
  readonly background?: Colour;
}

interface TickerState {
  readonly at: number;
}

/** A band whose text changes every few seconds: a timer of its own, cleared when it leaves the page. */
export class Ticker extends Component<TickerProps, TickerState> {
  override state: TickerState = { at: 0 };
  #timer = -1;

  override componentDidMount(): void {
    this.#timer = this.setInterval(() => this.setState((state) => ({ at: state.at + 1 })), this.props.everyMs, { whileViewed: true });
  }

  override componentWillUnmount(): void {
    this.clearTimer(this.#timer);
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const items = this.props.items;
    const shown = items.length === 0 ? "" : (items[this.state.at % items.length] ?? "");
    canvas.background(0, 0, this.props.background ?? Colour.Blue);
    canvas.coloured(0, 2, Colour.White, pad(shown, canvas.width - 4));
  }
}

export interface NewsflashProps {
  readonly text: string;
}

/** NEWSFLASH, flashing in red, then the news in yellow. */
export class Newsflash extends Component<NewsflashProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const word = "NEWSFLASH";
    canvas.codes(0, 0, [Colour.Red, Attribute.Flash]);
    canvas.text(0, 2, word);
    canvas.code(0, 2 + word.length, Attribute.Steady);
    canvas.coloured(0, 3 + word.length, Colour.Yellow, fit(this.props.text, canvas.width - word.length - 4));
  }
}
