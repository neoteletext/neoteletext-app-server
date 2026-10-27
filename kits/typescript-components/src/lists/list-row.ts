// A list's row as a component: the band, the mark and the colours every
// list's row has in each state (core/look.ts), and whatever the row holds,
// laid out from ITEM_TEXT: a line of text by default, or any components a
// list's user gives it. A row that can be pressed declares its band's cells
// as a control (core/controls.ts), which the browser lights as the pointer
// passes, and only a click there presses it: not one on its indent, a
// tree's guides or a control's label column. ListRows is a list's rows, as many as are shown from its top,
// each one a ListRow, so every list chooses, highlights and takes clicks
// alike.

import type { Canvas } from "../core/canvas.ts";
import { type Box, Component, type LayoutChild, type Size, stack } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import type { PointerEvent } from "../core/events.ts";
import { CHOSEN, DISABLED, ITEM_TEXT, LABEL, MARK, paintEmpty, SELECTED } from "../core/look.ts";
import { fit, headline } from "../core/text.ts";
import type { Colour } from "../kit.ts";

/** A row's state, for what it holds to draw itself to match. */
export interface RowLook {
  readonly chosen: boolean;
  /** The list has the keys. */
  readonly focused: boolean;
  readonly disabled: boolean;
  /** On a band: the chosen row of the list with the keys. */
  readonly band: boolean;
  /** The foreground to draw in, to read on the band or off it. */
  readonly foreground: Colour;
}

export interface ListRowProps {
  readonly chosen: boolean;
  readonly focused: boolean;
  readonly disabled?: boolean;
  /** Its foreground off the band, LABEL when absent. */
  readonly foreground?: Colour;
  /** Rows it takes: 1 when absent. */
  readonly rows?: number;
  /** Cells before the row's band and mark: a tree's depth, a control's label column. */
  readonly indent?: number;
  /** What those cells hold: a tree's guide lines. */
  readonly before?: Child;
  /** What it holds, drawn to its look. */
  readonly content: (look: RowLook) => Child;
  readonly onPress?: (session: number) => void;
}

/** Where a row's content starts: on the cell before its text, which its content's colour takes. */
const CONTENT_AT = ITEM_TEXT - 1;
/** The region its band is. */
const BAND = "band";

export class ListRow extends Component<ListRowProps> {
  get look(): RowLook {
    const { chosen, focused } = this.props;
    const disabled = this.props.disabled === true;
    const band = chosen && focused && !disabled;
    const foreground = band ? CHOSEN.foreground : disabled ? DISABLED : chosen ? SELECTED : (this.props.foreground ?? LABEL);
    return { chosen, focused, disabled, band, foreground };
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.props.rows ?? 1, space.height) };
  }

  get #indent(): number {
    return this.props.indent ?? 0;
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    const [first, ...rest] = children;
    const content = this.props.before === undefined ? children : rest;
    if (this.props.before !== undefined && first !== undefined) {
      first.place({ ...box, width: Math.min(box.width, this.#indent) });
    }
    const at = this.#indent + CONTENT_AT;
    stack({ ...box, column: box.column + at, width: Math.max(0, box.width - at) }, content);
  }

  override paint(canvas: Canvas): void {
    const look = this.look;
    if (look.band) {
      for (let row = 0; row < canvas.height; row += 1) {
        canvas.background(row, this.#indent, CHOSEN.background);
      }
    } else if (look.chosen) {
      canvas.coloured(0, this.#indent, SELECTED, MARK);
    }
    // Where its band is, or would be, on every row it takes: one control, its rows joined.
    if (this.props.onPress !== undefined && !look.disabled) {
      canvas.bandControl(0, this.#indent, canvas.height, BAND);
    }
  }

  override render(): Child {
    const content = this.props.content(this.look);
    return this.props.before === undefined ? content : [this.props.before, content];
  }

  override onClick(event: PointerEvent): boolean {
    if (this.props.disabled === true || this.props.onPress === undefined || event.region !== BAND) {
      return false;
    }
    this.props.onPress(event.session);
    return true;
  }
}

export interface ItemTextProps {
  readonly text: string;
  readonly foreground: Colour;
  /** Rows to wrap it over: 1 when absent. */
  readonly rows?: number;
}

/** A row's text in its look's foreground, wrapped over its rows at word boundaries, the last cut with an ellipsis. */
export class ItemText extends Component<ItemTextProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.props.rows ?? 1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const lines = headline(this.props.text, canvas.width - 1, Math.max(1, canvas.height));
    lines.forEach((line, row) => canvas.coloured(row, 0, this.props.foreground, fit(line, canvas.width - 1)));
  }
}

export interface ListRowsProps<T> {
  readonly items: readonly T[];
  /** Cells before each row's band, and what they hold. */
  readonly indent?: (item: T, index: number) => number;
  readonly before?: (item: T, index: number) => Child;
  /** The first item shown. */
  readonly top?: number;
  /** How many items are shown from the top. */
  readonly shown: number;
  /** The chosen item, or null for none. */
  readonly chosen: number | null;
  readonly focused: boolean;
  /** Rows each item takes. */
  readonly itemRows?: number;
  /** What an empty list says. */
  readonly empty?: string;
  /** What an item's row holds: its text in its look by default. */
  readonly render?: (item: T, look: RowLook, index: number) => Child;
  /** An item's text, for the default row: the item itself when it is text. */
  readonly text?: (item: T, index: number) => string;
  readonly disabled?: (item: T, index: number) => boolean;
  /** An item's foreground off the band. */
  readonly foreground?: (item: T, index: number) => Colour | undefined;
  readonly onPick: (index: number, session: number) => void;
}

/** A list's rows, a ListRow each, as many as are shown from `top`. */
export class ListRows<T> extends Component<ListRowsProps<T>> {
  /** The rows its items take, or one for what an empty list says. */
  override measure(space: Size): Size {
    const items = Math.max(1, Math.min(this.props.shown, this.props.items.length - (this.props.top ?? 0)));
    return { width: space.width, height: Math.min(space.height, items * (this.props.itemRows ?? 1)) };
  }

  override paint(canvas: Canvas): void {
    if (this.props.items.length === 0) {
      paintEmpty(canvas, this.props.empty ?? "nothing to show");
    }
  }

  override render(): Child {
    const { items, shown, chosen, focused } = this.props;
    const top = this.props.top ?? 0;
    const rows = this.props.itemRows ?? 1;
    const textOf = this.props.text ?? ((item: T): string => String(item));
    return items.slice(top, top + shown).map((item, offset) => {
      const index = top + offset;
      const disabled = this.props.disabled?.(item, index) === true;
      const foreground = this.props.foreground?.(item, index);
      const before = this.props.before?.(item, index);
      return h(ListRow, {
        key: index,
        chosen: index === chosen,
        focused,
        disabled,
        rows,
        indent: this.props.indent?.(item, index) ?? 0,
        ...(before === undefined ? {} : { before }),
        ...(foreground === undefined ? {} : { foreground }),
        content: (look: RowLook) => this.props.render?.(item, look, index) ?? h(ItemText, { text: textOf(item, index), foreground: look.foreground, rows }),
        onPress: (session: number) => this.props.onPick(index, session),
      });
    });
  }
}
