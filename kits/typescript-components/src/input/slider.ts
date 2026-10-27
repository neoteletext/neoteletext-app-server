// A slider: a value in a range, set by a knob on a track of mosaic sixels,
// two to a cell, so the knob moves half a cell at a time. Left and Right
// step, Page Up and Page Down step ten times, Home and End go to the ends,
// and the pointer sets it where the track is pressed or dragged. The track
// is its control, since a press anywhere on it moves the knob there, and
// the browser holds its pressed look while the knob is dragged; a drag is
// sent with its button held, so it needs no pointer area. Filled, the track
// is thicker up to the knob.

import type { Canvas } from "../core/canvas.ts";
import type { Size } from "../core/component.ts";
import { isPress, type KeyEvent, type PointerEvent } from "../core/events.ts";
import { DISABLED, FOCUSED_LABEL } from "../core/look.ts";
import { BOTTOM_ROW, LEFT_HALF, MIDDLE_ROW, RIGHT_HALF } from "../core/mosaics.ts";
import { padStart } from "../core/text.ts";
import { Colour, Key, SIXELS_ACROSS } from "../kit.ts";
import { type ValueProps, ValueControl } from "./control.ts";

export interface SliderProps extends ValueProps<number> {
  readonly label: string;
  readonly min: number;
  readonly max: number;
  readonly step?: number;
  readonly foreground?: Colour;
  readonly filled?: boolean;
  /** What follows the value: "%", "dB". */
  readonly unit?: string;
}

/** The value's cells after the track, its colour cell included. */
const VALUE_CELLS = 6;
/** Page Up and Page Down move this many steps. */
const PAGE_STEPS = 10;
const TRACK = "track";

export class Slider extends ValueControl<number, SliderProps> {
  protected empty(): number {
    return this.props.min;
  }

  #clamp(value: number): number {
    const step = this.props.step ?? 1;
    const stepped = this.props.min + Math.round((value - this.props.min) / step) * step;
    return Math.max(this.props.min, Math.min(this.props.max, stepped));
  }

  protected override keysHint(): string {
    return "Left Right step, PgUp PgDn more, drag the knob";
  }

  /** The track's cells, after its mosaic colour's cell. */
  #trackCells(width: number): number {
    return Math.max(1, width - this.labelCells - 1 - VALUE_CELLS - (this.props.unit?.length ?? 0));
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const at = this.paintLabel(canvas);
    const track = this.#trackCells(canvas.width);
    const sixels = track * SIXELS_ACROSS;
    const range = this.props.max - this.props.min;
    const knob = range <= 0 ? 0 : Math.round(((this.value - this.props.min) / range) * (sixels - 1));
    const codes = Array.from({ length: track }, (_, cell) => {
      if (Math.floor(knob / SIXELS_ACROSS) === cell) {
        return (knob % SIXELS_ACROSS === 0 ? LEFT_HALF : RIGHT_HALF) | MIDDLE_ROW;
      }
      const before = (cell + 1) * SIXELS_ACROSS - 1 < knob;
      return this.props.filled === true && before ? MIDDLE_ROW | BOTTOM_ROW : MIDDLE_ROW;
    });
    const foreground = this.disabled ? DISABLED : this.focused ? FOCUSED_LABEL : (this.props.foreground ?? Colour.Cyan);
    const after = canvas.mosaics(0, at, foreground, codes);
    if (!this.disabled) {
      canvas.control(0, at + 1, track, 1, TRACK);
    }
    canvas.coloured(0, after, this.disabled ? DISABLED : Colour.White, `${padStart(String(this.value), VALUE_CELLS - 2)}${this.props.unit ?? ""}`);
  }

  override onKey(event: KeyEvent): boolean {
    if (this.disabled) {
      return false;
    }
    const step = this.props.step ?? 1;
    const { value } = this;
    const moves: Readonly<Record<string, number>> = {
      [Key.Right]: value + step,
      [Key.Left]: value - step,
      [Key.PageUp]: value + PAGE_STEPS * step,
      [Key.PageDown]: value - PAGE_STEPS * step,
      [Key.Home]: this.props.min,
      [Key.End]: this.props.max,
    };
    const to = event.action === "release" ? undefined : moves[event.key];
    if (to === undefined || !Object.keys(moves).some((key) => isPress(event, key))) {
      return false;
    }
    this.change(this.#clamp(to), event.session);
    return true;
  }

  override onPointer(event: PointerEvent): boolean {
    if (event.action === "release") {
      return true;
    }
    if (this.disabled || (event.action === "move" && event.buttons === 0)) {
      return false;
    }
    const track = this.#trackCells(this.box.width);
    const cell = Math.max(0, Math.min(track - 1, event.local.column - this.labelCells - 1));
    if (event.action === "press" && event.region !== TRACK) {
      return false;
    }
    const fraction = track <= 1 ? 0 : cell / (track - 1);
    this.change(this.#clamp(this.props.min + fraction * (this.props.max - this.props.min)), event.session);
    return true;
  }
}
