// Page 204: a quiz of teletext and television history, each viewer's own
// (crates/teletext-showcase/src/interactive/quiz.rs): ten questions
// answered with 1 to 4, a score, and a result.

import { INDEX, page } from "../common.ts";
import { Colour, type Context, type Input, Key, type Screen, pressed } from "../kit.ts";
import { Effect, type Sounds } from "../sound.ts";
import { choice } from "./field.ts";
import { CALCULATOR, MENU, POLL, QUIZ } from "./pages.ts";

/** One question: its text on up to two rows, four answers, and the right one's place, 0 to 3. */
export interface Question {
  readonly text: readonly [string, string];
  readonly answers: readonly [string, string, string, string];
  readonly right: number;
}

/** The questions: facts of the record, well known. */
export const QUESTIONS: readonly Question[] = [
  { text: ["In which year did the BBC's Ceefax", "service begin?"], answers: ["1966", "1974", "1982", "1990"], right: 1 },
  { text: ["Before 1993, ITV's teletext service", "was called what?"], answers: ["Prestel", "Antiope", "Oracle", "Viewdata"], right: 2 },
  { text: ["Which page carried subtitles on UK", "teletext?"], answers: ["100", "302", "500", "888"], right: 3 },
  { text: ["How many characters fit across one", "teletext row?"], answers: ["40", "32", "64", "80"], right: 0 },
  { text: ["How many colours has Level 1", "teletext, black included?"], answers: ["4", "16", "8", "256"], right: 2 },
  { text: ["A mosaic graphics character divides", "its cell into how many blocks?"], answers: ["4", "6", "8", "9"], right: 1 },
  {
    text: ["Teletext travels in which part of", "the television signal?"],
    answers: ["The sound carrier", "The colour burst", "The vertical blanking interval", "The line sync pulses"],
    right: 2,
  },
  {
    text: ["Who first showed television to the", "public, in London in 1926?"],
    answers: ["John Logie Baird", "Guglielmo Marconi", "Thomas Edison", "Alexander Graham Bell"],
    right: 0,
  },
  { text: ["Antiope was the teletext system of", "which country?"], answers: ["Germany", "Italy", "Sweden", "France"], right: 3 },
  { text: ["In which year did Ceefax close?", ""], answers: ["2006", "2009", "2012", "2015"], right: 2 },
];
/** The answers to each question. */
const ANSWERS = 4;
/** The score that wins. */
const PASS = 7;
/** The answers' first row, and rows between them. */
const ANSWER_ROW = 9;
const ANSWER_STEP = 2;
/** The question's first row. */
const QUESTION_ROW = 6;
/** The score's column. */
const SCORE_COLUMN = 27;
/** The answer's column, after its number and colour. */
const ANSWER_COLUMN = 3;
/** The column of the page's lines. */
const LINE_COLUMN = 1;

/** What the last answer was: none yet, right, or wrong for the question asked. */
type Last = { readonly kind: "none" } | { readonly kind: "right" } | { readonly kind: "wrong"; readonly asked: number };

/** One viewer's game. */
class Game {
  /** The questions answered so far. */
  asked = 0;
  score = 0;
  last: Last = { kind: "none" };

  finished(): boolean {
    return this.asked >= QUESTIONS.length;
  }

  /** Answers the question asked with `answer`, 0 to 3: the sound. */
  answer(answer: number): Effect {
    const question = QUESTIONS[this.asked];
    if (question === undefined) {
      return Effect.Hit;
    }
    const right = answer === question.right;
    if (right) {
      this.score += 1;
      this.last = { kind: "right" };
    } else {
      this.last = { kind: "wrong", asked: this.asked };
    }
    this.asked += 1;
    if (this.finished()) {
      return this.score >= PASS ? Effect.Win : Effect.Lose;
    }
    return right ? Effect.Coin : Effect.Hit;
  }

  screen(): Screen {
    const screen = page("Teletext quiz", Colour.Cyan, [
      ["Input", MENU],
      ["Poll", POLL],
      ["Sums", CALCULATOR],
      ["Index", INDEX],
    ]);
    const total = QUESTIONS.length;
    if (this.last.kind === "right") {
      screen.coloured(4, LINE_COLUMN, Colour.Green, "Right!");
    } else if (this.last.kind === "wrong") {
      const question = QUESTIONS[this.last.asked];
      if (question !== undefined) {
        screen.coloured(4, LINE_COLUMN, Colour.Red, `No: it was ${question.right + 1}, ${question.answers[question.right] ?? ""}`);
      }
    }
    const question = QUESTIONS[this.asked];
    if (question !== undefined) {
      screen.coloured(3, LINE_COLUMN, Colour.Cyan, `Question ${this.asked + 1} of ${total}`);
      screen.coloured(3, SCORE_COLUMN, Colour.Cyan, `Score ${this.score}`);
      question.text.forEach((line, offset) => screen.coloured(QUESTION_ROW + offset, LINE_COLUMN, Colour.White, line));
      question.answers.forEach((answer, index) => {
        const row = ANSWER_ROW + index * ANSWER_STEP;
        screen.coloured(row, LINE_COLUMN, Colour.Yellow, `${index + 1}`);
        screen.coloured(row, ANSWER_COLUMN, Colour.White, answer);
      });
      screen.coloured(21, LINE_COLUMN, Colour.Magenta, "Press 1, 2, 3 or 4 to answer");
    } else {
      screen.doubleHeight(8, LINE_COLUMN, Colour.Yellow, `You scored ${this.score} out of ${total}`);
      const verdict =
        this.score === total
          ? "Perfect: a teletext legend!"
          : this.score >= PASS
            ? "Well done: a true viewer."
            : this.score >= Math.floor(total / 2)
              ? "Not bad: keep reading."
              : "Time to read more teletext!";
      screen.coloured(11, LINE_COLUMN, Colour.White, verdict);
      screen.coloured(21, LINE_COLUMN, Colour.Magenta, "Press Enter to play again");
    }
    return screen;
  }
}

/** Every viewer's game. */
export class Quiz {
  readonly #games = new Map<number, Game>();

  /** The page as a viewer first sees it, which is also the channel's copy, for a receiver that shows the page before saying it opened it. */
  static firstScreen(): Screen {
    return new Game().screen();
  }

  #game(session: number): Game {
    const game = this.#games.get(session) ?? new Game();
    this.#games.set(session, game);
    return game;
  }

  opened(session: number, context: Context): void {
    context.publish(session, QUIZ, this.#game(session).screen());
  }

  input(session: number, input: Input, sounds: Sounds, context: Context): void {
    let game = this.#game(session);
    let effect: Effect;
    if (game.finished()) {
      if (!pressed(input, Key.Enter)) {
        return;
      }
      game = new Game();
      this.#games.set(session, game);
      effect = Effect.Click;
    } else {
      const answer = choice(input, ANSWERS);
      if (answer === undefined) {
        return;
      }
      effect = game.answer(answer);
    }
    context.publish(session, QUIZ, game.screen());
    sounds.play(session, effect, context);
  }

  ended(session: number): void {
    this.#games.delete(session);
  }
}
