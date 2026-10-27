// A canvas a viewer draws on with the pointer held down. Its surface is
// not a control: the browser's hover look would recolour all of it, and
// its pressed look turn all of it round, for as long as a stroke lasts. A
// stroke is a drag, which is sent with its button held, so drawing needs
// no pointer area; showing where the brush will fall before a press does,
// and `brush` asks for one over the surface.

import type { Canvas } from "../core/canvas.ts";
import type { Size } from "../core/component.ts";
import { isPress, type KeyEvent, type PointerEvent } from "../core/events.ts";
import { digitOf } from "../core/keys.ts";
import { SIXEL } from "../core/mosaics.ts";
import type { PageNeeds } from "../core/needs.ts";
import { Colour, Key, Sixels, SIXELS_ACROSS, SIXELS_DOWN } from "../kit.ts";
import { Control, type ControlProps } from "../input/control.ts";

export interface PaintProps extends ControlProps {
  /** The canvas's size in cells; its colour takes the first column. */
  readonly columns: number;
  readonly rows: number;
  readonly foreground?: Colour;
  /**
   * The cell the brush will paint shows as the pointer passes over the
   * surface, before a press: its middle row of sixels turned round. The
   * surface is then a pointer area (INPUT.md section 7.4), so every move
   * over it is a message to the server; the mark stays where the pointer
   * last was on it, since a move off it is not sent.
   */
  readonly brush?: boolean;
}

interface PaintState {
  readonly foreground: Colour;
  /** Bumped at each change, since the sixels change in place. */
  readonly version: number;
  /** The surface's cell under the pointer while no button is held, for the brush; null for none. */
  readonly brushAt: { readonly row: number; readonly column: number } | null;
}

/** The colours a viewer paints in, chosen with the keys 1 to 7. */
const PAINT_COLOURS = [Colour.Red, Colour.Green, Colour.Yellow, Colour.Blue, Colour.Magenta, Colour.Cyan, Colour.White] as const;
/** The surface's first column: the colour takes the one before. */
const SURFACE_AT = 1;
/** What the brush's mark turns round in its cell: the middle row, which shows on a painted cell and on an empty one. */
const BRUSH_MARK = SIXEL.middleLeft | SIXEL.middleRight;

/** A canvas of sixels drawn on with the pointer held down: every drag arrives (INPUT.md section 6.3). */
export class PaintCanvas extends Control<PaintProps, PaintState> {
  override state: PaintState = { foreground: this.props.foreground ?? Colour.Yellow, version: 0, brushAt: null };
  readonly #sixels = new Sixels((this.props.columns - SURFACE_AT) * SIXELS_ACROSS, this.props.rows * SIXELS_DOWN);

  protected override keyNeeds(): PageNeeds {
    return { digits: true };
  }

  protected override keysHint(): string {
    return "Drag to draw; 1-7 colour; Backspace clears";
  }

  override measure(space: Size): Size {
    return { width: Math.min(space.width, this.props.columns), height: Math.min(space.height, this.props.rows) };
  }

  override paint(canvas: Canvas): void {
    canvas.sixels(0, SURFACE_AT, this.state.foreground, this.#sixels);
    const { brushAt } = this.state;
    if (this.props.brush === true && !this.disabled) {
      // Its own columns: a stack gives it the whole row.
      canvas.pointerArea(0, SURFACE_AT, this.props.columns - SURFACE_AT, this.props.rows);
      if (brushAt !== null) {
        canvas.code(brushAt.row, brushAt.column, this.#sixels.cell((brushAt.column - SURFACE_AT) * SIXELS_ACROSS, brushAt.row * SIXELS_DOWN) ^ BRUSH_MARK);
      }
    }
  }

  /** The surface's cell at `row`, `column` of the box, or null off it: the colour's column, or past the surface. */
  #onSurface(row: number, column: number): { row: number; column: number } | null {
    return row >= 0 && row < this.props.rows && column >= SURFACE_AT && column < this.props.columns ? { row, column } : null;
  }

  /** Sets the sixels under a cell: the whole cell, since a cell is the pointer's grain. */
  #set(row: number, column: number): void {
    for (let y = 0; y < SIXELS_DOWN; y += 1) {
      for (let x = 0; x < SIXELS_ACROSS; x += 1) {
        this.#sixels.set((column - SURFACE_AT) * SIXELS_ACROSS + x, row * SIXELS_DOWN + y, true);
      }
    }
    this.setState({ version: this.state.version + 1 });
  }

  override onPointer(event: PointerEvent): boolean {
    if (this.disabled) {
      return false;
    }
    const cell = this.#onSurface(event.local.row, event.local.column);
    if (event.action === "move" && event.buttons === 0) {
      // A move from the pointer area: only a new cell changes the page.
      if (cell?.row !== this.state.brushAt?.row || cell?.column !== this.state.brushAt?.column) {
        this.setState({ brushAt: cell });
      }
      return true;
    }
    if (cell !== null && (event.action === "press" || event.action === "move")) {
      // The brush's mark goes while it paints: the cell under it is painted.
      this.setState({ brushAt: null });
      this.#set(cell.row, cell.column);
    }
    return true;
  }

  override onKey(event: KeyEvent): boolean {
    if (this.disabled) {
      return false;
    }
    const digit = digitOf(event);
    if (digit !== null && digit >= 1 && digit <= PAINT_COLOURS.length) {
      this.setState({ foreground: PAINT_COLOURS[digit - 1] ?? Colour.White });
      return true;
    }
    if (isPress(event, Key.Backspace) || isPress(event, "c")) {
      this.#sixels.clear();
      this.setState({ version: this.state.version + 1 });
      return true;
    }
    return false;
  }
}
