// Page 106: sport, simulated (crates/teletext-showcase/src/services/
// sport.rs). A made-up league of eight teams plays its rounds live: four
// matches at once, scores changing as the simulated minutes pass, a goal
// flashing until the next update, and the table as it stands with the live
// scores in it. The page says it is demonstration data.

import { page } from "../common.ts";
import { type Application, Attribute, Colour, type Context, type PageEntry, Screen, textColour } from "../kit.ts";
import { SECOND_MS } from "./calendar.ts";
import { band, demonstration, publish, right } from "./draw.ts";
import { SPORT, navigation } from "./pages.ts";

/** The league's teams. */
const TEAMS = ["Sixel Rovers", "Mosaic United", "Fastext Town", "Parity City", "Header Albion", "Carousel FC", "Raster Rangers", "Pixel Athletic"] as const;
/** The number of teams, which the schedule pairs off. */
const TEAM_COUNT = TEAMS.length;
/** The matches in a round: every team plays. */
const MATCHES = TEAM_COUNT / 2;
/** Rounds in which every team meets every other once (the circle method). */
const ROUNDS_ONCE = TEAM_COUNT - 1;
/** A season: each pair meets home and away. */
const SEASON_ROUNDS = ROUNDS_ONCE * 2;

/** The page changes every fifteen seconds. */
const INTERVAL_MS = 15 * SECOND_MS;
/** Simulated minutes in each update: a match lasts 30 updates, seven and a half minutes. */
const MINUTES_PER_UPDATE = 3;
/** A match's length in minutes. */
const FULL_TIME = 90;
/** Half time, where play pauses for one update. */
const HALF_TIME = 45;
/** Updates at full time before the next round kicks off: one minute. */
const FULL_TIME_UPDATES = 4;
/** The chance of a goal for the home team in an update, in thousandths: about 1.5 goals a match. */
const HOME_GOAL_CHANCE = 50;
/** For the away team: about 1.2. */
const AWAY_GOAL_CHANCE = 40;
/** Chances are reckoned in thousandths. */
const CHANCE_SCALE = 1_000;
/** Points for a win. */
const WIN_POINTS = 3;

/** The row of the round and the time. */
const STATUS_ROW = 3;
/** The first match's row. */
const MATCHES_ROW = 5;
/** The width of a team's name. */
const NAME_WIDTH = 14;
/** The score's colour attribute's column, after the home team. */
const SCORE_COLUMN = 15;
/** The away team's colour attribute's column. */
const AWAY_COLUMN = 21;
/** The table's heading row. */
const TABLE_ROW = 10;
/** The note's row, under the table. */
const NOTE_ROW = 20;
/** The table's columns: a position, played, won, drawn and lost in two characters; the goal difference and points in three. */
const COUNT_WIDTH = 2;
const TOTAL_WIDTH = 3;

/** Where the matches are: playing at a minute, half time for one update, or full time with updates left before the next round. */
export type Phase = { readonly kind: "playing"; readonly minute: number } | { readonly kind: "halfTime" } | { readonly kind: "fullTime"; readonly left: number };

/** One match of the round: the teams as indices into the teams, the goals, and whether a goal came in the last update. */
export interface Match {
  home: number;
  away: number;
  homeGoals: number;
  awayGoals: number;
  scored: boolean;
}

/** A team's line of the table. */
export class Standing {
  readonly played: number;
  readonly won: number;
  readonly drawn: number;
  readonly lost: number;
  readonly scored: number;
  readonly conceded: number;

  constructor(played = 0, won = 0, drawn = 0, lost = 0, scored = 0, conceded = 0) {
    this.played = played;
    this.won = won;
    this.drawn = drawn;
    this.lost = lost;
    this.scored = scored;
    this.conceded = conceded;
  }

  /** Points: three for a win, one for a draw. */
  points(): number {
    return this.won * WIN_POINTS + this.drawn;
  }

  /** Goal difference. */
  difference(): number {
    return this.scored - this.conceded;
  }

  /** The standing with one more result. */
  with(scored: number, conceded: number): Standing {
    return new Standing(
      this.played + 1,
      this.won + (scored > conceded ? 1 : 0),
      this.drawn + (scored === conceded ? 1 : 0),
      this.lost + (scored < conceded ? 1 : 0),
      this.scored + scored,
      this.conceded + conceded,
    );
  }
}

/**
 * Round `round`'s matches by the circle method: team 0 stays, the others
 * turn one place each round, and the second half of the season swaps home
 * and away.
 */
export function fixtures(round: number): Match[] {
  const turn = round % ROUNDS_ONCE;
  const secondHalf = round >= ROUNDS_ONCE;
  // The circle's places: place 0 is team 0, place p > 0 holds the team
  // the turn has moved there.
  const teamAt = (place: number): number => (place === 0 ? 0 : ((place - 1 + turn) % ROUNDS_ONCE) + 1);
  return Array.from({ length: MATCHES }, (_, index) => {
    const [first, second] = [teamAt(index), teamAt(TEAM_COUNT - 1 - index)];
    // Alternating who is at home spreads home matches across teams.
    const firstAtHome = (turn + index) % 2 === 0;
    const [home, away] = firstAtHome !== secondHalf ? [first, second] : [second, first];
    return { home, away, homeGoals: 0, awayGoals: 0, scored: false };
  });
}

/** The league: the round being played and the table before it. */
export class Sport implements Application {
  /** The round in the season, from 0. */
  round = 0;
  /** The season, from 1. */
  season = 1;
  phase: Phase = { kind: "playing", minute: 0 };
  matches: Match[] = fixtures(0);
  /** The table before this round. */
  table: Standing[] = TEAMS.map(() => new Standing());

  pages(): readonly PageEntry[] {
    return [{ page: SPORT, title: "Sport" }];
  }

  /** The table with the matches' scores as they stand counted in. */
  liveTable(): Standing[] {
    const table = [...this.table];
    const started = !(this.phase.kind === "playing" && this.phase.minute === 0);
    if (started) {
      for (const game of this.matches) {
        table[game.home] = (table[game.home] ?? new Standing()).with(game.homeGoals, game.awayGoals);
        table[game.away] = (table[game.away] ?? new Standing()).with(game.awayGoals, game.homeGoals);
      }
    }
    return table;
  }

  /** One update: the clock runs on, goals may come, and after full time the next round begins. */
  update(context: Context): void {
    for (const game of this.matches) {
      game.scored = false;
    }
    const phase = this.phase;
    if (phase.kind === "playing") {
      const minute = Math.min(phase.minute + MINUTES_PER_UPDATE, FULL_TIME);
      for (const game of this.matches) {
        if (context.below(CHANCE_SCALE) < HOME_GOAL_CHANCE) {
          game.homeGoals += 1;
          game.scored = true;
        }
        if (context.below(CHANCE_SCALE) < AWAY_GOAL_CHANCE) {
          game.awayGoals += 1;
          game.scored = true;
        }
      }
      this.phase = minute === HALF_TIME ? { kind: "halfTime" } : minute === FULL_TIME ? { kind: "fullTime", left: FULL_TIME_UPDATES } : { kind: "playing", minute };
    } else if (phase.kind === "halfTime") {
      this.phase = { kind: "playing", minute: HALF_TIME };
    } else if (phase.left === 0) {
      this.#nextRound();
      this.phase = { kind: "playing", minute: 0 };
    } else {
      this.phase = { kind: "fullTime", left: phase.left - 1 };
    }
  }

  /** Counts the round into the table and draws up the next; a new season starts with an empty table. */
  #nextRound(): void {
    this.table = this.liveTable();
    this.round += 1;
    if (this.round >= SEASON_ROUNDS) {
      this.round = 0;
      this.season += 1;
      this.table = TEAMS.map(() => new Standing());
    }
    this.matches = fixtures(this.round);
  }

  /** The page. */
  screen(): Screen {
    const screen = page("Sport", Colour.Red, navigation(SPORT));
    screen.coloured(STATUS_ROW, 0, Colour.Cyan, `Mosaic League, round ${this.round + 1}`);
    const phase = this.phase;
    const time =
      phase.kind === "playing" ? (phase.minute === 0 ? "Kick-off" : `${phase.minute} min`) : phase.kind === "halfTime" ? "Half time" : "Full time";
    right(screen, STATUS_ROW, Colour.Yellow, time);
    this.matches.forEach((game, index) => drawMatch(screen, MATCHES_ROW + index, game));
    // The heading's text starts two cells right of the rows', where their
    // positions are, so it leaves that field out.
    const counts = ["P", "W", "D", "L"].map((heading) => heading.padStart(COUNT_WIDTH)).join(" ");
    const heading = ` ${"TEAM".padEnd(NAME_WIDTH)} ${counts} ${"GD".padStart(TOTAL_WIDTH)} ${"PTS".padStart(TOTAL_WIDTH)}`;
    band(screen, TABLE_ROW, Colour.Blue, Colour.White, heading);
    const table = this.liveTable();
    const standing = (team: number): Standing => table[team] ?? new Standing();
    const order = TEAMS.map((_, team) => team).sort((a, b) => {
      const [first, second] = [standing(a), standing(b)];
      return second.points() - first.points() || second.difference() - first.difference() || second.scored - first.scored || a - b;
    });
    order.forEach((team, position) => {
      const line = standing(team);
      const columns = [line.played, line.won, line.drawn, line.lost].map((value) => value.toString().padStart(COUNT_WIDTH)).join(" ");
      const name = (TEAMS[team] ?? "").padEnd(NAME_WIDTH);
      const totals = `${line.difference().toString().padStart(TOTAL_WIDTH)} ${line.points().toString().padStart(TOTAL_WIDTH)}`;
      const text = `${(position + 1).toString().padStart(COUNT_WIDTH)} ${name} ${columns} ${totals}`;
      screen.coloured(TABLE_ROW + 1 + position, 0, position === 0 ? Colour.Yellow : Colour.White, text);
    });
    const note = phase.kind === "fullTime" ? "next round kicks off in a minute" : "scores update every 15 s";
    screen.coloured(NOTE_ROW, 0, Colour.Green, `Season ${this.season}, ${note}`);
    demonstration(screen);
    return screen;
  }

  start(context: Context): void {
    publish(context, SPORT, this.screen());
    context.wakeIn(INTERVAL_MS);
  }

  wake(context: Context): void {
    this.update(context);
    publish(context, SPORT, this.screen());
    context.wakeIn(INTERVAL_MS);
  }
}

/** A match's line: the home team right-aligned against the score, a new goal's score flashing, the away team after it. */
function drawMatch(screen: Screen, row: number, game: Match): void {
  screen.coloured(row, 0, Colour.White, (TEAMS[game.home] ?? "").padStart(NAME_WIDTH));
  screen.code(row, SCORE_COLUMN, textColour(Colour.Yellow));
  if (game.scored) {
    screen.code(row, SCORE_COLUMN + 1, Attribute.Flash);
  }
  screen.text(row, SCORE_COLUMN + 2, `${game.homeGoals}-${game.awayGoals}`);
  // Steady is set-at, so the cell after the score is already steady.
  screen.code(row, AWAY_COLUMN - 1, Attribute.Steady);
  screen.coloured(row, AWAY_COLUMN, Colour.White, TEAMS[game.away] ?? "");
}
