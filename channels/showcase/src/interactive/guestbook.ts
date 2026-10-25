// Page 202, the guestbook every viewer sees, and page 207, where each
// viewer types a line for it (crates/teletext-showcase/src/interactive/
// guestbook.rs).
//
// The book is the channel's page, published once per change (decision 7).
// What a viewer is typing is theirs, so it is on a page of their own:
// typing on the book starts a line there and sends them to it; Enter posts
// the line and sends them back to the book.
//
// What viewers type reaches every viewer, so it is bounded: a line is at
// most MESSAGE_LIMIT characters the page can show, the book keeps the
// newest ENTRIES_KEPT, and a session posts at most once each
// POST_INTERVAL_MS.

import { INDEX, page } from "../common.ts";
import { Colour, type Context, CursorFlag, type Input, type Screen } from "../kit.ts";
import { Effect, type Sounds } from "../sound.ts";
import { BAND_LEAD, Field, band, codesOf, editOf, printable } from "./field.ts";
import { GUESTBOOK, MENU, POLL, Republish, SIGN } from "./pages.ts";

/** The longest line. */
export const MESSAGE_LIMIT = 30;
/** The lines the book keeps, one to a row. */
export const ENTRIES_KEPT = 14;
/** The shortest time between two posts of one session. */
export const POST_INTERVAL_MS = 5_000;
/** The book's first line's row. */
const FIRST_ROW = 4;
/** The field's row on the signing page. */
const FIELD_ROW = 6;
/** Where the field's band starts. */
const BAND_COLUMN = 1;
/** Milliseconds in a minute, minutes in a day and in an hour, for the time of a post. */
const MINUTE_MS = 60_000;
const DAY_MINUTES = 1_440;
const HOUR_MINUTES = 60;
/** Milliseconds in a second, for the wait a viewer is told. */
const SECOND_MS = 1_000;
/** A line's text, after its time and the time's colour. */
const TEXT_COLUMN = 6;
/** The column of the pages' notes. */
const NOTE_COLUMN = 1;
/** The row where the book says it is empty. */
const EMPTY_ROW = 8;

/** One line of the book: the time it was posted, HH:MM UTC, and its text. */
interface Entry {
  readonly time: string;
  readonly text: string;
}

/** What the signing page last said: nothing, Enter with nothing typed, or posted too soon (the seconds left). */
type Note = { readonly kind: "none" } | { readonly kind: "empty" } | { readonly kind: "wait"; readonly seconds: number };

/** One session's line and when it last posted. */
class Signer {
  readonly field = new Field(MESSAGE_LIMIT);
  lastPost: number | null = null;
  note: Note = { kind: "none" };
}

/** The time of day of `epochMs`, HH:MM UTC. */
function timeOfDay(epochMs: number): string {
  const minutes = Math.floor(epochMs / MINUTE_MS) % DAY_MINUTES;
  const two = (value: number): string => value.toString().padStart(2, "0");
  return `${two(Math.floor(minutes / HOUR_MINUTES))}:${two(minutes % HOUR_MINUTES)}`;
}

/** One session's signing page. */
function sign(signer: Signer): Screen {
  const screen = page("Sign the guestbook", Colour.Red, [
    ["Input", MENU],
    ["The book", GUESTBOOK],
    ["", null],
    ["Index", INDEX],
  ]);
  screen.coloured(4, NOTE_COLUMN, Colour.Cyan, `Your message, up to ${MESSAGE_LIMIT} characters:`);
  band(screen, FIELD_ROW, BAND_COLUMN, MESSAGE_LIMIT, Colour.Red, Colour.Yellow, codesOf(signer.field.text()));
  signer.field.cursor(screen, FIELD_ROW, BAND_COLUMN + BAND_LEAD, CursorFlag.Echo);
  if (signer.note.kind === "empty") {
    screen.coloured(8, NOTE_COLUMN, Colour.Red, "Type something first");
  } else if (signer.note.kind === "wait") {
    screen.coloured(8, NOTE_COLUMN, Colour.Red, `Wait ${signer.note.seconds} more seconds to post`);
  }
  screen.coloured(10, NOTE_COLUMN, Colour.White, "Enter posts it to the book, where");
  screen.coloured(11, NOTE_COLUMN, Colour.White, "every viewer sees it at once.");
  screen.coloured(13, NOTE_COLUMN, Colour.White, "Letters, digits and punctuation the");
  screen.coloured(14, NOTE_COLUMN, Colour.White, "page can show are kept; the rest go.");
  screen.coloured(22, NOTE_COLUMN, Colour.Magenta, "Backspace deletes  Esc goes back");
  return screen;
}

/** The book and its signers. */
export class Guestbook {
  /** Newest first. */
  readonly #entries: Entry[] = [];
  readonly #signers = new Map<number, Signer>();
  readonly #republish = new Republish();

  start(context: Context): void {
    context.publish("channel", GUESTBOOK, this.#book());
  }

  /** The signing page as a viewer first sees it, which is also the channel's copy. */
  static firstSignScreen(): Screen {
    return sign(new Signer());
  }

  /** The book's page. */
  #book(): Screen {
    const screen = page("Guestbook", Colour.Red, [
      ["Input", MENU],
      ["Sign it", SIGN],
      ["Poll", POLL],
      ["Index", INDEX],
    ]);
    if (this.#entries.length === 0) {
      screen.centred(EMPTY_ROW, Colour.Yellow, "No messages yet: be the first!");
    }
    this.#entries.forEach((entry, number) => {
      const row = FIRST_ROW + number;
      screen.coloured(row, 0, Colour.Cyan, entry.time);
      screen.coloured(row, TEXT_COLUMN, number % 2 === 0 ? Colour.White : Colour.Yellow, entry.text);
    });
    screen.coloured(19, NOTE_COLUMN, Colour.Green, "Start typing to sign the book");
    screen.coloured(20, NOTE_COLUMN, Colour.White, `One line of ${MESSAGE_LIMIT} characters, one every`);
    screen.coloured(21, NOTE_COLUMN, Colour.White, `${POST_INTERVAL_MS / SECOND_MS} seconds each; newest at the top.`);
    screen.coloured(22, NOTE_COLUMN, Colour.Cyan, "One shared page: every viewer sees it");
    return screen;
  }

  #signer(session: number): Signer {
    const signer = this.#signers.get(session) ?? new Signer();
    this.#signers.set(session, signer);
    return signer;
  }

  #publishSign(session: number, context: Context): void {
    const signer = this.#signers.get(session);
    if (signer !== undefined) {
      context.publish(session, SIGN, sign(signer));
    }
  }

  openedSign(session: number, context: Context): void {
    this.#signer(session);
    this.#publishSign(session, context);
  }

  /** Input on the book or the signing page: both edit the session's line, and input on the book sends the viewer to the signing page. */
  input(session: number, onPage: number, input: Input, sounds: Sounds, context: Context): void {
    const edit = editOf(input);
    const signer = this.#signer(session);
    let effect: Effect | null;
    switch (edit.kind) {
      case "text":
      case "character":
        signer.field.typeText(edit.kind === "text" ? edit.text : edit.character, printable);
        signer.note = { kind: "none" };
        effect = Effect.Click;
        break;
      case "backspace":
        effect = signer.field.backspace() ? Effect.Click : null;
        break;
      case "escape":
        if (onPage === SIGN) {
          context.goto(session, GUESTBOOK);
        }
        return;
      case "enter":
        if (onPage === GUESTBOOK && signer.field.isBlank()) {
          effect = Effect.Click;
          break;
        }
        this.#post(session, sounds, context);
        return;
      case "tab":
      case "other":
        effect = null;
        break;
    }
    if (effect === null) {
      return;
    }
    this.#publishSign(session, context);
    if (onPage === GUESTBOOK) {
      context.goto(session, SIGN);
    }
    sounds.play(session, effect, context);
  }

  /** Posts the session's line to the book, if it may. */
  #post(session: number, sounds: Sounds, context: Context): void {
    const now = context.now();
    const signer = this.#signers.get(session);
    if (signer === undefined) {
      return;
    }
    const since = signer.lastPost === null ? null : Math.max(0, now - signer.lastPost);
    if (signer.field.isBlank()) {
      signer.note = { kind: "empty" };
    } else if (since !== null && since < POST_INTERVAL_MS) {
      signer.note = { kind: "wait", seconds: Math.ceil((POST_INTERVAL_MS - since) / SECOND_MS) };
    } else {
      const text = signer.field.text().trim();
      signer.field.clear();
      signer.lastPost = now;
      signer.note = { kind: "none" };
      this.#entries.unshift({ time: timeOfDay(context.epochMs()), text });
      this.#entries.splice(ENTRIES_KEPT);
      if (this.#republish.changed(context)) {
        context.publish("channel", GUESTBOOK, this.#book());
      }
      this.#publishSign(session, context);
      context.goto(session, GUESTBOOK);
      sounds.play(session, Effect.Bell, context);
      return;
    }
    this.#publishSign(session, context);
    context.goto(session, SIGN);
    sounds.play(session, Effect.Hit, context);
  }

  wake(context: Context): void {
    if (this.#republish.woken(context)) {
      context.publish("channel", GUESTBOOK, this.#book());
    }
  }

  ended(session: number): void {
    this.#signers.delete(session);
  }
}
