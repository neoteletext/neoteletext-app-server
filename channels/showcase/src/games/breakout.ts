// Breakout: five walls of coloured bricks, a ball and a paddle held left
// and right (crates/teletext-showcase/src/games/breakout.rs).
//
// Each row of bricks fills one row of cells, so each has its own colour
// (EN 300 706 clause 12.2: one colour attribute at the row's start). The
// ball moves in sixteenths of a sixel, in steps of at most half a sixel,
// so it cannot pass through a brick between frames.

import { Colour, SIXELS_DOWN, type Screen } from "../kit.ts";
import { Effect } from "../sound.ts";
import type { Canvas } from "./canvas.ts";
import * as look from "./look.ts";
import { FIELD_HEIGHT, FIELD_WIDTH } from "./look.ts";
import { Control, Game, type Keys } from "./pages.ts";

/** Bricks: thirteen across in slots of six sixels, five lit and one gap, on five rows of cells, two sixels lit and one gap. */
const BRICK_COLUMNS = 13;
const BRICK_ROWS = 5;
const SLOT_WIDTH = 6;
const BRICK_WIDTH = 5;
const SLOT_HEIGHT = SIXELS_DOWN;
const BRICK_HEIGHT = 2;
/** The wall's first row: the fourth row of cells, leaving room above to bounce the ball behind the wall. */
const BRICK_TOP = 3 * SLOT_HEIGHT;
/** Each row's colour and worth, top row first. */
const ROW_COLOURS: readonly Colour[] = [Colour.Red, Colour.Magenta, Colour.Yellow, Colour.Green, Colour.Cyan];
const ROW_POINTS: readonly number[] = [50, 40, 30, 20, 10];
/** A full row of bricks, a bit each. */
const FULL_ROW = (1 << BRICK_COLUMNS) - 1;
/** The paddle, on the last row of cells. */
const PADDLE_Y = FIELD_HEIGHT - SLOT_HEIGHT;
const PADDLE_WIDTH = 12;
const PADDLE_HEIGHT = 2;
const PADDLE_SPEED = 3;
/** The ball: two sixels square, its place in sixteenths of a sixel. */
const BALL = 2;
const FIXED = 16;
/** The longest step the ball takes, so it meets every brick it crosses. */
const HALF_SIXEL = FIXED / 2;
/** The ball's speed down the screen, in sixteenths of a sixel a tick: eighteen at first, three more each wall, one more each eight paddle hits, forty at most. */
const START_SPEED = 18;
const WALL_SPEEDUP = 3;
const HITS_PER_SPEEDUP = 8;
const TOP_SPEED = 40;
/** How far the ball's direction swings with where it meets the paddle: across speed is the speed times the distance from the paddle's middle over this. */
const SWING = 8;
/** A ball left on the paddle launches itself after this many ticks. */
const AUTO_LAUNCH_TICKS = 40;
/** Lives at the start. */
const LIVES = 3;
/** The bits of a row of bricks. */
const ROW_BITS = 16;

/** `bits` with bit `column` cleared. */
function cleared(bits: number, column: number): number {
  return column >= 0 && column < ROW_BITS ? bits & ~(1 << column) : bits;
}

/** Whether bit `column` of `bits` is set. */
function has(bits: number, column: number): boolean {
  return column >= 0 && column < ROW_BITS && ((bits >> column) & 1) === 1;
}

/** Five full rows of bricks. */
function fullWall(): number[] {
  return new Array<number>(BRICK_ROWS).fill(FULL_ROW);
}

/** The number of bits set in `bits`. */
function countOnes(bits: number): number {
  let count = 0;
  for (let rest = bits; rest !== 0; rest &= rest - 1) {
    count += 1;
  }
  return count;
}

/** A place or a speed: across and down. */
type Pair = readonly [number, number];

/** A game of breakout. */
export class Breakout {
  #bricks = fullWall();
  #paddleX = Math.trunc((FIELD_WIDTH - PADDLE_WIDTH) / 2);
  /** The ball, in sixteenths of a sixel, and its speed. */
  #ball: Pair = [0, 0];
  #velocity: Pair = [0, 0];
  /** Ticks the ball has sat on the paddle; null in flight. */
  #held: number | null = 0;
  #speed = START_SPEED;
  #hits = 0;
  #wall = 0;
  #lives = LIVES;
  #score = 0;
  #over = false;

  /** The score. */
  score(): number {
    return this.#score;
  }

  /** Whether the game is over. */
  over(): boolean {
    return this.#over;
  }

  /** The paddle's left edge. */
  paddleX(): number {
    return this.#paddleX;
  }

  /** Bricks left. */
  bricksLeft(): number {
    return this.#bricks.reduce((sum, row) => sum + countOnes(row), 0);
  }

  /** Whether the ball is in flight. */
  inFlight(): boolean {
    return this.#held === null;
  }

  /** The ball sitting on the paddle's middle. */
  #ballOnPaddle(): Pair {
    const x = this.#paddleX + Math.trunc((PADDLE_WIDTH - BALL) / 2);
    return [x * FIXED, (PADDLE_Y - BALL) * FIXED];
  }

  /** One tick: whether the screen changed. */
  tick(keys: Keys, sounds: Effect[]): boolean {
    if (this.#over) {
      return false;
    }
    const [paddleBefore, ballBefore] = [this.#paddleX, this.#ball];
    if (keys.active(Control.Left)) {
      this.#paddleX = Math.max(this.#paddleX - PADDLE_SPEED, 0);
    }
    if (keys.active(Control.Right)) {
      this.#paddleX = Math.min(this.#paddleX + PADDLE_SPEED, FIELD_WIDTH - PADDLE_WIDTH);
    }
    const waited = this.#held;
    if (waited !== null) {
      this.#ball = this.#ballOnPaddle();
      if (keys.active(Control.Fire) || keys.active(Control.Up) || waited >= AUTO_LAUNCH_TICKS) {
        this.#held = null;
        // Up and away from the nearer wall.
        const leftwards = this.#paddleX * 2 > FIELD_WIDTH - PADDLE_WIDTH;
        const across = Math.trunc(this.#speed / 2);
        this.#velocity = [leftwards ? -across : across, -this.#speed];
        sounds.push(Effect.Click);
      } else {
        this.#held = waited + 1;
      }
      return paddleBefore !== this.#paddleX || ballBefore[0] !== this.#ball[0] || ballBefore[1] !== this.#ball[1];
    }
    this.#fly(sounds);
    return true;
  }

  /** The ball's flight for a tick, in steps of at most half a sixel. */
  #fly(sounds: Effect[]): void {
    const [vx, vy] = this.#velocity;
    const steps = Math.trunc(Math.max(Math.abs(vx), Math.abs(vy)) / HALF_SIXEL) + 1;
    let moved: Pair = [0, 0];
    for (let step = 1; step <= steps; step += 1) {
      // Where this step should have the ball, from the tick's start, so
      // no fraction of the velocity is lost to rounding.
      const target: Pair = [Math.trunc((this.#velocity[0] * step) / steps), Math.trunc((this.#velocity[1] * step) / steps)];
      const [dx, dy] = [target[0] - moved[0], target[1] - moved[1]];
      moved = target;
      const x = this.#ball[0] + dx;
      if (x < 0 || x + BALL * FIXED > FIELD_WIDTH * FIXED || this.#hitBricks(x, this.#ball[1], sounds)) {
        this.#velocity = [-this.#velocity[0], this.#velocity[1]];
      } else {
        this.#ball = [x, this.#ball[1]];
      }
      const y = this.#ball[1] + dy;
      if (y < 0 || this.#hitBricks(this.#ball[0], y, sounds)) {
        this.#velocity = [this.#velocity[0], -this.#velocity[1]];
      } else if (this.#velocity[1] > 0 && this.#meetsPaddle(y)) {
        this.#bounce(sounds);
      } else {
        this.#ball = [this.#ball[0], y];
      }
      if (this.#ball[1] >= FIELD_HEIGHT * FIXED) {
        this.#loseBall(sounds);
        return;
      }
    }
    if (this.#bricks.every((row) => row === 0)) {
      this.#wall += 1;
      this.#bricks = fullWall();
      this.#speed = Math.min(START_SPEED + WALL_SPEEDUP * this.#wall, TOP_SPEED);
      this.#held = 0;
      sounds.push(Effect.Win);
    }
  }

  /** Whether the ball, its top at `y`, lands on the paddle. */
  #meetsPaddle(y: number): boolean {
    const bottom = Math.trunc((y + BALL * FIXED) / FIXED);
    const left = Math.trunc(this.#ball[0] / FIXED);
    return bottom > PADDLE_Y && bottom <= PADDLE_Y + PADDLE_HEIGHT && left + BALL > this.#paddleX && left < this.#paddleX + PADDLE_WIDTH;
  }

  /** Off the paddle: upwards, swung by where it met the paddle. */
  #bounce(sounds: Effect[]): void {
    this.#hits += 1;
    if (this.#hits % HITS_PER_SPEEDUP === 0) {
      this.#speed = Math.min(this.#speed + 1, TOP_SPEED);
    }
    const middle = this.#paddleX + Math.trunc(PADDLE_WIDTH / 2);
    const ballMiddle = Math.trunc(this.#ball[0] / FIXED) + Math.trunc(BALL / 2);
    const offset = ballMiddle - middle;
    this.#velocity = [Math.trunc((this.#speed * offset) / SWING), -this.#speed];
    sounds.push(Effect.Click);
  }

  #loseBall(sounds: Effect[]): void {
    this.#lives = Math.max(0, this.#lives - 1);
    sounds.push(Effect.Explosion);
    if (this.#lives === 0) {
      this.#over = true;
      sounds.push(Effect.Lose);
    } else {
      this.#held = 0;
      this.#ball = this.#ballOnPaddle();
    }
  }

  /** Knocks out every brick the ball would cover at `x`, `y`; whether any. */
  #hitBricks(x: number, y: number, sounds: Effect[]): boolean {
    const [left, top] = [Math.trunc(x / FIXED), Math.trunc(y / FIXED)];
    let hit = false;
    for (let sy = top; sy < top + BALL; sy += 1) {
      for (let sx = left; sx < left + BALL; sx += 1) {
        const brick = brickAt(sx, sy);
        if (brick === undefined) {
          continue;
        }
        const [row, column] = brick;
        const bits = this.#bricks[row];
        if (bits !== undefined && has(bits, column)) {
          this.#bricks[row] = cleared(bits, column);
          this.#score += ROW_POINTS[row] ?? 0;
          hit = true;
        }
      }
    }
    if (hit) {
      sounds.push(Effect.Hit);
    }
    return hit;
  }

  /** Draws the game. */
  draw(screen: Screen): void {
    look.status(screen, Game.Breakout, [
      [Colour.Yellow, `SCORE ${look.digits(this.#score)}`],
      [Colour.Cyan, `WALL ${this.#wall + 1}`],
      [Colour.White, `BALLS ${this.#lives}`],
    ]);
    const field = look.field();
    wallOf(field, this.#bricks, BRICK_TOP);
    field.rectangle(this.#paddleX, PADDLE_Y, PADDLE_WIDTH, PADDLE_HEIGHT, Colour.White);
    const ball = this.#held !== null ? this.#ballOnPaddle() : this.#ball;
    field.rectangle(Math.trunc(ball[0] / FIXED), Math.trunc(ball[1] / FIXED), BALL, BALL, Colour.White);
    field.draw(screen, look.FIELD_ROW, look.FIELD_COLUMN);
    look.hint(screen, this.#held !== null ? "← → move   Space launches   Esc menu" : "Hold ← → to move   Esc menu");
  }
}

/** The brick whose lit part holds the sixel `x`, `y`: its row and column. */
function brickAt(x: number, y: number): readonly [number, number] | undefined {
  const down = y - BRICK_TOP;
  if (down < 0) {
    return undefined;
  }
  const row = Math.trunc(down / SLOT_HEIGHT);
  const column = Math.trunc(x / SLOT_WIDTH);
  if (row >= BRICK_ROWS || column < 0 || column >= BRICK_COLUMNS || x < 0) {
    return undefined;
  }
  const lit = down % SLOT_HEIGHT < BRICK_HEIGHT && x % SLOT_WIDTH < BRICK_WIDTH;
  return lit ? [row, column] : undefined;
}

/** Draws the bricks of `rows` from sixel row `top`. */
function wallOf(field: Canvas, rows: readonly number[], top: number): void {
  rows.slice(0, ROW_COLOURS.length).forEach((bits, row) => {
    const colour = ROW_COLOURS[row] ?? Colour.White;
    const y = top + row * SLOT_HEIGHT;
    for (let column = 0; column < BRICK_COLUMNS; column += 1) {
      if (has(bits, column)) {
        field.rectangle(column * SLOT_WIDTH, y, BRICK_WIDTH, BRICK_HEIGHT, colour);
      }
    }
  });
}

/** The title page's picture: a wall with a gap knocked in it, the ball on its way back, and the paddle. */
export function art(): Canvas {
  const picture = look.art(look.ART_HEIGHT);
  // The picture's layout, chosen by eye: a hole knocked up into the wall,
  // the ball under it, the paddle below.
  const knocked = [FULL_ROW, FULL_ROW & ~0b0000_0110_0000, FULL_ROW & ~0b0000_1111_0000, FULL_ROW & ~0b0001_1111_1000, FULL_ROW & ~0b0011_1111_1100];
  wallOf(picture, knocked, 0);
  picture.rectangle(44, 17, BALL, BALL, Colour.White);
  picture.rectangle(36, 22, PADDLE_WIDTH, 1, Colour.White);
  return picture;
}
