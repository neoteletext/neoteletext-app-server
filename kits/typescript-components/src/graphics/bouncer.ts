// A sprite that moves: a ball bouncing round its box, a frame a tick.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { Colour, Sixels, SIXELS_ACROSS, SIXELS_DOWN } from "../kit.ts";
import type { Bitmap } from "./mosaic.ts";

/** A ball, 4 by 6 sixels. */
const BALL: Bitmap = [".##.", "####", "####", "####", "####", ".##."];

const BALL_WIDTH = 4;

const BALL_HEIGHT = 6;

export interface BouncerProps {
  /** Milliseconds a frame. */
  readonly everyMs: number;
}

interface BouncerState {
  readonly x: number;
  readonly y: number;
  readonly dx: number;
  readonly dy: number;
  readonly frames: number;
}

/** A ball that bounces round its box, a frame each tick: animation by a timer, sixel by sixel. */
export class Bouncer extends Component<BouncerProps, BouncerState> {
  override state: BouncerState = { x: 2, y: 1, dx: 1, dy: 1, frames: 0 };
  #across = 0;
  #down = 0;

  override measure(space: Size): Size {
    return space;
  }

  override componentDidMount(): void {
    this.setInterval(() => {
      this.setState((state) => {
        let { x, y, dx, dy } = state;
        if (x + dx < 0 || x + dx + BALL_WIDTH > this.#across) {
          dx = -dx;
        }
        if (y + dy < 0 || y + dy + BALL_HEIGHT > this.#down) {
          dy = -dy;
        }
        x += dx;
        y += dy;
        return { x, y, dx, dy, frames: state.frames + 1 };
      });
    }, this.props.everyMs, { whileViewed: true });
  }

  override paint(canvas: Canvas): void {
    this.#across = (canvas.width - 1) * SIXELS_ACROSS;
    this.#down = canvas.height * SIXELS_DOWN;
    const sixels = new Sixels(this.#across, this.#down);
    BALL.forEach((line, y) => [...line].forEach((sixel, x) => sixels.set(this.state.x + x, this.state.y + y, sixel === "#")));
    canvas.sixels(0, 1, Colour.Cyan, sixels);
  }
}
