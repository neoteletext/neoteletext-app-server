// A line to type on, which the receiver echoes into (INPUT.md section 7.1):
// the cursor stands after what is typed, and only while the field has the
// keys, since a page has one cursor. It may have a label, a placeholder
// while it is empty, its characters masked (the receiver echoes `*`), a
// check whose complaint shows under it, and it may be disabled. Enter sends
// submit up the tree; Escape clears, or sends cancel when it is clear.

import type { Canvas } from "../core/canvas.ts";
import type { Size } from "../core/component.ts";
import { isPress, type KeyEvent, type TextEvent } from "../core/events.ts";
import { DISABLED, EMPTY } from "../core/look.ts";
import type { PageNeeds } from "../core/needs.ts";
import { cells, fit } from "../core/text.ts";
import { Colour, CursorFlag, Key } from "../kit.ts";
import { type ValueProps, ValueControl } from "./control.ts";

/** The prompt before what is typed. */
const PROMPT = "> ";
/** What a masked field shows for each character. */
const MASK = "*";
/** The cells a background takes before the prompt: its colour and New Background. */
const BACKGROUND_CELLS = 2;

export interface TextFieldProps extends ValueProps<string> {
  /** The most characters it takes. */
  readonly longest: number;
  readonly onSubmit?: (text: string, session: number) => void;
  readonly background?: Colour;
  readonly placeholder?: string;
  readonly masked?: boolean;
  /** Enter leaves the text there rather than clearing it. */
  readonly keep?: boolean;
}

export class TextField extends ValueControl<string, TextFieldProps> {
  protected empty(): string {
    return "";
  }

  /** A page with a field to type in takes the digits, or typing a number would choose a page. */
  protected override keyNeeds(): PageNeeds {
    return { digits: true };
  }

  protected override keysHint(): string {
    return "Type, then Enter; Escape clears";
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.props.validate === undefined ? 1 : 2, space.height) };
  }

  override paint(canvas: Canvas): void {
    const field = this.paintLabel(canvas);
    canvas.background(0, field, this.disabled ? Colour.Black : (this.props.background ?? Colour.Blue));
    const room = canvas.width - field - BACKGROUND_CELLS - cells(PROMPT) - 2;
    const typed = this.props.masked === true ? MASK.repeat(this.value.length) : this.value;
    const shown = fit(typed, room);
    const at = field + BACKGROUND_CELLS;
    if (this.disabled) {
      canvas.coloured(0, at, DISABLED, fit(`${PROMPT}${shown === "" ? "(disabled)" : shown}`, room + cells(PROMPT)));
      return;
    }
    canvas.coloured(0, at, Colour.Yellow, `${PROMPT}${shown}`);
    const cursorAt = at + 1 + cells(PROMPT) + cells(shown);
    if (shown === "" && this.props.placeholder !== undefined && !this.focused) {
      canvas.coloured(0, cursorAt - 1, EMPTY, fit(this.props.placeholder, room));
    }
    if (this.focused) {
      canvas.cursor(0, cursorAt, Math.max(0, canvas.width - cursorAt - 1), CursorFlag.Echo | (this.props.masked === true ? CursorFlag.Masked : 0));
    }
    const complaint = this.value === "" ? null : this.complaint;
    if (complaint !== null) {
      canvas.coloured(1, field, Colour.Red, fit(`! ${complaint}`, canvas.width - field - 1));
    }
  }

  override onText(event: TextEvent): boolean {
    if (this.disabled) {
      return false;
    }
    this.change(`${this.value}${event.text}`.slice(0, this.props.longest), event.session);
    return true;
  }

  override onKey(event: KeyEvent): boolean {
    if (this.disabled) {
      return false;
    }
    if (isPress(event, Key.Backspace)) {
      this.change(this.value.slice(0, -1), event.session);
      return true;
    }
    if (isPress(event, Key.Escape)) {
      if (this.value === "") {
        this.cancel(event.session);
        return false;
      }
      this.change("", event.session);
      return true;
    }
    if (isPress(event, Key.Enter) && this.value.trim() !== "" && this.complaint === null) {
      const text = this.value.trim();
      this.emit({ type: "submit", value: text, session: event.session, origin: "viewer" });
      this.play("choose", event.session);
      this.props.onSubmit?.(text, event.session);
      if (this.props.keep !== true) {
        this.change("", event.session);
      }
      return this.props.onSubmit !== undefined;
    }
    return false;
  }
}
