// Scrolling: a scroll bar of whole cells that tells its parent where to go,
// as React's controlled components do, and lines that scroll beside one,
// moved with the keys (core/list-cursor.ts), the wheel and the bar.

import type { Canvas } from "../core/canvas.ts";
import type { Size } from "../core/component.ts";
import { Component } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import type { KeyEvent, PointerEvent, WheelEvent } from "../core/events.ts";
import { listStep, WheelLines } from "../core/list-cursor.ts";
import { FULL_BLOCK, LEFT_HALF } from "../core/mosaics.ts";
import { Colour, mosaicColour } from "../kit.ts";
import { Control, type ControlProps } from "../input/control.ts";
import { Columns } from "../layout/layouts.ts";
import { type Line, LinesBlock } from "../text/lines.ts";

/** The scroll bar's cells: its colour, then the bar. */
export const BAR_CELLS = 2;
/** The left button, held, as a move's buttons say (docs/applications.md section 2). */
const LEFT_HELD = 1;

/** The scroll bar's block: its first cell and its length, for `shown` of `total` from `top` on a track of `track` cells. */
export function thumb(top: number, total: number, track: number, shown: number): { start: number; length: number } {
  if (total <= shown) {
    return { start: 0, length: track };
  }
  const length = Math.max(1, Math.min(track, Math.round((track * shown) / total)));
  const lastTop = total - shown;
  return { start: Math.round(((track - length) * Math.min(top, lastTop)) / lastTop), length };
}

/** The first item shown when the block is dragged to start at cell `start`: `thumb` turned round. */
export function topAt(start: number, total: number, track: number, shown: number): number {
  const { length } = thumb(0, total, track, shown);
  const room = track - length;
  const most = Math.max(0, total - shown);
  return room <= 0 || most <= 0 ? 0 : Math.max(0, Math.min(most, Math.round((start * most) / room)));
}

export interface ScrollBarProps {
  readonly top: number;
  readonly total: number;
  readonly shown: number;
  /** Where the viewer moved it: the first item to show. */
  readonly onScroll: (top: number) => void;
  /** What it scrolls cannot be used: it is drawn, but takes nothing and declares nothing. */
  readonly disabled?: boolean;
}

/**
 * A scroll bar down its box: grip the block and drag it, or press above or
 * below it to turn a screen. Nothing while everything shows. A press
 * anywhere on it acts, so the whole bar is one control, which the browser
 * shows pressed while the block is dragged; a drag is sent with its button
 * held, so it needs no pointer area.
 */
export class ScrollBar extends Component<ScrollBarProps> {
  #grip: number | null = null;

  override measure(space: Size): Size {
    return { width: Math.min(BAR_CELLS, space.width), height: space.height };
  }

  override paint(canvas: Canvas): void {
    if (this.props.total <= this.props.shown) {
      return;
    }
    const { start, length } = thumb(this.props.top, this.props.total, canvas.height, this.props.shown);
    for (let cell = 0; cell < canvas.height; cell += 1) {
      const inside = cell >= start && cell < start + length;
      canvas.code(cell, 0, mosaicColour(inside ? Colour.White : Colour.Blue));
      canvas.code(cell, 1, inside ? FULL_BLOCK : LEFT_HALF);
    }
    if (this.props.disabled !== true) {
      canvas.control(0, 0, canvas.width, canvas.height);
    }
  }

  override onPointer(event: PointerEvent): boolean {
    const { top, total, shown } = this.props;
    const track = this.box.height;
    if (total <= shown || this.props.disabled === true) {
      return false;
    }
    const cell = event.local.row;
    if (event.action === "press") {
      const { start, length } = thumb(top, total, track, shown);
      // Above or below the block, a screen's worth; on it, the block is gripped where it was pressed.
      this.#grip = null;
      if (cell < start) {
        this.props.onScroll(Math.max(0, top - Math.max(1, shown - 1)));
      } else if (cell >= start + length) {
        this.props.onScroll(Math.min(total - shown, top + Math.max(1, shown - 1)));
      } else {
        this.#grip = cell - start;
      }
      return true;
    }
    if (event.action === "move" && (event.buttons & LEFT_HELD) !== 0 && this.#grip !== null) {
      this.props.onScroll(topAt(cell - this.#grip, total, track, shown));
      return true;
    }
    this.#grip = null;
    return true;
  }
}

export interface ScrollingTextProps extends ControlProps {
  readonly lines: readonly Line[];
  /** The rows it shows at once. */
  readonly shown: number;
  readonly empty?: string;
}

interface ScrollingTextState {
  readonly top: number;
}

/** Lines that scroll in their box with the keys, the wheel and a scroll bar. */
export class ScrollingText extends Control<ScrollingTextProps, ScrollingTextState> {
  override state: ScrollingTextState = { top: 0 };
  readonly #wheel = new WheelLines();

  protected override keysHint(): string {
    return "PgDn PgUp arrows Home End, wheel, drag the bar";
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.props.shown, space.height) };
  }

  get #last(): number {
    return Math.max(0, this.props.lines.length - this.props.shown);
  }

  override render(): Child {
    const { lines, shown } = this.props;
    const top = Math.min(this.state.top, this.#last);
    return h(
      Columns,
      { widths: [null, BAR_CELLS] },
      h(LinesBlock, { lines: lines.slice(top, top + shown), empty: this.props.empty ?? "nothing to read" }),
      h(ScrollBar, { top, total: lines.length, shown, disabled: this.disabled, onScroll: (to) => this.setState({ top: to }) }),
    );
  }

  override onKey(event: KeyEvent): boolean {
    const step = listStep(event, this.props.shown);
    if (step === null || this.disabled) {
      return false;
    }
    this.setState({ top: Math.max(0, Math.min(this.#last, Number.isFinite(step) ? this.state.top + step : step > 0 ? this.#last : 0)) });
    return true;
  }

  override onWheel(event: WheelEvent): boolean {
    if (this.disabled) {
      return false;
    }
    this.setState({ top: Math.max(0, Math.min(this.#last, this.state.top + this.#wheel.lines(event.dy))) });
    return true;
  }
}

