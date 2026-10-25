// Snake: steer with the arrows, eat to grow, and never bite yourself
// (crates/teletext-showcase/src/games/snake.rs).
//
// The snake lives on a grid of two-by-two sixel squares, which join into
// a solid body in contiguous mosaics. A blank column of cells keeps the
// grid off the side walls, so the snake and the food keep their own
// colours there (see canvas.ts).

import { Colour, type Screen } from "../kit.ts";
import { Effect } from "../sound.ts";
import type { Canvas } from "./canvas.ts";
import * as look from "./look.ts";
import { FIELD_WIDTH } from "./look.ts";
import { Control, type Dice, Game } from "./pages.ts";

/** A grid square's side, in sixels. */
const SQUARE = 2;
/**
 * The grid: 35 squares across from sixel 4, 27 down from sixel 3, inside
 * walls on the cells around it. The walls keep rows and columns of cells
 * of their own, so the snake and its food keep their colours beside them.
 */
const GRID_LEFT = 4;
const GRID_TOP = 3;
const GRID_COLUMNS = 35;
const GRID_ROWS = 27;
/** The walls: a sixel line in the outermost cells. */
const WALL_LEFT = 1;
const WALL_RIGHT = FIELD_WIDTH - 2;
const WALL_TOP = 2;
const WALL_BOTTOM = GRID_TOP + GRID_ROWS * SQUARE;
/** The snake at the start, and how much each meal adds. */
const START_LENGTH = 4;
const GROWTH = 3;
/** Speed in steps a tick, in tenths: five steps a second at first, one tenth faster every three meals, up to a step every tick. */
const TENTHS = 10;
const START_SPEED = 5;
const MEALS_PER_SPEED = 3;
const TOP_SPEED = 10;
/** Every seventh meal brings a bonus, for 60 ticks, worth 50 points. */
const BONUS_EVERY = 7;
const BONUS_TICKS = 60;
const BONUS_POINTS = 50;
/** A meal is worth this, times the speed. */
const MEAL_POINTS = 2;
/** Turns pressed faster than the snake steps wait their turn, up to two. */
const TURNS_QUEUED = 2;

const Heading = { Up: "up", Down: "down", Left: "left", Right: "right" } as const;
type Heading = (typeof Heading)[keyof typeof Heading];

/** Each heading's step across and down, and its opposite. */
const HEADINGS: Readonly<Record<Heading, { readonly step: readonly [number, number]; readonly opposite: Heading }>> = {
  up: { step: [0, -1], opposite: Heading.Down },
  down: { step: [0, 1], opposite: Heading.Up },
  left: { step: [-1, 0], opposite: Heading.Right },
  right: { step: [1, 0], opposite: Heading.Left },
};

/** The heading an arrow's control turns to. */
function headingOf(control: number): Heading | undefined {
  switch (control) {
    case Control.Up:
      return Heading.Up;
    case Control.Down:
      return Heading.Down;
    case Control.Left:
      return Heading.Left;
    case Control.Right:
      return Heading.Right;
    default:
      return undefined;
  }
}

/** A grid square: its column and row. */
export type Square = readonly [number, number];
/** Off the grid: the food's place before it is first put down. */
const NOWHERE: Square = [-1, -1];

function same(left: Square, right: Square): boolean {
  return left[0] === right[0] && left[1] === right[1];
}

/** A game of snake. */
export class Snake {
  /** Head first. */
  readonly #body: Square[];
  #heading: Heading = Heading.Right;
  readonly #turns: Heading[] = [];
  #growing = 0;
  #food: Square = NOWHERE;
  #bonus: { readonly square: Square; readonly ticks: number } | null = null;
  #meals = 0;
  #score = 0;
  /** Steps owed, in tenths. */
  #owed = 0;
  #over = false;

  /** A new game: a short snake in the middle, heading right. */
  constructor(dice: Dice) {
    const [middleX, middleY] = [Math.trunc(GRID_COLUMNS / 2), Math.trunc(GRID_ROWS / 2)];
    this.#body = Array.from({ length: START_LENGTH }, (_, back): Square => [middleX - back, middleY]);
    this.#food = this.#freeSquare(dice);
  }

  /** The score. */
  score(): number {
    return this.#score;
  }

  /** Whether the game is over. */
  over(): boolean {
    return this.#over;
  }

  /** The snake's length. */
  length(): number {
    return this.#body.length;
  }

  /** The head's square. */
  head(): Square {
    return this.#body[0] ?? [0, 0];
  }

  #speed(): number {
    return Math.min(START_SPEED + Math.floor(this.#meals / MEALS_PER_SPEED), TOP_SPEED);
  }

  /** An arrow was pressed: the snake turns at its next step, or after the turn already waiting; never straight back on itself. */
  turn(control: number): void {
    const heading = headingOf(control);
    if (heading === undefined) {
      return;
    }
    const last = this.#turns[this.#turns.length - 1] ?? this.#heading;
    if (heading !== last && heading !== HEADINGS[last].opposite && this.#turns.length < TURNS_QUEUED) {
      this.#turns.push(heading);
    }
  }

  /** A square no part of the snake or food is on. */
  #freeSquare(dice: Dice): Square {
    const squares = GRID_COLUMNS * GRID_ROWS;
    const start = dice.below(squares);
    // From a random square, the first free one: always ends, as the snake
    // can never fill the grid before it bites itself.
    for (let offset = 0; offset < squares; offset += 1) {
      const index = (start + offset) % squares;
      const square: Square = [index % GRID_COLUMNS, Math.floor(index / GRID_COLUMNS)];
      const taken =
        this.#body.some((part) => same(part, square)) || same(square, this.#food) || (this.#bonus !== null && same(this.#bonus.square, square));
      if (!taken) {
        return square;
      }
    }
    return [0, 0];
  }

  /** One tick: whether the screen changed. */
  tick(dice: Dice, sounds: Effect[]): boolean {
    if (this.#over) {
      return false;
    }
    let changed = false;
    if (this.#bonus !== null) {
      const { square, ticks } = this.#bonus;
      this.#bonus = ticks > 1 ? { square, ticks: ticks - 1 } : null;
      changed ||= this.#bonus === null;
    }
    this.#owed += this.#speed();
    while (this.#owed >= TENTHS && !this.#over) {
      this.#owed -= TENTHS;
      this.#step(dice, sounds);
      changed = true;
    }
    return changed;
  }

  #step(dice: Dice, sounds: Effect[]): void {
    const turn = this.#turns.shift();
    if (turn !== undefined) {
      this.#heading = turn;
    }
    const [dx, dy] = HEADINGS[this.#heading].step;
    const front = this.#body[0];
    if (front === undefined) {
      return;
    }
    const head: Square = [front[0] + dx, front[1] + dy];
    // The tail moves on as the head moves, unless the snake is growing:
    // the head may take the square the tail leaves.
    const tailMoves = this.#growing === 0;
    const body = this.#body.length - (tailMoves ? 1 : 0);
    const outside = head[0] < 0 || head[0] >= GRID_COLUMNS || head[1] < 0 || head[1] >= GRID_ROWS;
    if (outside || this.#body.slice(0, body).some((square) => same(square, head))) {
      this.#over = true;
      sounds.push(Effect.Explosion, Effect.Lose);
      return;
    }
    if (tailMoves) {
      this.#body.pop();
    } else {
      this.#growing -= 1;
    }
    this.#body.unshift(head);
    if (same(head, this.#food)) {
      this.#meals += 1;
      this.#growing += GROWTH;
      this.#score += MEAL_POINTS * this.#speed();
      this.#food = this.#freeSquare(dice);
      sounds.push(Effect.Coin);
      if (this.#meals % BONUS_EVERY === 0) {
        this.#bonus = { square: this.#freeSquare(dice), ticks: BONUS_TICKS };
      }
    } else if (this.#bonus !== null && same(this.#bonus.square, head)) {
      this.#bonus = null;
      this.#score += BONUS_POINTS;
      sounds.push(Effect.Jump);
    }
  }

  /** Draws the game. */
  draw(screen: Screen): void {
    look.status(screen, Game.Snake, [
      [Colour.Yellow, `SCORE ${look.digits(this.#score)}`],
      [Colour.White, `LENGTH ${this.length()}`],
      [Colour.Cyan, `SPEED ${this.#speed() - START_SPEED + 1}`],
    ]);
    const field = look.field();
    walls(field);
    for (const square of this.#body) {
      fill(field, square, Colour.Green);
    }
    fill(field, this.#food, Colour.Red);
    if (this.#bonus !== null) {
      fill(field, this.#bonus.square, Colour.Yellow);
    }
    field.draw(screen, look.FIELD_ROW, look.FIELD_COLUMN);
    look.hint(screen, "Arrows steer   Esc menu");
  }
}

/** The walls round the grid. */
function walls(field: Canvas): void {
  const height = WALL_BOTTOM - WALL_TOP + 1;
  const width = WALL_RIGHT - WALL_LEFT + 1;
  field.rectangle(WALL_LEFT, WALL_TOP, width, 1, Colour.Blue);
  field.rectangle(WALL_LEFT, WALL_BOTTOM, width, 1, Colour.Blue);
  field.rectangle(WALL_LEFT, WALL_TOP, 1, height, Colour.Blue);
  field.rectangle(WALL_RIGHT, WALL_TOP, 1, height, Colour.Blue);
}

/** Lights a grid square. */
function fill(field: Canvas, [column, row]: Square, colour: Colour): void {
  field.rectangle(GRID_LEFT + column * SQUARE, GRID_TOP + row * SQUARE, SQUARE, SQUARE, colour);
}

/** The title page's picture: a snake winding towards its food. */
export function art(): Canvas {
  const picture = look.art(look.ART_HEIGHT);
  // A wave of squares, and the food ahead of it: the picture's layout,
  // chosen by eye.
  const path: readonly Square[] = [
    [4, 8],
    [5, 8],
    [6, 8],
    [7, 8],
    [8, 8],
    [8, 7],
    [8, 6],
    [8, 5],
    [9, 5],
    [10, 5],
    [11, 5],
    [12, 5],
    [12, 6],
    [12, 7],
    [12, 8],
    [13, 8],
    [14, 8],
    [15, 8],
    [16, 8],
    [16, 7],
    [16, 6],
    [16, 5],
    [17, 5],
    [18, 5],
    [19, 5],
    [20, 5],
    [20, 6],
    [20, 7],
    [20, 8],
    [21, 8],
    [22, 8],
    [23, 8],
  ];
  for (const [column, row] of path) {
    picture.rectangle(column * SQUARE + GRID_LEFT, row * SQUARE, SQUARE, SQUARE, Colour.Green);
  }
  const food: Square = [27, 8];
  picture.rectangle(food[0] * SQUARE + GRID_LEFT, food[1] * SQUARE, SQUARE, SQUARE, Colour.Red);
  return picture;
}
