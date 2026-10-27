// User input (pages 200 to 299; crates/teletext-showcase/src/
// interactive.rs): a form with a masked field, a shared guestbook, a live
// poll, a quiz, a calculator and a mouse paint box.
//
// One application serves every page of the area, so a viewer is sent the
// sound library once however many of its pages they use, and the shared
// pages' republishing shares one wake.
//
// Scale (decision 7): the guestbook and the poll are the channel's pages,
// published once per change whatever the number of viewers, and a burst of
// changes is coalesced into at most one publication each
// REPUBLISH_INTERVAL_MS. What is one viewer's own (a half-typed field, a
// quiz score, a canvas) is small and forgotten when the session ends.

import type { Area, Section } from "./area.ts";
import { Calculator } from "./interactive/calculator.ts";
import { Form } from "./interactive/form.ts";
import { Guestbook } from "./interactive/guestbook.ts";
import { screen as menu } from "./interactive/menu.ts";
import { CALCULATOR, FORM, GUESTBOOK, MENU, PAINT, POLL, QUIZ, SIGN } from "./interactive/pages.ts";
import { Paint } from "./interactive/paint.ts";
import { Poll } from "./interactive/poll.ts";
import { Quiz } from "./interactive/quiz.ts";
import type { Application, Context, Input, PageEntry } from "./kit.ts";
import { Sounds } from "./sound.ts";

/** The application behind pages 200 to 207. */
export class Interactive implements Application {
  readonly #form = new Form();
  readonly #guestbook = new Guestbook();
  readonly #poll = new Poll();
  readonly #quiz = new Quiz();
  readonly #calculator = new Calculator();
  readonly #paint = new Paint();
  readonly #sounds = new Sounds();

  // The pages that type or choose with digits keep them; the menu and the
  // paint box, which takes only the mouse and C, leave digits to page
  // selection. None uses the coloured keys.
  pages(): readonly PageEntry[] {
    return [
      { page: MENU, title: "User input" },
      { page: FORM, title: "A form", ownsDigits: true },
      { page: GUESTBOOK, title: "Guestbook", ownsDigits: true },
      { page: POLL, title: "Live poll", ownsDigits: true },
      { page: QUIZ, title: "Teletext quiz", ownsDigits: true },
      { page: CALCULATOR, title: "Calculator", ownsDigits: true },
      { page: PAINT, title: "Mouse paint" },
      { page: SIGN, title: "Sign the guestbook", ownsDigits: true },
    ];
  }

  start(context: Context): void {
    context.publish("channel", MENU, menu());
    context.publish("channel", FORM, Form.firstScreen());
    context.publish("channel", QUIZ, Quiz.firstScreen());
    context.publish("channel", CALCULATOR, Calculator.firstScreen());
    context.publish("channel", PAINT, Paint.firstScreen());
    context.publish("channel", SIGN, Guestbook.firstSignScreen());
    this.#guestbook.start(context);
    this.#poll.start(context);
  }

  opened(session: number, page: number, context: Context): void {
    switch (page) {
      case FORM:
        this.#form.opened(session, context);
        return;
      case QUIZ:
        this.#quiz.opened(session, context);
        return;
      case CALCULATOR:
        this.#calculator.opened(session, context);
        return;
      case PAINT:
        this.#paint.opened(session, context);
        return;
      case SIGN:
        this.#guestbook.openedSign(session, context);
        return;
      default:
        return;
    }
  }

  /**
   * Answers whether the page took `input`. Every page is sent its clicks
   * (INPUT.md section 6.3), but only the paint page uses the mouse: on the
   * others a click is the kit's to follow, to the prompt or page number
   * under it.
   */
  input(session: number, page: number, input: Input, context: Context): boolean {
    switch (page) {
      case FORM:
        this.#form.input(session, input, this.#sounds, context);
        break;
      case GUESTBOOK:
      case SIGN:
        this.#guestbook.input(session, page, input, this.#sounds, context);
        break;
      case POLL:
        this.#poll.input(session, input, this.#sounds, context);
        break;
      case QUIZ:
        this.#quiz.input(session, input, this.#sounds, context);
        break;
      case CALCULATOR:
        this.#calculator.input(session, input, this.#sounds, context);
        break;
      case PAINT:
        return this.#paint.input(session, input, this.#sounds, context);
      default:
        return false;
    }
    return input.kind !== "mouse";
  }

  wake(context: Context): void {
    this.#guestbook.wake(context);
    this.#poll.wake(context);
  }

  ended(session: number, context: Context): void {
    this.#form.ended(session);
    this.#guestbook.ended(session);
    this.#poll.ended(session, context);
    this.#quiz.ended(session);
    this.#calculator.ended(session);
    this.#paint.ended(session);
    this.#sounds.forget(session);
  }
}

/** User input. */
export const interactive: Area = {
  applications: () => [new Interactive()],
  sections: (): readonly Section[] => [{ page: MENU, name: "User input", about: "guestbook, poll, quiz, paint" }],
};
