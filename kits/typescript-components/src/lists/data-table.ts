// A table of rows and columns under a heading: each column starts with its
// colour's cell, which is the gap between columns; the heading is on a band;
// the chosen row is on the band while the table has the keys (core/
// look.ts); rows may alternate their foreground; more rows than fit scroll
// beside a scroll bar. A column is sorted by its digit (the page takes the
// digits) or a click on its heading, and again the other way.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import type { KeyEvent, PointerEvent, WheelEvent } from "../core/events.ts";
import { choosing, digitOf } from "../core/keys.ts";
import { keepInView, listStep, moveChoice, WheelLines } from "../core/list-cursor.ts";
import { CHOSEN, MARK, paintEmpty, SELECTED } from "../core/look.ts";
import type { PageNeeds } from "../core/needs.ts";
import { fit, pad, padStart } from "../core/text.ts";
import { Colour } from "../kit.ts";
import { Control, type ControlProps } from "../input/control.ts";
import { Columns, Rows } from "../layout/layouts.ts";
import { BAR_CELLS, ScrollBar } from "./scroll.ts";

export interface TableColumn {
  readonly title: string;
  /** Its cells, its colour's cell included. */
  readonly width: number;
  readonly align?: "left" | "right";
  readonly foreground?: Colour;
}

export interface DataTableProps extends ControlProps {
  readonly columns: readonly TableColumn[];
  readonly rows: readonly (readonly string[])[];
  /** The body's rows shown at once. */
  readonly shown: number;
  /** A row can be chosen, and Enter, Space or a click chooses it. */
  readonly selectable?: boolean;
  readonly sortable?: boolean;
  /** Rows alternate white and cyan. */
  readonly striped?: boolean;
  readonly empty?: string;
  readonly onChoose?: (row: readonly string[], session: number) => void;
}

interface DataTableState {
  readonly top: number;
  readonly chosen: number;
  readonly sortBy: number | null;
  readonly descending: boolean;
}

/** The cells before the first column: a band's colour and New Background, kept on every row so the columns line up. */
const LEAD = 2;
/** What marks the sorted column: up for ascending; the English set has no down arrow (EN 300 706 Table 36), so a v for descending. */
const ASCENDING = "↑";
const DESCENDING = "v";

/** Each column's first cell. */
function starts(columns: readonly TableColumn[]): number[] {
  let column = LEAD;
  return columns.map((each) => {
    const at = column;
    column += each.width;
    return at;
  });
}

/** Numbers sort as numbers, the rest as text. */
function compare(a: string, b: string): number {
  const x = Number.parseFloat(a.replace(/,/gu, ""));
  const y = Number.parseFloat(b.replace(/,/gu, ""));
  return Number.isNaN(x) || Number.isNaN(y) ? a.localeCompare(b) : x - y;
}

function cell(text: string, column: TableColumn): string {
  return column.align === "right" ? padStart(text, column.width - 1) : pad(text, column.width - 1);
}

interface HeadingProps {
  readonly columns: readonly TableColumn[];
  /** A click on a column's title sorts by it: each title is a control. */
  readonly sortable: boolean;
  readonly sortBy: number | null;
  readonly descending: boolean;
  readonly onSort: (column: number) => void;
}

class Heading extends Component<HeadingProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.background(0, 0, Colour.Blue);
    const at = starts(this.props.columns);
    this.props.columns.forEach((column, index) => {
      const mark = index === this.props.sortBy ? (this.props.descending ? DESCENDING : ASCENDING) : "";
      canvas.coloured(0, at[index] ?? 0, Colour.Yellow, fit(`${column.title}${mark}`, column.width - 1));
      if (this.props.sortable) {
        // The column after its colour's cell.
        canvas.control(0, (at[index] ?? 0) + 1, column.width - 1, 1, String(index));
      }
    });
  }

  override onClick(event: PointerEvent): boolean {
    if (event.region === undefined) {
      return false;
    }
    this.props.onSort(Number(event.region));
    return true;
  }
}

interface BodyProps {
  readonly columns: readonly TableColumn[];
  readonly rows: readonly (readonly string[])[];
  readonly top: number;
  readonly chosen: number | null;
  readonly focused: boolean;
  readonly striped: boolean;
  /** A click on a row chooses it: each row is a control. */
  readonly selectable: boolean;
  readonly empty: string;
  readonly onPick: (index: number, session: number) => void;
}

class Body extends Component<BodyProps> {
  override measure(space: Size): Size {
    return space;
  }

  override paint(canvas: Canvas): void {
    if (this.props.rows.length === 0) {
      paintEmpty(canvas, this.props.empty);
      return;
    }
    const at = starts(this.props.columns);
    this.props.rows.slice(this.props.top, this.props.top + canvas.height).forEach((values, row) => {
      const index = this.props.top + row;
      const chosen = index === this.props.chosen;
      const banded = chosen && this.props.focused;
      if (banded) {
        canvas.background(row, 0, CHOSEN.background);
      } else if (chosen) {
        canvas.coloured(row, 0, SELECTED, MARK);
      }
      this.props.columns.forEach((column, place) => {
        const foreground = banded ? CHOSEN.foreground : chosen ? SELECTED : this.props.striped && index % 2 === 1 ? Colour.Cyan : (column.foreground ?? Colour.White);
        canvas.coloured(row, at[place] ?? 0, foreground, cell(values[place] ?? "", column));
      });
      if (this.props.selectable) {
        canvas.bandControl(row, 0, 1, String(index));
      }
    });
  }

  override onClick(event: PointerEvent): boolean {
    if (event.region === undefined) {
      return false;
    }
    this.props.onPick(Number(event.region), event.session);
    return true;
  }
}

export class DataTable extends Control<DataTableProps, DataTableState> {
  override state: DataTableState = { top: 0, chosen: 0, sortBy: null, descending: false };
  readonly #wheel = new WheelLines();

  protected override keyNeeds(): PageNeeds {
    return { digits: this.props.sortable === true };
  }

  protected override keysHint(): string {
    return [this.props.selectable === true ? "Up Down choose, Enter" : "Up Down scroll", this.props.sortable === true ? "1-9 sorts" : ""].filter((part) => part !== "").join("; ");
  }

  #sorted(): readonly (readonly string[])[] {
    const { sortBy, descending } = this.state;
    if (sortBy === null) {
      return this.props.rows;
    }
    const sorted = [...this.props.rows].sort((a, b) => compare(a[sortBy] ?? "", b[sortBy] ?? ""));
    return descending ? sorted.reverse() : sorted;
  }

  #sort(column: number): void {
    if (this.props.sortable !== true) {
      return;
    }
    this.setState({ sortBy: column, descending: this.state.sortBy === column && !this.state.descending, top: 0, chosen: 0 });
  }

  #choose(index: number, session: number): void {
    this.setState({ chosen: index });
    const row = this.#sorted()[index];
    if (row !== undefined && this.props.selectable === true) {
      this.activate(row, session);
      this.props.onChoose?.(row, session);
    }
  }

  override render(): Child {
    return h(
      Rows,
      {},
      h(Heading, { columns: this.props.columns, sortable: this.props.sortable === true && !this.disabled, sortBy: this.state.sortBy, descending: this.state.descending, onSort: (column: number) => this.#sort(column) }),
      h(
        Columns,
        { widths: [null, BAR_CELLS], height: this.props.shown },
        h(Body, {
          columns: this.props.columns,
          rows: this.#sorted(),
          top: this.state.top,
          chosen: this.props.selectable === true ? this.state.chosen : null,
          focused: this.focused,
          striped: this.props.striped === true,
          selectable: this.props.selectable === true && !this.disabled,
          empty: this.props.empty ?? "no rows",
          onPick: (index: number, session: number) => this.#choose(index, session),
        }),
        h(ScrollBar, { top: this.state.top, total: this.props.rows.length, shown: this.props.shown, disabled: this.disabled, onScroll: (top) => this.setState({ top }) }),
      ),
    );
  }

  override onKey(event: KeyEvent): boolean {
    if (this.disabled) {
      return false;
    }
    const count = this.props.rows.length;
    const digit = digitOf(event);
    if (this.props.sortable === true && digit !== null && digit >= 1 && digit <= this.props.columns.length) {
      this.#sort(digit - 1);
      return true;
    }
    if (choosing(event) && this.props.selectable === true) {
      this.#choose(this.state.chosen, event.session);
      return true;
    }
    const step = listStep(event, this.props.shown);
    if (step === null) {
      return false;
    }
    if (this.props.selectable === true) {
      const chosen = moveChoice(this.state.chosen, step, count);
      this.setState({ chosen, top: keepInView(this.state.top, chosen, this.props.shown, count) });
      this.play("move", event.session);
    } else {
      const last = Math.max(0, count - this.props.shown);
      this.setState({ top: Math.max(0, Math.min(last, Number.isFinite(step) ? this.state.top + step : step > 0 ? last : 0)) });
    }
    return true;
  }

  override onWheel(event: WheelEvent): boolean {
    if (this.disabled) {
      return false;
    }
    const last = Math.max(0, this.props.rows.length - this.props.shown);
    this.setState({ top: Math.max(0, Math.min(last, this.state.top + this.#wheel.lines(event.dy))) });
    return true;
  }
}
