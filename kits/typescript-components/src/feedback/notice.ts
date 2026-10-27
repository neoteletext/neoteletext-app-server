// Notices: a line that says what happened, its kind's mark on a chip of
// its kind's colour (core/look.ts) so it shows without colour too. A notice
// stays; a toast is a notice that goes after a few seconds. Toasts are sent
// on the bus (core/library-events.ts), so any component can raise one and
// every shelf the bus's audience reaches shows it: a toast for one viewer
// shows only on their own copy of a page.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import type { ToastEvent } from "../core/library-events.ts";
import { type Kind, KINDS } from "../core/look.ts";
import { fit } from "../core/text.ts";

export interface NoticeProps {
  readonly kind: Kind;
  readonly text: string;
}

/** The mark's chip, always as wide whatever the mark, so notices line up. */
const MARK_CELLS = 2;

function paintNotice(canvas: Canvas, row: number, kind: Kind, text: string): void {
  const look = KINDS[kind];
  const after = canvas.chip(row, 0, look.background, look.foreground, look.mark.padEnd(MARK_CELLS));
  canvas.coloured(row, after, look.background, fit(text, canvas.width - after - 1));
}

/** A notice that stays. */
export class Notice extends Component<NoticeProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    paintNotice(canvas, 0, this.props.kind, this.props.text);
  }
}

export interface ToastShelfProps {
  /** The most toasts shown at once; older ones go first. */
  readonly maxShown?: number;
  /** How long each stays. */
  readonly forMs?: number;
}

interface Shown {
  readonly id: number;
  readonly toast: ToastEvent;
}

interface ToastShelfState {
  readonly shown: readonly Shown[];
}

/** How long a toast stays when not told: long enough to read a line. */
const TOAST_MS = 4_000;
const MOST_TOASTS = 3;

/** Where toasts show: as many rows as it may show, empty while there are none. */
export class ToastShelf extends Component<ToastShelfProps, ToastShelfState> {
  override state: ToastShelfState = { shown: [] };
  #unsubscribe: (() => void) | null = null;
  #next = 0;

  override componentDidMount(): void {
    this.#unsubscribe = this.services.bus.on("toast", (toast) => {
      const forMs = this.props.forMs ?? TOAST_MS;
      const shown: Shown = { id: this.#next, toast };
      this.#next += 1;
      this.setState((state) => ({ shown: [...state.shown, shown].slice(-(this.props.maxShown ?? MOST_TOASTS)) }));
      this.setTimeout(() => this.setState((state) => ({ shown: state.shown.filter((each) => each.id !== shown.id) })), forMs);
    });
  }

  override componentWillUnmount(): void {
    this.#unsubscribe?.();
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(space.height, this.props.maxShown ?? MOST_TOASTS) };
  }

  override paint(canvas: Canvas): void {
    this.state.shown.forEach(({ toast }, row) => paintNotice(canvas, row, toast.kind, toast.text));
  }
}
