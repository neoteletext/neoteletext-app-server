// The cave flyer: an endless cave scrolls past, narrowing and speeding up
// as it goes; climb and dive to stay off its walls and its rocks, and
// pick up the gold (crates/teletext-showcase/src/games/cave.rs).
//
// The cave is a column of rock above and below for each sixel across.
// The rock is coloured by its row of cells, in strata: colour changes
// between rows of cells cost nothing (EN 300 706 clause 12.2 sets colour
// along a row, never down a column).

import { Colour, SIXELS_DOWN, type Screen } from "../kit.ts";
import { Effect } from "../sound.ts";
import type { Canvas } from "./canvas.ts";
import * as look from "./look.ts";
import { FIELD_HEIGHT, FIELD_SIXELS_ACROSS, FIELD_WIDTH } from "./look.ts";
import { Control, type Dice, Game, type Keys, between } from "./pages.ts";
import { Sprite, burst } from "./sprites.ts";

/** The ship's column, and how fast it climbs or dives in sixels a tick. */
const SHIP_X = 10;
const CLIMB = 2;
/** The cave's opening at the start and at its narrowest, in sixels; it narrows a sixel every 60 columns. */
const START_GAP = 36;
const NARROWEST_GAP = 15;
const COLUMNS_PER_NARROWING = 60;
/** Rock above and below is never thinner than this. */
const MINIMUM_ROCK = 1;
/** The cave's middle wanders: a new direction one column in this many. */
const WANDER_ONE_IN = 6;
/** Scrolling, in tenths of a column a tick: one column a tick at first, a tenth more every 150 columns, up to two. */
const TENTHS = 10;
const START_SPEED = 10;
const COLUMNS_PER_SPEEDUP = 150;
const TOP_SPEED = 20;
/** Rocks and gold, which appear in one new column in this many, rocks only once the first 100 columns have given the player a start. */
const ROCK_ONE_IN = 35;
const ROCKS_AFTER = 100;
const GOLD_ONE_IN = 25;
const MAX_THINGS = 4;
/** The ship's height, and the size of the rocks and gold (the sprites below). */
const SHIP_HEIGHT = 3;
const THING_WIDTH = 4;
const THING_HEIGHT = 3;
/** Rocks leave the ship's height and a sixel clear of the rock above and below, so there is always a way past; gold sits two sixels clear. */
const ROCK_CLEARANCE = SHIP_HEIGHT + 1;
const GOLD_CLEARANCE = 2;
/** Gold is worth this; each column flown is worth one. */
const GOLD_POINTS = 25;
/** Banners go on these rows, away from the ship. */
const LOW_BANNER_ROW = 15;
const HIGH_BANNER_ROW = 5;
/** The strata, by row of cells from the top. */
const STRATA: readonly Colour[] = [Colour.Red, Colour.Magenta, Colour.Red, Colour.Yellow];
/** The rows above the ship where its explosion shows. */
const BURST_ABOVE_SHIP = 1;

function ship(): Sprite {
  return Sprite.drawn("Ship", Colour.Cyan, ["#......", "####...", "#######"]);
}

function rock(): Sprite {
  return Sprite.drawn("Rock", Colour.White, [".##.", "####", ".##."]);
}

function gold(): Sprite {
  return Sprite.drawn("Gold", Colour.Yellow, [".#.", "###", ".#."]);
}

/** A rock or gold, at its top-left. */
type Thing = readonly [number, number];

/** A cave flight. */
export class Cave {
  /** For each sixel column, the first row clear of the rock above and the first row of the rock below. */
  readonly #top: number[] = new Array<number>(FIELD_SIXELS_ACROSS).fill(0);
  readonly #bottom: number[] = new Array<number>(FIELD_SIXELS_ACROSS).fill(0);
  #middle = Math.trunc(FIELD_HEIGHT / 2);
  #wander = 0;
  #shipY = this.#middle - 1;
  readonly #rocks: (Thing | null)[] = new Array<Thing | null>(MAX_THINGS).fill(null);
  readonly #golds: (Thing | null)[] = new Array<Thing | null>(MAX_THINGS).fill(null);
  #distance = 0;
  #goldTaken = 0;
  #owed = 0;
  #over = false;

  /** A new flight at the cave's wide mouth. */
  constructor(dice: Dice) {
    for (let column = 0; column < FIELD_SIXELS_ACROSS; column += 1) {
      const [top, bottom] = this.#nextColumn(dice);
      this.#top[column] = top;
      this.#bottom[column] = bottom;
    }
  }

  /** The score: a point a column, and the gold. */
  score(): number {
    return this.#distance + this.#goldTaken * GOLD_POINTS;
  }

  /** Whether the flight is over. */
  over(): boolean {
    return this.#over;
  }

  /** Where a banner goes: away from the ship. */
  bannerRow(): number {
    return this.#shipY < Math.trunc(FIELD_HEIGHT / 2) ? LOW_BANNER_ROW : HIGH_BANNER_ROW;
  }

  /** The ship's top row. */
  shipY(): number {
    return this.#shipY;
  }

  #gap(): number {
    return Math.max(START_GAP - Math.floor(this.#distance / COLUMNS_PER_NARROWING), NARROWEST_GAP);
  }

  #speed(): number {
    return Math.min(START_SPEED + Math.floor(this.#distance / COLUMNS_PER_SPEEDUP), TOP_SPEED);
  }

  /** The next column of the cave: its middle wanders, kept where the opening fits. */
  #nextColumn(dice: Dice): readonly [number, number] {
    if (dice.below(WANDER_ONE_IN) === 0) {
      this.#wander = between(dice, -1, 1);
    }
    const gap = this.#gap();
    const half = Math.trunc(gap / 2);
    const lowest = MINIMUM_ROCK + half;
    const highest = FIELD_HEIGHT - MINIMUM_ROCK - (gap - half);
    this.#middle = Math.min(Math.max(this.#middle + this.#wander, lowest), highest);
    if (this.#middle === lowest || this.#middle === highest) {
      this.#wander = -this.#wander;
    }
    const top = this.#middle - half;
    return [top, top + gap];
  }

  /** Whether the sixel at `x`, `y` is rock. */
  #rockAt(x: number, y: number): boolean {
    const [top, bottom] = [this.#top[x], this.#bottom[x]];
    return x >= 0 && top !== undefined && bottom !== undefined && (y < top || y >= bottom);
  }

  /** One tick: whether the screen changed. */
  tick(keys: Keys, dice: Dice, sounds: Effect[]): boolean {
    if (this.#over) {
      return false;
    }
    if (keys.active(Control.Up)) {
      this.#shipY = Math.max(this.#shipY - CLIMB, 0);
    }
    if (keys.active(Control.Down)) {
      this.#shipY = Math.min(this.#shipY + CLIMB, FIELD_HEIGHT - SHIP_HEIGHT);
    }
    this.#owed += this.#speed();
    while (this.#owed >= TENTHS) {
      this.#owed -= TENTHS;
      this.#scroll(dice);
    }
    this.#collide(sounds);
    return true;
  }

  /** The cave moves a column left, and a new column comes in. */
  #scroll(dice: Dice): void {
    this.#top.copyWithin(0, 1);
    this.#bottom.copyWithin(0, 1);
    const [top, bottom] = this.#nextColumn(dice);
    this.#top[FIELD_SIXELS_ACROSS - 1] = top;
    this.#bottom[FIELD_SIXELS_ACROSS - 1] = bottom;
    this.#distance += 1;
    for (const things of [this.#rocks, this.#golds]) {
      things.forEach((thing, index) => {
        things[index] = thing !== null && thing[0] > -THING_WIDTH ? [thing[0] - 1, thing[1]] : null;
      });
    }
    if (this.#distance > ROCKS_AFTER && dice.below(ROCK_ONE_IN) === 0) {
      place(this.#rocks, [FIELD_WIDTH, between(dice, top + ROCK_CLEARANCE, bottom - ROCK_CLEARANCE - THING_HEIGHT)]);
    }
    if (dice.below(GOLD_ONE_IN) === 0) {
      place(this.#golds, [FIELD_WIDTH, between(dice, top + GOLD_CLEARANCE, bottom - GOLD_CLEARANCE - THING_HEIGHT)]);
    }
  }

  /** The ship against the rock and the gold. */
  #collide(sounds: Effect[]): void {
    const flyer = ship();
    const lit: Thing[] = [];
    for (let y = 0; y < flyer.height; y += 1) {
      for (let x = 0; x < flyer.width; x += 1) {
        if (flyer.lit(x, y)) {
          lit.push([SHIP_X + x, this.#shipY + y]);
        }
      }
    }
    const touches = (thing: Thing | null, sprite: Sprite): boolean =>
      thing !== null && lit.some(([x, y]) => sprite.lit(x - thing[0], y - thing[1]));
    const boulder = rock();
    const crashed = lit.some(([x, y]) => this.#rockAt(x, y)) || this.#rocks.some((thing) => touches(thing, boulder));
    const nugget = gold();
    this.#golds.forEach((thing, index) => {
      if (touches(thing, nugget)) {
        this.#golds[index] = null;
        this.#goldTaken += 1;
        sounds.push(Effect.Coin);
      }
    });
    if (crashed) {
      this.#over = true;
      sounds.push(Effect.Explosion, Effect.Lose);
    }
  }

  /** Draws the flight. */
  draw(screen: Screen): void {
    look.status(screen, Game.Cave, [
      [Colour.Yellow, `SCORE ${look.digits(this.score())}`],
      [Colour.White, `GOLD ${this.#goldTaken}`],
      [Colour.Cyan, `SPEED ${this.#speed() - START_SPEED + 1}`],
    ]);
    const field = look.field();
    strata(field, this.#top, this.#bottom);
    for (const thing of this.#rocks) {
      if (thing !== null) {
        field.sprite(rock(), thing[0], thing[1], Colour.White);
      }
    }
    for (const thing of this.#golds) {
      if (thing !== null) {
        field.sprite(gold(), thing[0], thing[1], Colour.Yellow);
      }
    }
    field.sprite(ship(), SHIP_X, this.#shipY, Colour.Cyan);
    if (this.#over) {
      field.sprite(burst(), SHIP_X, this.#shipY - BURST_ABOVE_SHIP, Colour.Yellow);
    }
    field.draw(screen, look.FIELD_ROW, look.FIELD_COLUMN);
    look.hint(screen, "Hold ↑ to climb, Down to dive");
  }
}

/** Puts `thing` in a free slot, if there is one. */
function place(things: (Thing | null)[], thing: Thing): void {
  const slot = things.indexOf(null);
  if (slot !== -1) {
    things[slot] = thing;
  }
}

/** The rock above `top` and from `bottom` down, each sixel coloured by its row of cells. */
function strata(field: Canvas, top: readonly number[], bottom: readonly number[]): void {
  const colour = (y: number): Colour => STRATA[Math.floor(y / SIXELS_DOWN) % STRATA.length] ?? Colour.Red;
  top.forEach((above, x) => {
    const below = bottom[x];
    if (below === undefined) {
      return;
    }
    for (let y = 0; y < above; y += 1) {
      field.plot(x, y, colour(y));
    }
    for (let y = below; y < FIELD_HEIGHT; y += 1) {
      field.plot(x, y, colour(y));
    }
  });
}

/** The title page's picture: the cave with the ship in it. */
export function art(): Canvas {
  const picture = look.art(look.ART_HEIGHT);
  // A cave that rises and falls, as triangle waves of two lengths and six
  // sixels deep, with the ship, gold and a rock along it: the picture's
  // numbers are its layout, chosen by eye.
  const wave = (x: number, length: number, height: number): number => {
    const phase = x % length;
    const half = Math.trunc(length / 2);
    return Math.trunc(((phase < half ? phase : length - phase) * height) / half);
  };
  const top = Array.from({ length: FIELD_SIXELS_ACROSS }, (_, x) => wave(x, 26, 6) + 1);
  const bottom = Array.from({ length: FIELD_SIXELS_ACROSS }, (_, x) => look.ART_HEIGHT - wave(x + 9, 34, 6) - 1);
  strata(picture, top, bottom);
  picture.sprite(ship(), SHIP_X, 10, Colour.Cyan);
  picture.sprite(gold(), 30, 11, Colour.Yellow);
  picture.sprite(rock(), 52, 12, Colour.White);
  return picture;
}
