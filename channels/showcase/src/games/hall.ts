// The hall of fame: the best scores of every game and every player, one
// shared page (SERVER.md section 2; crates/teletext-showcase/src/games/
// hall.rs). It changes when a game ends with a score good enough, and is
// published once for every viewer.

import { INDEX, page } from "../common.ts";
import { Colour, type Screen } from "../kit.ts";
import { type Best, digits } from "./look.ts";
import { type Game, INVADERS, MENU, SNAKE, nameOf } from "./pages.ts";

/** The scores kept: as many as the page has rows for. */
const PLACES = 12;
/** The table: its heading under the title band, the scores from two rows below it, each after its colour in column 1. */
const HEADING_ROW = 3;
const FIRST_ROW = 5;
const TABLE_COLUMN = 1;
/** Where an empty table says so. */
const EMPTY_ROW = 9;
/** The note at the foot, under the twelve places. */
const FOOT_ROW = 19;
/** Players are named by their session's number, cut to four digits: no name is asked for, and the number is enough to tell players apart on one page. */
const PLAYER_NUMBERS = 10_000;
const PLAYER_DIGITS = 4;
/** The table's columns: the place's width, then the player's and the game's. */
const PLACE_WIDTH = 2;
const NAME_WIDTH = 12;

/** One score. */
export interface Entry {
  readonly score: number;
  readonly game: Game;
  /** The player's session. */
  readonly player: number;
}

/** A player's name on the page. */
export function playerName(session: number): string {
  return `Player ${(session % PLAYER_NUMBERS).toString().padStart(PLAYER_DIGITS, "0")}`;
}

/** The best scores, best first. */
export class Hall {
  readonly #entries: Entry[] = [];

  /**
   * Records `score`; whether it made the table, so the page changed. A
   * score that ties one already there goes under it: the first to reach
   * it keeps the place.
   */
  record(game: Game, player: number, score: number): boolean {
    if (score === 0) {
      return false;
    }
    const better = this.#entries.findIndex((entry) => entry.score < score);
    const place = better === -1 ? this.#entries.length : better;
    if (place >= PLACES) {
      return false;
    }
    this.#entries.splice(place, 0, { score, game, player });
    this.#entries.length = Math.min(this.#entries.length, PLACES);
    return true;
  }

  /** The best score of `game` and who made it. */
  best(game: Game): Best | undefined {
    const entry = this.#entries.find((each) => each.game === game);
    return entry === undefined ? undefined : [entry.score, playerName(entry.player)];
  }

  /** The scores, best first. */
  entries(): readonly Entry[] {
    return this.#entries;
  }

  /** The page. */
  screen(): Screen {
    const screen = page("Hall of fame", Colour.Blue, [
      ["Menu", MENU],
      ["Invaders", INVADERS],
      ["Snake", SNAKE],
      ["Index", INDEX],
    ]);
    screen.coloured(HEADING_ROW, TABLE_COLUMN, Colour.Cyan, "No  Player       Game          Score");
    if (this.#entries.length === 0) {
      screen.centred(EMPTY_ROW, Colour.White, "No scores yet.");
      screen.centred(EMPTY_ROW + 2, Colour.Green, "Play a game: yours could be first.");
    }
    let row = FIRST_ROW;
    this.#entries.forEach((entry, place) => {
      // Gold, then the others in turn, so the rows read across.
      const colour = place === 0 ? Colour.Yellow : place % 2 === 1 ? Colour.White : Colour.Cyan;
      const number = (place + 1).toString().padStart(PLACE_WIDTH);
      const line = `${number}  ${playerName(entry.player).padEnd(NAME_WIDTH)} ${nameOf(entry.game).padEnd(NAME_WIDTH)}  ${digits(entry.score)}`;
      screen.coloured(row, TABLE_COLUMN, colour, line);
      row += 1;
    });
    screen.centred(FOOT_ROW, Colour.Green, "One shared page: every viewer sees");
    screen.centred(FOOT_ROW + 1, Colour.Green, "each new best the moment a game ends");
    return screen;
  }
}
