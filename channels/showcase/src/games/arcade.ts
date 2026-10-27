// The application behind pages 300 to 305: the menu, the four games and
// the hall of fame (crates/teletext-showcase/src/games/arcade.rs).

import { type Application, Colour, type Context, type Input, Key, type PageEntry, type Screen, character, pressed, released } from "../kit.ts";
import type { Picture } from "../pictures.ts";
import { Effect, Sounds } from "../sound.ts";
import * as breakout from "./breakout.ts";
import * as cave from "./cave.ts";
import { Hall } from "./hall.ts";
import * as invaders from "./invaders.ts";
import * as look from "./look.ts";
import { Control, GAMES, Game, HALL, Keys, MENU, bitOf, controlOf, gameOn, nameOf, pageOf } from "./pages.ts";
import { Running } from "./running.ts";
import * as snake from "./snake.ts";
import { type Sprite, enemies } from "./sprites.ts";

/**
 * The games' tick, in milliseconds: ten frames a second. Each frame is
 * the viewer's whole page, about a kilobyte, so a game costs its viewer
 * about ten kilobytes a second, and the server one page for each player
 * a tick (decision 7).
 */
export const TICK_MS = 100;
/** After a game ends, keys are ignored this long, so the fire key still held from the game does not start the next at once. */
const RESTART_PAUSE_MS = 1_000;
/** The enemies' portraits on the invaders' title page, in sixels. */
const PORTRAIT_WIDTH = 12;
const PORTRAIT_HEIGHT = 9;
/** The character that starts a game or fires as text: Space. */
const SPACE_CHARACTER = " ";

/** A viewer's game. */
interface Player {
  readonly running: Running;
  readonly keys: Keys;
  /** When the game ended; null while it is in play. */
  endedAt: number | null;
}

/** The menu, the games and the hall of fame. */
export class Arcade implements Application {
  /** The enemies at wave size and at portrait size, one each per sci-fi sprite. */
  readonly #enemies: readonly Sprite[];
  readonly #portraits: readonly Sprite[];
  /** Each viewer's game, while they are on its page. */
  readonly #players = new Map<number, Player>();
  /**
   * The game pages each session has had a page of its own on: going back
   * to one, it is given the title page again, since its own page still
   * overrides the shared one.
   */
  readonly #owned = new Map<number, number>();
  readonly #hall = new Hall();
  readonly #sounds = new Sounds();
  /** Whether a wake is asked for. */
  #ticking = false;

  /** The arcade, its enemies taken from the sci-fi sprites among `pictures`. */
  constructor(pictures: readonly Picture[]) {
    this.#enemies = enemies(pictures, invaders.ENEMY_WIDTH, invaders.ENEMY_HEIGHT);
    this.#portraits = enemies(pictures, PORTRAIT_WIDTH, PORTRAIT_HEIGHT);
  }

  /** The title page of `game`. */
  #title(game: Game): Screen {
    const best = this.#hall.best(game);
    switch (game) {
      case Game.Invaders: {
        const firstWave: Sprite[] = [];
        for (let row = 0; row < invaders.ENEMY_ROWS; row += 1) {
          const portrait = invaders.enemy(this.#portraits, 0, row);
          if (portrait !== undefined) {
            firstWave.push(portrait);
          }
        }
        return look.invadersTitle(firstWave, invaders.POINTS, best);
      }
      case Game.Snake:
        return look.title(
          game,
          snake.art(),
          [
            [Colour.White, "Steer with the arrows. Eat the red"],
            [Colour.White, "food to grow; the snake speeds up."],
            [Colour.Cyan, "Walls and your own tail are fatal."],
          ],
          best,
        );
      case Game.Breakout:
        return look.title(
          game,
          breakout.art(),
          [
            [Colour.White, "Hold ← → to move the paddle."],
            [Colour.White, "Space launches the ball. Clear the"],
            [Colour.Cyan, "wall; each wall comes back faster."],
          ],
          best,
        );
      case Game.Cave:
        return look.title(
          game,
          cave.art(),
          [
            [Colour.White, "Hold ↑ and down to climb and dive."],
            [Colour.White, "The cave narrows as you go deeper."],
            [Colour.Cyan, "Gold diamonds are worth 25 points."],
          ],
          best,
        );
    }
  }

  /** Publishes every shared page: the menu, the titles and the hall. */
  #publishShared(context: Context): void {
    context.publish("channel", MENU, look.menu(this.#enemies));
    for (const game of GAMES) {
      context.publish("channel", pageOf(game), this.#title(game));
    }
    context.publish("channel", HALL, this.#hall.screen());
  }

  /** Publishes `session`'s game. */
  #publishGame(session: number, player: Player, context: Context): void {
    context.publish(session, pageOf(player.running.game()), this.#screenOf(player));
  }

  #screenOf(player: Player): Screen {
    const game = player.running.game();
    const screen = look.playPage(game);
    player.running.draw(screen, this.#enemies);
    if (player.endedAt !== null) {
      const best = this.#hall.best(game)?.[0] === player.running.score();
      const line = best ? "A new best! Space plays again" : "Space plays again, Esc for menu";
      look.banner(screen, player.running.bannerRow(), Colour.Red, "GAME OVER", line);
    }
    return screen;
  }

  /** Starts `game` for `session`. */
  #startGame(session: number, game: Game, context: Context): void {
    const player: Player = { running: new Running(game, context), keys: new Keys(), endedAt: null };
    this.#publishGame(session, player, context);
    this.#players.set(session, player);
    this.#owned.set(session, (this.#owned.get(session) ?? 0) | bitOf(game));
    this.#sounds.play(session, Effect.Coin, context);
    if (!this.#ticking) {
      this.#ticking = true;
      context.wakeIn(TICK_MS);
    }
  }

  /** Ends `session`'s game, if it has one: a game left in play still counts for the hall of fame. */
  #finish(session: number, context: Context): void {
    const player = this.#players.get(session);
    if (player === undefined) {
      return;
    }
    this.#players.delete(session);
    if (player.endedAt === null && this.#hall.record(player.running.game(), session, player.running.score())) {
      this.#publishSharedScores(context);
    }
  }

  /** The hall changed: its page, and the titles' best lines. */
  #publishSharedScores(context: Context): void {
    context.publish("channel", HALL, this.#hall.screen());
    for (const game of GAMES) {
      context.publish("channel", pageOf(game), this.#title(game));
    }
  }

  /** The players by session, in order: the Rust kept them in a sorted map, and the order sets who draws the channel's random numbers first. */
  #playersInOrder(): [number, Player][] {
    return [...this.#players.entries()].sort(([left], [right]) => left - right);
  }

  // No page here uses digits or the coloured keys, so the kit's page
  // selection has them everywhere (PORTING.md section 3.4).
  pages(): readonly PageEntry[] {
    return [
      { page: MENU, title: "Action games" },
      ...GAMES.map((game) => ({ page: pageOf(game), title: nameOf(game) })),
      { page: HALL, title: "Hall of fame" },
    ];
  }

  start(context: Context): void {
    this.#publishShared(context);
  }

  opened(session: number, page: number, context: Context): void {
    const game = gameOn(page);
    if (game !== undefined && ((this.#owned.get(session) ?? 0) & bitOf(game)) !== 0) {
      context.publish(session, page, this.#title(game));
    }
  }

  leftPage(session: number, page: number, context: Context): void {
    const player = this.#players.get(session);
    if (player !== undefined && pageOf(player.running.game()) === page) {
      this.#finish(session, context);
    }
  }

  input(session: number, page: number, input: Input, context: Context): void {
    const game = gameOn(page);
    if (game === undefined) {
      return;
    }
    if (pressed(input, Key.Escape)) {
      this.#finish(session, context);
      context.goto(session, MENU);
      return;
    }
    if (input.kind === "blur") {
      this.#players.get(session)?.keys.releaseAll();
      return;
    }
    const now = context.now();
    const found = this.#players.get(session);
    const player = found !== undefined && found.running.game() === game ? found : undefined;
    if (player !== undefined && player.endedAt === null) {
      // In play: the controls.
      const control = input.kind === "key" ? controlOf(input.key) : undefined;
      if (input.kind === "key" && control !== undefined) {
        if (pressed(input, input.key)) {
          player.keys.press(control);
          player.running.press(control);
        } else if (released(input, input.key)) {
          player.keys.release(control);
        }
      } else if (character(input) === SPACE_CHARACTER) {
        // Text that came from no key, a phone's: no release will
        // follow, so a tap.
        player.keys.tap(Control.Fire);
      }
      return;
    }
    // On the title page, or after a game: Space or Enter plays.
    const starts = character(input) === SPACE_CHARACTER || pressed(input, Key.Enter);
    const endedAt = player?.endedAt ?? null;
    const paused = endedAt !== null && now < endedAt + RESTART_PAUSE_MS;
    if (starts && !paused) {
      this.#startGame(session, game, context);
    }
  }

  wake(context: Context): void {
    const now = context.now();
    let scoresChanged = false;
    const finished: number[] = [];
    const changed: number[] = [];
    const sounds: [number, Effect][] = [];
    for (const [session, player] of this.#playersInOrder()) {
      if (player.endedAt !== null) {
        continue;
      }
      const effects: Effect[] = [];
      if (player.running.tick(player.keys, context, effects)) {
        changed.push(session);
      }
      player.keys.endTick();
      if (player.running.over()) {
        player.endedAt = now;
        finished.push(session);
        if (!changed.includes(session)) {
          changed.push(session);
        }
      }
      sounds.push(...effects.map((effect): [number, Effect] => [session, effect]));
    }
    for (const session of finished) {
      const player = this.#players.get(session);
      if (player !== undefined) {
        scoresChanged = this.#hall.record(player.running.game(), session, player.running.score()) || scoresChanged;
      }
    }
    for (const [session, effect] of sounds) {
      this.#sounds.play(session, effect, context);
    }
    for (const session of changed) {
      const player = this.#players.get(session);
      if (player !== undefined) {
        this.#publishGame(session, player, context);
      }
    }
    if (scoresChanged) {
      this.#publishSharedScores(context);
    }
    this.#ticking = [...this.#players.values()].some((player) => player.endedAt === null);
    if (this.#ticking) {
      context.wakeIn(TICK_MS);
    }
  }

  ended(session: number, context: Context): void {
    this.#finish(session, context);
    this.#owned.delete(session);
    this.#sounds.forget(session);
  }
}
