// Page 203: a live poll (crates/teletext-showcase/src/interactive/
// poll.rs). Viewers vote on the results page itself with the keys 1 to 4,
// and the results, the channel's one page, change for every viewer at once
// (decision 7), coalesced as the guestbook is.
//
// The poll counts the sessions connected now: a session has one vote,
// which it may change at most once each CHANGE_INTERVAL_MS, and its vote
// goes when it ends. So no viewer counts twice by reconnecting, and the
// state is bounded by the sessions.

import { INDEX, page } from "../common.ts";
import { Colour, type Context, type Input, type Screen, Sixels } from "../kit.ts";
import { Effect, type Sounds } from "../sound.ts";
import { choice } from "./field.ts";
import { GUESTBOOK, MENU, POLL, QUIZ, Republish } from "./pages.ts";

/** The question, on two rows. */
const QUESTION = ["Which teletext page do you", "turn to first?"] as const;
/** The answers, with each one's colour. */
export const ANSWERS: readonly (readonly [answer: string, colour: Colour])[] = [
  ["News", Colour.Red],
  ["Weather", Colour.Green],
  ["Sport", Colour.Yellow],
  ["TV listings", Colour.Cyan],
];
/** The shortest time between two votes of one session. */
export const CHANGE_INTERVAL_MS = 1_000;
/** The question's first row. */
const QUESTION_ROW = 3;
/** The first answer's row; each answer takes a label row, a bar row and a gap. */
const FIRST_ROW = 6;
/** Rows each answer takes. */
const ROWS_EACH = 3;
/** The widest bar, in cells: from column 2 to 37. */
export const BAR_CELLS = 36;
/** Where a bar starts, after its colour attribute. */
const BAR_COLUMN = 2;
/** The sixel rows a bar fills of the cell's three: two, so bars stand apart from the label below. */
const BAR_SIXEL_ROWS = 2;
/** A cell's sixels across. */
const SIXELS_EACH = 2;
/** Where the count and percentage stand, so they end inside the bar's width. */
const PERCENT_COLUMN = 28;
/** A whole, in percent. */
const WHOLE = 100;
/** The widths of a count and a percentage. */
const COUNT_WIDTH = 3;
/** The column of the page's notes. */
const NOTE_COLUMN = 1;

/** One session's vote. */
interface Vote {
  readonly answer: number;
  readonly at: number;
}

/** `part` of `whole` as `scale` would show it, rounded down; 0 of nothing. */
function share(part: number, whole: number, scale: number): number {
  return whole === 0 ? 0 : Math.floor((part * scale) / whole);
}

/** The poll. */
export class Poll {
  readonly #votes = new Map<number, Vote>();
  readonly #counts: number[] = ANSWERS.map(() => 0);
  readonly #republish = new Republish();

  start(context: Context): void {
    context.publish("channel", POLL, this.#screen());
  }

  #screen(): Screen {
    const screen = page("Live poll", Colour.Magenta, [
      ["Input", MENU],
      ["Guestbook", GUESTBOOK],
      ["Quiz", QUIZ],
      ["Index", INDEX],
    ]);
    QUESTION.forEach((line, offset) => screen.coloured(QUESTION_ROW + offset, NOTE_COLUMN, Colour.White, line));
    const total = this.#counts.reduce((sum, count) => sum + count, 0);
    ANSWERS.forEach(([answer, colour], index) => {
      const row = FIRST_ROW + index * ROWS_EACH;
      const count = this.#counts[index] ?? 0;
      screen.coloured(row, 0, colour, `${index + 1} ${answer}`);
      const percent = share(count, total, WHOLE);
      screen.coloured(row, PERCENT_COLUMN, Colour.White, `${count.toString().padStart(COUNT_WIDTH)}  ${percent.toString().padStart(COUNT_WIDTH)}%`);
      const barCells = share(count, total, BAR_CELLS);
      const bar = new Sixels(BAR_CELLS * SIXELS_EACH, BAR_SIXEL_ROWS);
      for (let x = 0; x < barCells * SIXELS_EACH; x += 1) {
        for (let y = 0; y < BAR_SIXEL_ROWS; y += 1) {
          bar.set(x, y, true);
        }
      }
      bar.draw(screen, row + 1, BAR_COLUMN, colour);
    });
    screen.coloured(19, NOTE_COLUMN, Colour.Cyan, `${total === 1 ? "1 vote" : `${total} votes`} from viewers here now`);
    screen.coloured(21, NOTE_COLUMN, Colour.Yellow, "Press 1, 2, 3 or 4 to vote; press");
    screen.coloured(22, NOTE_COLUMN, Colour.Yellow, "another to change your mind.");
    return screen;
  }

  input(session: number, input: Input, sounds: Sounds, context: Context): void {
    const answer = choice(input, ANSWERS.length);
    if (answer === undefined) {
      return;
    }
    const now = context.now();
    const vote = this.#votes.get(session);
    if (vote !== undefined && (vote.answer === answer || Math.max(0, now - vote.at) < CHANGE_INTERVAL_MS)) {
      sounds.play(session, Effect.Hit, context);
      return;
    }
    this.#votes.set(session, { answer, at: now });
    this.#count(vote?.answer ?? null, answer);
    sounds.play(session, Effect.Click, context);
    this.#changed(context);
  }

  /** Moves one vote from `from` to `to`. */
  #count(from: number | null, to: number | null): void {
    if (from !== null) {
      this.#counts[from] = Math.max(0, (this.#counts[from] ?? 0) - 1);
    }
    if (to !== null) {
      this.#counts[to] = (this.#counts[to] ?? 0) + 1;
    }
  }

  #changed(context: Context): void {
    if (this.#republish.changed(context)) {
      context.publish("channel", POLL, this.#screen());
    }
  }

  wake(context: Context): void {
    if (this.#republish.woken(context)) {
      context.publish("channel", POLL, this.#screen());
    }
  }

  ended(session: number, context: Context): void {
    const vote = this.#votes.get(session);
    if (vote !== undefined) {
      this.#votes.delete(session);
      this.#count(vote.answer, null);
      this.#changed(context);
    }
  }
}
