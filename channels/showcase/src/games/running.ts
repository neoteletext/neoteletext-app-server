// A game in play: each game's state behind one shape, so the arcade keeps
// a viewer's game whichever it is (the Rust `Running` of
// crates/teletext-showcase/src/games.rs, here in a file of its own so
// the games' module does not import its parts back).

import type { Screen } from "../kit.ts";
import type { Effect } from "../sound.ts";
import { Breakout } from "./breakout.ts";
import { Cave } from "./cave.ts";
import { BANNER_ROW, Invaders } from "./invaders.ts";
import { MIDDLE_ROW } from "./look.ts";
import { type Dice, Game, type Keys } from "./pages.ts";
import { Snake } from "./snake.ts";
import type { Sprite } from "./sprites.ts";

type State =
  | { readonly game: typeof Game.Invaders; readonly play: Invaders }
  | { readonly game: typeof Game.Snake; readonly play: Snake }
  | { readonly game: typeof Game.Breakout; readonly play: Breakout }
  | { readonly game: typeof Game.Cave; readonly play: Cave };

/** A game in play. */
export class Running {
  readonly #state: State;

  constructor(game: Game, dice: Dice) {
    switch (game) {
      case Game.Invaders:
        this.#state = { game, play: new Invaders() };
        break;
      case Game.Snake:
        this.#state = { game, play: new Snake(dice) };
        break;
      case Game.Breakout:
        this.#state = { game, play: new Breakout() };
        break;
      case Game.Cave:
        this.#state = { game, play: new Cave(dice) };
        break;
    }
  }

  game(): Game {
    return this.#state.game;
  }

  /** A control went down, for games that act on the press itself. */
  press(control: number): void {
    if (this.#state.game === Game.Snake) {
      this.#state.play.turn(control);
    }
  }

  /** One tick: whether the screen changed. */
  tick(keys: Keys, dice: Dice, sounds: Effect[]): boolean {
    const state = this.#state;
    switch (state.game) {
      case Game.Invaders:
        return state.play.tick(keys, dice, sounds);
      case Game.Snake:
        return state.play.tick(dice, sounds);
      case Game.Breakout:
        return state.play.tick(keys, sounds);
      case Game.Cave:
        return state.play.tick(keys, dice, sounds);
    }
  }

  draw(screen: Screen, enemies: readonly Sprite[]): void {
    const state = this.#state;
    if (state.game === Game.Invaders) {
      state.play.draw(screen, enemies);
    } else {
      state.play.draw(screen);
    }
  }

  /** Where a banner over the game goes: clear of what matters most. */
  bannerRow(): number {
    const state = this.#state;
    switch (state.game) {
      case Game.Invaders:
        return BANNER_ROW;
      case Game.Snake:
      case Game.Breakout:
        return MIDDLE_ROW;
      case Game.Cave:
        return state.play.bannerRow();
    }
  }

  score(): number {
    return this.#state.play.score();
  }

  over(): boolean {
    return this.#state.play.over();
  }
}
