// What the games' files share (crates/teletext-showcase/src/games.rs):
// the pages, the games, the controls a game reads, and the randomness a
// game is given.

import { Key } from "../kit.ts";

/** The games' menu. */
export const MENU = 0x300;
/** Space invaders. */
export const INVADERS = 0x301;
/** Snake. */
export const SNAKE = 0x302;
/** Breakout. */
export const BREAKOUT = 0x303;
/** The cave flyer. */
export const CAVE = 0x304;
/** The hall of fame, shared by every viewer. */
export const HALL = 0x305;

/** A game. */
export const Game = {
  /** Space invaders with the sci-fi sprites. */
  Invaders: "invaders",
  /** Snake. */
  Snake: "snake",
  /** Breakout. */
  Breakout: "breakout",
  /** The cave flyer. */
  Cave: "cave",
} as const;
/** A game. */
export type Game = (typeof Game)[keyof typeof Game];

/** Every game, in page order. */
export const GAMES: readonly Game[] = [Game.Invaders, Game.Snake, Game.Breakout, Game.Cave];

/** Each game's page, name and bit in a set of games. */
const ABOUT: Readonly<Record<Game, { readonly page: number; readonly name: string; readonly bit: number }>> = {
  invaders: { page: INVADERS, name: "Invaders", bit: 1 },
  snake: { page: SNAKE, name: "Snake", bit: 2 },
  breakout: { page: BREAKOUT, name: "Breakout", bit: 4 },
  cave: { page: CAVE, name: "Cave flyer", bit: 8 },
};

/** The page of `game`. */
export function pageOf(game: Game): number {
  return ABOUT[game].page;
}

/** The game on `page`, if one is. */
export function gameOn(page: number): Game | undefined {
  return GAMES.find((game) => pageOf(game) === page);
}

/** The name of `game`. */
export function nameOf(game: Game): string {
  return ABOUT[game].name;
}

/** The bit of `game` in a set of games. */
export function bitOf(game: Game): number {
  return ABOUT[game].bit;
}

/** The controls a game reads, as bits. */
export const Control = {
  Left: 1,
  Right: 2,
  Up: 4,
  Down: 8,
  /** Fire, launch or thrust. */
  Fire: 16,
} as const;

/** The key that fires as a character: Space. */
const SPACE_KEY = " ";

/** The control a key works, if any: the arrows, and Space or Enter to fire. */
export function controlOf(key: string): number | undefined {
  switch (key) {
    case Key.Left:
      return Control.Left;
    case Key.Right:
      return Control.Right;
    case Key.Up:
      return Control.Up;
    case Key.Down:
      return Control.Down;
    case Key.Enter:
    case SPACE_KEY:
      return Control.Fire;
    default:
      return undefined;
  }
}

/** The controls held, and those pressed since the last tick: a tap shorter than a tick still moves a step. */
export class Keys {
  #held = 0;
  #tapped = 0;

  /** `control` went down. */
  press(control: number): void {
    this.#held |= control;
    this.#tapped |= control;
  }

  /** `control` went down and up again before the next tick. */
  tap(control: number): void {
    this.#tapped |= control;
  }

  /** `control` came up. */
  release(control: number): void {
    this.#held &= ~control;
  }

  /** Everything came up: the receiver lost the focus, and will not say when the keys held come up (INPUT.md section 5.4, FOCUS_OUT). */
  releaseAll(): void {
    this.#held = 0;
  }

  /** Whether `control` is held or was pressed since the last tick. */
  active(control: number): boolean {
    return ((this.#held | this.#tapped) & control) !== 0;
  }

  /** Whether `control` was pressed since the last tick. */
  tapped(control: number): boolean {
    return (this.#tapped & control) !== 0;
  }

  /** A tick has read the taps. */
  endTick(): void {
    this.#tapped = 0;
  }
}

/** Randomness for the games: the channel's (a Context is one), or a test's. */
export interface Dice {
  /** A number below `bound`; 0 when `bound` is 0. */
  below(bound: number): number;
}

/** A number from `low` to `high`, both included. */
export function between(dice: Dice, low: number, high: number): number {
  // An empty range counts as one number, as the Rust's failed u32
  // conversion did.
  const span = high - low + 1;
  return low + dice.below(span >= 0 ? span : 1);
}
