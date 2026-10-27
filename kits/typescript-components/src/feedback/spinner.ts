// Something is happening: a spinner and what it is doing, moved by a timer
// of its own. Teletext has no spinner glyphs, so the frames are mosaics: a
// sixel going round a cell, a block going to and fro, a cell filling and
// emptying; or dots after the words. Done, it says so and stops.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { FULL_BLOCK, MOSAIC_BASE, SIXEL } from "../core/mosaics.ts";
import { fit } from "../core/text.ts";
import { Colour, mosaicColour } from "../kit.ts";

export type SpinnerStyle = "orbit" | "bounce" | "fill" | "dots";

export interface SpinnerProps {
  readonly label: string;
  readonly style?: SpinnerStyle;
  readonly foreground?: Colour;
  /** Milliseconds a frame. */
  readonly everyMs?: number;
  /** It has finished: "OK" in green, and the timer stops. */
  readonly done?: boolean;
}

interface SpinnerState {
  readonly frame: number;
}

/** The sixels round a cell, clockwise from the top left. */
const ORBIT = [SIXEL.topLeft, SIXEL.topRight, SIXEL.middleRight, SIXEL.bottomRight, SIXEL.bottomLeft, SIXEL.middleLeft].map((sixel) => MOSAIC_BASE | sixel);
/** A cell filling from the bottom, then emptying from the top. */
const FILL = [
  MOSAIC_BASE,
  MOSAIC_BASE | SIXEL.bottomLeft | SIXEL.bottomRight,
  MOSAIC_BASE | SIXEL.bottomLeft | SIXEL.bottomRight | SIXEL.middleLeft | SIXEL.middleRight,
  FULL_BLOCK,
  MOSAIC_BASE | SIXEL.middleLeft | SIXEL.middleRight | SIXEL.bottomLeft | SIXEL.bottomRight,
  MOSAIC_BASE | SIXEL.bottomLeft | SIXEL.bottomRight,
];
/** The cells a bouncing block goes across. */
const BOUNCE_CELLS = 4;
/** The most dots after the words. */
const MOST_DOTS = 3;
/** Milliseconds a frame when not told: four a second, which the server publishes as page updates. */
const FRAME_MS = 250;

export class Spinner extends Component<SpinnerProps, SpinnerState> {
  override state: SpinnerState = { frame: 0 };
  #timer = -1;

  override componentDidMount(): void {
    this.#start();
  }

  override componentDidUpdate(previous: Readonly<SpinnerProps>): void {
    if (previous.done !== this.props.done) {
      this.#start();
    }
  }

  override componentWillUnmount(): void {
    this.clearTimer(this.#timer);
  }

  #start(): void {
    this.clearTimer(this.#timer);
    if (this.props.done !== true) {
      this.#timer = this.setInterval(() => this.setState((state) => ({ frame: state.frame + 1 })), this.props.everyMs ?? FRAME_MS, { whileViewed: true });
    }
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  /** The mosaic cells this frame. */
  #cells(): number[] {
    const { frame } = this.state;
    switch (this.props.style ?? "orbit") {
      case "orbit":
        return [ORBIT[frame % ORBIT.length] ?? MOSAIC_BASE];
      case "fill":
        return [FILL[frame % FILL.length] ?? MOSAIC_BASE];
      case "bounce": {
        const span = 2 * (BOUNCE_CELLS - 1);
        const at = frame % span < BOUNCE_CELLS ? frame % span : span - (frame % span);
        return Array.from({ length: BOUNCE_CELLS }, (_, cell) => (cell === at ? FULL_BLOCK : MOSAIC_BASE));
      }
      case "dots":
        return [];
    }
  }

  override paint(canvas: Canvas): void {
    if (this.props.done === true) {
      canvas.coloured(0, 0, Colour.Green, fit(`OK  ${this.props.label}`, canvas.width - 1));
      return;
    }
    const cells = this.#cells();
    let column = 0;
    if (cells.length > 0) {
      canvas.codes(0, 0, [mosaicColour(this.props.foreground ?? Colour.Cyan), ...cells]);
      column = 1 + cells.length;
    }
    const dots = this.props.style === "dots" ? ".".repeat(this.state.frame % (MOST_DOTS + 1)) : "";
    canvas.coloured(0, column, Colour.White, fit(`${this.props.label}${dots}`, canvas.width - column - 1));
  }
}
