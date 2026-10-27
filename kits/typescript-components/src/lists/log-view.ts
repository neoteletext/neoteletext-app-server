// A log: entries as they come, the newest at the bottom, each with its
// time and its level's tag in the level's colour. It follows the newest
// until the viewer scrolls back, and says so; End follows again.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import { isPress, type KeyEvent, type WheelEvent } from "../core/events.ts";
import { listStep, WheelLines } from "../core/list-cursor.ts";
import { paintEmpty } from "../core/look.ts";
import { fit } from "../core/text.ts";
import { Colour, Key, textColour } from "../kit.ts";
import { Control, type ControlProps } from "../input/control.ts";
import { Columns, Rows } from "../layout/layouts.ts";
import { Badges } from "../text/badge.ts";
import { BAR_CELLS, ScrollBar } from "./scroll.ts";
import { DATE_FORMATS, formatDate } from "../time/date-text.ts";

export type LogLevel = "debug" | "info" | "warning" | "error";

export interface LogEntry {
  /** Milliseconds since the Unix epoch. */
  readonly at: number;
  readonly level: LogLevel;
  readonly text: string;
}

export interface LogViewProps extends ControlProps {
  readonly entries: readonly LogEntry[];
  /** The entries shown at once. */
  readonly shown: number;
  readonly empty?: string;
}

interface LogViewState {
  /** The first entry shown, or null while it follows the newest. */
  readonly top: number | null;
}

/** Each level's tag and colour: the tag says it without colour. */
const LEVELS: Readonly<Record<LogLevel, { readonly tag: string; readonly foreground: Colour }>> = {
  debug: { tag: "DBG", foreground: Colour.Blue },
  info: { tag: "INF", foreground: Colour.Cyan },
  warning: { tag: "WRN", foreground: Colour.Yellow },
  error: { tag: "ERR", foreground: Colour.Red },
};
/** Where an entry's parts start: its time after the line's colour cell ("HH:MM:SS"), its tag after its colour's cell, its text after its own. */
const TIME_CELLS = 8;
const TAG_AT = 1 + TIME_CELLS;
const TAG_CELLS = 3;
const TEXT_AT = TAG_AT + 1 + TAG_CELLS;

function time(epochMs: number): string {
  return formatDate(epochMs, DATE_FORMATS.seconds);
}

class LogLines extends Component<{ readonly entries: readonly LogEntry[]; readonly empty: string }> {
  override measure(space: Size): Size {
    return space;
  }

  override paint(canvas: Canvas): void {
    if (this.props.entries.length === 0) {
      paintEmpty(canvas, this.props.empty);
      return;
    }
    this.props.entries.forEach((entry, row) => {
      const level = LEVELS[entry.level];
      canvas.codes(row, 0, [textColour(Colour.White)]);
      canvas.text(row, 1, time(entry.at));
      canvas.coloured(row, TAG_AT, level.foreground, level.tag);
      canvas.coloured(row, TEXT_AT, entry.level === "error" ? Colour.Red : Colour.White, fit(entry.text, canvas.width - TEXT_AT - 1));
    });
  }
}

export class LogView extends Control<LogViewProps, LogViewState> {
  override state: LogViewState = { top: null };
  readonly #wheel = new WheelLines();

  protected override keysHint(): string {
    return "Up PgUp scroll back, End follows";
  }

  get #last(): number {
    return Math.max(0, this.props.entries.length - this.props.shown);
  }

  override render(): Child {
    const top = this.state.top ?? this.#last;
    const following = this.state.top === null;
    return h(
      Rows,
      {},
      h(
        Columns,
        { widths: [null, BAR_CELLS], height: this.props.shown },
        h(LogLines, { entries: this.props.entries.slice(top, top + this.props.shown), empty: this.props.empty ?? "no entries yet" }),
        h(ScrollBar, { top, total: this.props.entries.length, shown: this.props.shown, disabled: this.disabled, onScroll: (to) => this.#scroll(to) }),
      ),
      h(
        Badges,
        following
          ? { badges: [{ text: "LIVE", background: Colour.Red, flash: true }], text: `${this.props.entries.length} entries` }
          : { badges: [{ text: "PAUSED", background: Colour.Blue }], text: "End follows the newest again" },
      ),
    );
  }

  #scroll(top: number): void {
    this.setState({ top: top >= this.#last ? null : Math.max(0, top) });
  }

  override onKey(event: KeyEvent): boolean {
    if (isPress(event, Key.End)) {
      this.setState({ top: null });
      return true;
    }
    const step = listStep(event, this.props.shown);
    if (step === null) {
      return false;
    }
    const top = this.state.top ?? this.#last;
    this.#scroll(Number.isFinite(step) ? top + step : step > 0 ? this.#last : 0);
    return true;
  }

  override onWheel(event: WheelEvent): boolean {
    if (this.disabled) {
      return false;
    }
    this.#scroll((this.state.top ?? this.#last) + this.#wheel.lines(event.dy));
    return true;
  }
}
