// Pagination: which of a run of pages is shown, and the way to the others.
// Three looks: the numbers round the current one ("← 1 .. 4 5 6 .. 20 →",
// the current on a band), a count ("← Page 5 of 20 →"), and a row of mosaic
// dots, the current one large. Left and Right, Home and End, or a click
// move it; each number, arrow or dot that goes somewhere is a control.

import { CHIP_CELLS, type Canvas } from "../core/canvas.ts";
import type { Size } from "../core/component.ts";
import { isPress, type KeyEvent, type PointerEvent } from "../core/events.ts";
import { CHOSEN, DISABLED, DISABLED_CHIP } from "../core/look.ts";
import { FULL_BLOCK, MOSAIC_BASE, SIXEL } from "../core/mosaics.ts";
import { cells } from "../core/text.ts";
import { Colour, Key } from "../kit.ts";
import { type ValueProps, ValueControl } from "../input/control.ts";

export type PaginationStyle = "numbers" | "count" | "dots";

export interface PaginationProps extends ValueProps<number> {
  /** How many pages there are. */
  readonly count: number;
  readonly style?: PaginationStyle;
}

/** One thing in the row: its text, the page a click on it goes to, and what it is. */
interface Token {
  readonly text: string;
  readonly target: number | null;
  readonly current: boolean;
  readonly arrow: boolean;
}

/** The pages either side of the current one that the numbers show. */
const AROUND = 1;
/** What stands for pages left out. */
const GAP = "..";
/** A dot for each page that is not the current one: its middle two sixels. */
const SMALL_DOT = MOSAIC_BASE | SIXEL.middleLeft | SIXEL.middleRight;

export class Pagination extends ValueControl<number, PaginationProps> {
  protected empty(): number {
    return 0;
  }

  protected override keysHint(): string {
    return "Left Right, Home End or a click turn the page";
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  #tokens(): Token[] {
    const { count } = this.props;
    const current = this.value;
    const previous: Token = { text: "←", target: current > 0 ? current - 1 : null, current: false, arrow: true };
    const next: Token = { text: "→", target: current < count - 1 ? current + 1 : null, current: false, arrow: true };
    if ((this.props.style ?? "numbers") === "count") {
      return [previous, { text: `Page ${current + 1} of ${count}`, target: null, current: false, arrow: false }, next];
    }
    const pages = [0, count - 1, ...Array.from({ length: 2 * AROUND + 1 }, (_, offset) => current - AROUND + offset)];
    const numbers: Token[] = [];
    let last = -1;
    for (const page of [...new Set(pages.filter((each) => each >= 0 && each < count))].sort((a, b) => a - b)) {
      if (page > last + 1) {
        numbers.push({ text: GAP, target: null, current: false, arrow: false });
      }
      numbers.push({ text: String(page + 1), target: page, current: page === current, arrow: false });
      last = page;
    }
    return [previous, ...numbers, next];
  }

  override paint(canvas: Canvas): void {
    if ((this.props.style ?? "numbers") === "dots") {
      const shownDots = Math.min(this.props.count, Math.floor((canvas.width - 1) / 2));
      // A space between dots: a dot a cell would run together.
      const codes = Array.from({ length: shownDots }, (_, page) => [page === this.value ? FULL_BLOCK : SMALL_DOT, MOSAIC_BASE]).flat();
      canvas.mosaics(0, 0, this.disabled ? DISABLED : Colour.White, codes);
      if (!this.disabled) {
        for (let page = 0; page < shownDots; page += 1) {
          // The dot, not the space after it.
          canvas.control(0, 1 + 2 * page, 1, 1, String(page));
        }
      }
      return;
    }
    let column = 0;
    for (const token of this.#tokens()) {
      const width = cells(token.text) + (token.current ? CHIP_CELLS : 1);
      if (column + width > canvas.width) {
        break;
      }
      if (token.current) {
        const look = this.disabled ? DISABLED_CHIP : { background: this.focused ? CHOSEN.background : Colour.Cyan, foreground: CHOSEN.foreground };
        canvas.chip(0, column, look.background, look.foreground, token.text);
      } else {
        // An arrow with nowhere to go is dimmed.
        canvas.coloured(0, column, this.disabled || (token.arrow && token.target === null) ? DISABLED : Colour.Cyan, token.text);
      }
      if (token.target !== null && !this.disabled) {
        if (token.current) {
          canvas.chipControl(0, column, column + width, String(token.target));
        } else {
          canvas.control(0, column + 1, cells(token.text), 1, String(token.target));
        }
      }
      column += width;
    }
  }

  #go(page: number, session: number): void {
    this.change(Math.max(0, Math.min(this.props.count - 1, page)), session);
  }

  override onKey(event: KeyEvent): boolean {
    const current = this.value;
    const to = isPress(event, Key.Right) ? current + 1 : isPress(event, Key.Left) ? current - 1 : isPress(event, Key.Home) ? 0 : isPress(event, Key.End) ? this.props.count - 1 : null;
    if (to === null || this.disabled) {
      return false;
    }
    this.#go(to, event.session);
    return true;
  }

  override onClick(event: PointerEvent): boolean {
    if (event.region !== undefined) {
      this.#go(Number(event.region), event.session);
    }
    return true;
  }
}
