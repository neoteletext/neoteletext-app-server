// The input area's pages, and the coalescing of a shared page's changes
// (crates/teletext-showcase/src/interactive.rs).

import type { Context } from "../kit.ts";

/** The area's menu. */
export const MENU = 0x200;
/** A form: a name and a masked field. */
export const FORM = 0x201;
/** The guestbook everyone sees. */
export const GUESTBOOK = 0x202;
/** The live poll's results, where viewers vote. */
export const POLL = 0x203;
/** The quiz. */
export const QUIZ = 0x204;
/** The calculator. */
export const CALCULATOR = 0x205;
/** The mouse paint box. */
export const PAINT = 0x206;
/** Signing the guestbook: each viewer's own field, since a shared page cannot hold what one viewer is typing. */
export const SIGN = 0x207;

/**
 * The shortest time between two publications of one shared page: four a
 * second is live to a reader and bounds what a burst of posts or votes
 * costs every viewer.
 */
export const REPUBLISH_INTERVAL_MS = 250;

/**
 * When a shared page may next be published: at once if it has rested
 * {@link REPUBLISH_INTERVAL_MS}, else once at the end of that time,
 * however many changes come in between.
 */
export class Republish {
  #last: number | null = null;
  #pending = false;

  /** The time the page may next be published. */
  #due(): number {
    return this.#last === null ? 0 : this.#last + REPUBLISH_INTERVAL_MS;
  }

  /** The page changed: whether to publish it now. If not, a wake is asked for its due time and the change waits for it. */
  changed(context: Context): boolean {
    if (this.#pending) {
      return false;
    }
    const due = this.#due();
    if (context.now() >= due) {
      this.#last = context.now();
      return true;
    }
    this.#pending = true;
    context.wakeAt(due);
    return false;
  }

  /**
   * A wake came: whether a waiting change is due now. A change still
   * waiting asks for its wake again, since the channel keeps one wake for
   * the application and this one may have been for another page.
   */
  woken(context: Context): boolean {
    if (!this.#pending) {
      return false;
    }
    const due = this.#due();
    if (context.now() >= due) {
      this.#pending = false;
      this.#last = context.now();
      return true;
    }
    context.wakeAt(due);
    return false;
  }
}
