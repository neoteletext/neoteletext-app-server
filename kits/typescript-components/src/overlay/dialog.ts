// Dialogs: a question over the page, which keeps the keys until it is
// answered. Teletext draws a row from its first cell, so a dialog covers the
// whole of each row it is on: spaces in its margins and, between them, a
// background of its own with its title on a band of its kind's colour, its
// message, its children (a field, a spinner) and its buttons. Left, Right
// and Tab choose a button, Enter answers with it, Escape answers with none;
// a click on a button answers with it. A modal lays a dialog over a page,
// and keeps every click and turn of the wheel from the page under it.

import { type Canvas, NEW_BACKGROUND_CELL } from "../core/canvas.ts";
import { type Box, Component, type LayoutChild, type Size, stack, stackSize } from "../core/component.ts";
import { type Child, type Element, h } from "../core/element.ts";
import { isPress, type KeyEvent, type PointerEvent, type WheelEvent } from "../core/events.ts";
import { CHOSEN, type Kind, KINDS } from "../core/look.ts";
import { cells, fit, wrap } from "../core/text.ts";
import { Attribute, Colour, englishCodes, Key, SPACE, textColour } from "../kit.ts";
import { Control, type ControlProps } from "../input/control.ts";
import { At, Layers, Rows } from "../layout/layouts.ts";

export interface DialogProps extends ControlProps {
  readonly title: string;
  readonly message?: string;
  readonly kind?: Kind;
  /** The buttons, left to right; none for a dialog that only says something is happening. */
  readonly buttons?: readonly string[];
  /** The button chosen to begin with. */
  readonly initial?: number;
  /** The cells of margin either side. */
  readonly inset?: number;
  /** Its children have the keys first: a field to type in. */
  readonly childFirst?: boolean;
  /** The button pressed, or null for Escape. */
  readonly onAnswer?: (button: number | null, session: number) => void;
}

interface DialogState {
  readonly chosen: number;
}

/** The margin either side when not told. */
const INSET = 2;
/** The cells a row's background takes before its text: its colour and New Background. */
const BACKGROUND_CELLS = 2;
/** A button's cells beyond its label: its background's colour, New Background, its ink, and the dialog's background again after it (two cells). */
const BUTTON_CELLS = 5;
/** The region a button's band is, by its place. */
const BUTTON_REGION = "button-";
/** Of those, the band's beyond the label: New Background and the ink's cell. */
const BUTTON_BAND_CELLS = 2;
/** The dialog's own background. */
const BACKGROUND = Colour.Blue;
/** Rows round the content: the title band, a row before the buttons, the buttons, a row under them. */
const TITLE_ROWS = 1;
const FOOT_ROWS = 3;

export class Dialog extends Control<DialogProps, DialogState> {
  override state: DialogState = { chosen: this.props.initial ?? 0 };

  protected override keysHint(): string {
    return (this.props.buttons ?? []).length === 0 ? "Escape closes" : "Left Right or Tab choose, Enter answers, Escape";
  }

  override componentDidMount(): void {
    if (this.props.childFirst !== true) {
      this.focus();
    }
  }

  get #inset(): number {
    return this.props.inset ?? INSET;
  }

  get #buttons(): readonly string[] {
    return this.props.buttons ?? [];
  }

  #message(width: number): string[] {
    return this.props.message === undefined ? [] : wrap(this.props.message, this.#textCells(width));
  }

  #textCells(width: number): number {
    return Math.max(1, width - 2 * this.#inset - BACKGROUND_CELLS - 2);
  }

  #inner(box: Box): Box {
    const lines = this.#message(box.width).length;
    const column = box.column + this.#inset + BACKGROUND_CELLS;
    return { row: box.row + TITLE_ROWS + lines, column, width: Math.max(0, box.width - 2 * this.#inset - BACKGROUND_CELLS - 1), height: Math.max(0, box.height - TITLE_ROWS - lines - FOOT_ROWS) };
  }

  override measure(space: Size, children: readonly LayoutChild[]): Size {
    const inner = this.#inner({ row: 0, column: 0, ...space });
    const content = stackSize({ width: inner.width, height: inner.height }, children).height;
    const foot = this.#buttons.length === 0 ? 1 : FOOT_ROWS;
    return { width: space.width, height: Math.min(space.height, TITLE_ROWS + this.#message(space.width).length + content + foot) };
  }

  override arrange(box: Box, children: readonly LayoutChild[]): void {
    stack(this.#inner(box), children);
  }

  /** Each button's first cell, from the dialog's left. */
  #buttonStarts(): number[] {
    let column = this.#inset + BACKGROUND_CELLS;
    return this.#buttons.map((label) => {
      const at = column;
      column += cells(label) + BUTTON_CELLS + 1;
      return at;
    });
  }

  override paint(canvas: Canvas): void {
    const inset = this.#inset;
    const look = KINDS[this.props.kind ?? "info"];
    const right = canvas.width - inset - 1;
    for (let row = 0; row < canvas.height; row += 1) {
      canvas.codes(row, 0, new Array<number>(canvas.width).fill(SPACE));
      canvas.codes(row, inset, [textColour(row === 0 ? look.background : BACKGROUND), Attribute.NewBackground]);
      canvas.code(row, right, Attribute.BlackBackground);
    }
    canvas.coloured(0, inset + BACKGROUND_CELLS, look.foreground, fit(this.props.title, right - inset - BACKGROUND_CELLS - 1));
    this.#message(canvas.width).forEach((line, index) => canvas.coloured(TITLE_ROWS + index, inset + BACKGROUND_CELLS, Colour.White, line));
    if (this.#buttons.length === 0) {
      return;
    }
    const row = canvas.height - 2;
    this.#buttonStarts().forEach((start, index) => {
      const chosen = index === this.state.chosen && this.focused;
      const background = chosen ? CHOSEN.background : Colour.White;
      const label = this.#buttons[index] ?? "";
      canvas.codes(row, start, [textColour(background), Attribute.NewBackground, textColour(chosen ? CHOSEN.foreground : Colour.Black), ...englishCodes(label), textColour(BACKGROUND), Attribute.NewBackground]);
      // Its band: from its New Background to the cell before the dialog's background comes back.
      canvas.control(row, start + NEW_BACKGROUND_CELL, cells(label) + BUTTON_BAND_CELLS, 1, `${BUTTON_REGION}${index}`);
    });
  }

  #answer(button: number | null, session: number): void {
    if (button === null) {
      this.cancel(session);
    } else {
      this.activate(this.#buttons[button] ?? "", session);
    }
    this.props.onAnswer?.(button, session);
  }

  override onKey(event: KeyEvent): boolean {
    const count = this.#buttons.length;
    if (isPress(event, Key.Escape)) {
      this.#answer(null, event.session);
      return true;
    }
    if (count === 0) {
      return true;
    }
    if (isPress(event, Key.Enter)) {
      this.#answer(this.state.chosen, event.session);
      return true;
    }
    const step = isPress(event, Key.Right) || isPress(event, Key.Tab) ? 1 : isPress(event, Key.Left) ? -1 : 0;
    if (step !== 0) {
      this.focus();
      this.setState({ chosen: (this.state.chosen + step + count) % count });
    }
    // A modal keeps every key: none reaches the page under it.
    return true;
  }

  /** A click on a button's band answers with it; anywhere else on the dialog, nothing. */
  override onClick(event: PointerEvent): boolean {
    if (event.region?.startsWith(BUTTON_REGION) === true) {
      this.#answer(Number(event.region.slice(BUTTON_REGION.length)), event.session);
    }
    return true;
  }
}

/** What lies between a dialog and the page under it: it takes every click and turn of the wheel there, so none reaches the page while the dialog is up, and the page's controls under it are not declared. */
class Shield extends Component {
  /** No rows of its own: the layers give it the modal's whole box, whatever it measures. */
  override measure(space: Size): Size {
    return { width: space.width, height: 0 };
  }

  override onPointer(_event: PointerEvent): boolean {
    return true;
  }

  override onWheel(_event: WheelEvent): boolean {
    return true;
  }
}

export interface ModalProps {
  /** The dialog over the page, or null while there is none. */
  readonly dialog: Element | null;
  /** The row its top is on, in the content's box. */
  readonly row?: number;
}

/** A page with a dialog over it: the children are the page. */
export class Modal extends Component<ModalProps> {
  override render(): Child {
    const { dialog } = this.props;
    return h(
      Layers,
      {},
      h(Rows, { key: "content" }, ...(this.props.children ?? [])),
      dialog === null ? null : h(Shield, { key: "shield" }),
      dialog === null ? null : h(At, { key: "dialog", row: this.props.row ?? 2, column: 0 }, dialog),
    );
  }
}
