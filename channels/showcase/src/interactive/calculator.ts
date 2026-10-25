// Page 205: a calculator, each viewer's own (crates/teletext-showcase/src/
// interactive/calculator.rs). A sum of whole numbers with +, -, x or *, /
// and parentheses is typed, and Enter works it out in 64-bit integers,
// every step checked: overflow and division by zero are answers, not
// failures.

import { INDEX, page } from "../common.ts";
import { Colour, type Context, CursorFlag, ENGLISH, type Input, type Screen, textColour } from "../kit.ts";
import { Effect, type Sounds } from "../sound.ts";
import { BAND_LEAD, type Edit, Field, band, editOf } from "./field.ts";
import { CALCULATOR, MENU, PAINT, QUIZ } from "./pages.ts";

/** The longest sum. */
const SUM_LIMIT = 30;
/** The deepest nesting of parentheses and signs: more than a sum of SUM_LIMIT characters needs, so the parser's recursion is bounded whatever is typed. */
const DEEPEST = SUM_LIMIT;
/** Sums kept on the page. */
const HISTORY = 5;
/** The field's row. */
const FIELD_ROW = 5;
/** Where the field's band starts. */
const BAND_COLUMN = 1;
/** Base ten. */
const RADIX = 10n;
/** The range of a 64-bit signed integer, which every step must stay in. */
const LEAST = -(2n ** 63n);
const MOST = 2n ** 63n - 1n;
/** The first history row. */
const HISTORY_ROW = 12;
/** The answer's row, double height. */
const ANSWER_ROW = 8;
/** The column of the page's lines and of a history line's colour. */
const LINE_COLUMN = 1;
/** Where a history line's sum starts, after its colour. */
const HISTORY_TEXT_COLUMN = 2;

/** Why a sum has no answer. */
export const Problem = {
  DivisionByZero: "Cannot divide by zero",
  Overflow: "Too big for 64 bits",
  Unreadable: "Not a sum I can read",
} as const;
/** A problem, as its words on the page. */
export type Problem = (typeof Problem)[keyof typeof Problem];

/** A sum's answer, or its problem. */
export type Answer = { readonly value: bigint } | { readonly problem: Problem };

/** A sum's symbol as the calculator keeps it, from what a viewer types: x, X and × multiply, ÷ divides, − subtracts. */
export function symbol(typed: string): string | undefined {
  if (/^[0-9+\-*/() ]$/u.test(typed)) {
    return typed;
  }
  if (typed === "x" || typed === "X" || typed === "×") {
    return "*";
  }
  if (typed === "÷") {
    return "/";
  }
  return typed === "−" ? "-" : undefined;
}

/** A failed step: the parser stops with its problem. */
class Failure extends Error {
  readonly problem: Problem;

  constructor(problem: Problem) {
    super(problem);
    this.problem = problem;
  }
}

/** `value` if it fits in 64 bits. */
function checked(value: bigint): bigint {
  if (value < LEAST || value > MOST) {
    throw new Failure(Problem.Overflow);
  }
  return value;
}

/**
 * A recursive-descent parser of `sum := term (('+' | '-') term)*`,
 * `term := factor (('*' | '/') factor)*`, `factor := '-' factor | '(' sum
 * ')' | digits`.
 */
class Parser {
  readonly #symbols: string;
  #at = 0;
  #depth = 0;

  constructor(symbols: string) {
    this.#symbols = symbols;
  }

  /** The next symbol, past spaces. */
  peek(): string | undefined {
    while (this.#symbols[this.#at] === " ") {
      this.#at += 1;
    }
    return this.#symbols[this.#at];
  }

  #advance(): void {
    this.#at += 1;
  }

  sum(): bigint {
    let value = this.#term();
    for (let operator = this.peek(); operator === "+" || operator === "-"; operator = this.peek()) {
      this.#advance();
      const right = this.#term();
      value = checked(operator === "+" ? value + right : value - right);
    }
    return value;
  }

  #term(): bigint {
    let value = this.#factor();
    for (let operator = this.peek(); operator === "*" || operator === "/"; operator = this.peek()) {
      this.#advance();
      const right = this.#factor();
      if (operator === "*") {
        value = checked(value * right);
      } else if (right === 0n) {
        throw new Failure(Problem.DivisionByZero);
      } else {
        // Division rounds towards zero, as BigInt's does; the one that
        // overflows is the least value by -1.
        value = checked(value / right);
      }
    }
    return value;
  }

  #factor(): bigint {
    this.#depth += 1;
    if (this.#depth > DEEPEST) {
      throw new Failure(Problem.Unreadable);
    }
    let value: bigint;
    const next = this.peek();
    if (next === "-") {
      this.#advance();
      value = checked(-this.#factor());
    } else if (next === "(") {
      this.#advance();
      value = this.sum();
      if (this.peek() !== ")") {
        throw new Failure(Problem.Unreadable);
      }
      this.#advance();
    } else if (next !== undefined && next >= "0" && next <= "9") {
      value = this.#number();
    } else {
      throw new Failure(Problem.Unreadable);
    }
    this.#depth -= 1;
    return value;
  }

  #number(): bigint {
    let value = 0n;
    for (let digit = this.#symbols[this.#at]; digit !== undefined && digit >= "0" && digit <= "9"; digit = this.#symbols[this.#at]) {
      this.#advance();
      value = checked(value * RADIX + BigInt(digit));
    }
    return value;
  }
}

/** Works out `sum`. */
export function evaluate(sum: string): Answer {
  const parser = new Parser(sum);
  try {
    const value = parser.sum();
    return parser.peek() === undefined ? { value } : { problem: Problem.Unreadable };
  } catch (error) {
    if (error instanceof Failure) {
      return { problem: error.problem };
    }
    throw error;
  }
}

/** A sum as the page shows it: x for multiply and the English set's divide sign. */
function codesOf(sum: string): number[] {
  return [...sum].map((typed) => (typed === "*" ? "x".charCodeAt(0) : typed === "/" ? ENGLISH.divide : typed.charCodeAt(0)));
}

/** One viewer's calculator. */
class State {
  readonly field = new Field(SUM_LIMIT);
  answer: Answer | null = null;
  /** Newest first. */
  readonly history: (readonly [sum: string, answer: Answer])[] = [];

  /** Acts on an edit: the sound, if it changed anything. */
  edit(edit: Edit): Effect | null {
    switch (edit.kind) {
      case "text":
      case "character":
        this.field.typeText(edit.kind === "text" ? edit.text : edit.character, symbol);
        return Effect.Click;
      case "backspace":
        return this.field.backspace() ? Effect.Click : null;
      case "escape":
        this.field.clear();
        this.answer = null;
        return Effect.Click;
      case "enter": {
        if (this.field.isBlank()) {
          return null;
        }
        const answer = evaluate(this.field.text());
        this.history.unshift([this.field.text().trim(), answer]);
        this.history.splice(HISTORY);
        this.answer = answer;
        this.field.clear();
        return "value" in answer ? Effect.Bell : Effect.Hit;
      }
      case "tab":
      case "other":
        return null;
    }
  }

  screen(): Screen {
    const screen = page("Calculator", Colour.Yellow, [
      ["Input", MENU],
      ["Quiz", QUIZ],
      ["Paint", PAINT],
      ["Index", INDEX],
    ]);
    screen.coloured(3, LINE_COLUMN, Colour.Cyan, "Type a sum, then press Enter:");
    band(screen, FIELD_ROW, BAND_COLUMN, SUM_LIMIT, Colour.Blue, Colour.White, codesOf(this.field.text()));
    this.field.cursor(screen, FIELD_ROW, BAND_COLUMN + BAND_LEAD, CursorFlag.Echo);
    if (this.answer !== null) {
      if ("value" in this.answer) {
        screen.doubleHeight(ANSWER_ROW, LINE_COLUMN, Colour.Yellow, `= ${this.answer.value}`);
      } else {
        screen.doubleHeight(ANSWER_ROW, LINE_COLUMN, Colour.Red, this.answer.problem);
      }
    }
    if (this.history.length > 0) {
      screen.coloured(11, LINE_COLUMN, Colour.Cyan, "Your last sums");
    }
    this.history.forEach(([sum, answer], index) => {
      const row = HISTORY_ROW + index;
      const codes = [...codesOf(sum), ...codesOf(" = ")];
      screen.code(row, LINE_COLUMN, textColour(Colour.White));
      screen.codes(row, HISTORY_TEXT_COLUMN, codes);
      const column = codes.length + HISTORY_TEXT_COLUMN;
      if ("value" in answer) {
        screen.coloured(row, column, Colour.Yellow, `${answer.value}`);
      } else {
        screen.coloured(row, column, Colour.Red, answer.problem);
      }
    });
    screen.coloured(19, LINE_COLUMN, Colour.Green, "Whole numbers, 64 bits, each step");
    screen.coloured(20, LINE_COLUMN, Colour.Green, "checked; division rounds to zero.");
    screen.code(22, LINE_COLUMN, textColour(Colour.Magenta));
    screen.codes(22, HISTORY_TEXT_COLUMN, codesOf("+ - * / ( )   Backspace   Esc clears"));
    return screen;
  }
}

/** Every viewer's calculator. */
export class Calculator {
  readonly #sessions = new Map<number, State>();

  /** The page as a viewer first sees it, which is also the channel's copy, for a receiver that shows the page before saying it opened it. */
  static firstScreen(): Screen {
    return new State().screen();
  }

  #state(session: number): State {
    const state = this.#sessions.get(session) ?? new State();
    this.#sessions.set(session, state);
    return state;
  }

  opened(session: number, context: Context): void {
    context.publish(session, CALCULATOR, this.#state(session).screen());
  }

  input(session: number, input: Input, sounds: Sounds, context: Context): void {
    const state = this.#state(session);
    const effect = state.edit(editOf(input));
    if (effect !== null) {
      context.publish(session, CALCULATOR, state.screen());
      sounds.play(session, effect, context);
    }
  }

  ended(session: number): void {
    this.#sessions.delete(session);
  }
}
