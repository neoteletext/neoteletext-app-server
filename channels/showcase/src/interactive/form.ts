// Page 201: a form of two fields, a name and a masked one, each viewer's
// own (crates/teletext-showcase/src/interactive/form.rs). It shows what
// the page asks of the receiver: the cursor record's flags and width,
// which turn the receiver's echo on (INPUT.md section 7.1).

import { INDEX, page } from "../common.ts";
import { Colour, type Context, CursorFlag, type Input, type Screen } from "../kit.ts";
import { Effect, type Sounds } from "../sound.ts";
import { BAND_LEAD, type Edit, Field, band, codesOf, editOf, printable } from "./field.ts";
import { FORM, GUESTBOOK, MENU } from "./pages.ts";

/** The longest name. */
const NAME_LIMIT = 16;
/** The longest secret. */
const SECRET_LIMIT = 16;
/** The character a masked field shows for each one typed, as the receiver's echo does (INPUT.md section 9.3). */
const MASK = "*";
/** The name's row. */
const NAME_ROW = 5;
/** The secret's row. */
const SECRET_ROW = 7;
/** Where each field's band starts. */
const BAND_COLUMN = 11;
/** Where each field's text starts. */
const TEXT_COLUMN = BAND_COLUMN + BAND_LEAD;
/** Where each field's label goes, after the marker of the field with the cursor. */
const LABEL_COLUMN = 3;
/** Where the values of what the page asks go. */
const VALUE_COLUMN = 13;
/** The column of the page's notes. */
const NOTE_COLUMN = 1;

/** Which field has the cursor. */
type Focus = "name" | "secret";

/** What the last Enter did: nothing yet, no name typed, or the greeting with how long the secret was. */
type Answer = { readonly kind: "none" } | { readonly kind: "noName" } | { readonly kind: "greeted"; readonly secretLength: number };

/** One viewer's form. */
class State {
  readonly name = new Field(NAME_LIMIT);
  readonly secret = new Field(SECRET_LIMIT);
  focus: Focus = "name";
  /** Characters the page accepted, to show the echo answered. */
  typed = 0;
  answer: Answer = { kind: "none" };

  focused(): Field {
    return this.focus === "name" ? this.name : this.secret;
  }

  /** Acts on one edit: the sound, if it changed what the page shows. */
  edit(edit: Edit): Effect | null {
    switch (edit.kind) {
      case "text":
        this.typed += this.focused().typeText(edit.text, printable);
        return Effect.Click;
      case "character":
        return this.edit({ kind: "text", text: edit.character });
      case "backspace":
        return this.focused().backspace() ? Effect.Click : null;
      case "tab":
        this.focus = this.focus === "name" ? "secret" : "name";
        return Effect.Click;
      case "escape":
        this.focused().clear();
        return Effect.Click;
      case "enter":
        if (this.name.isBlank()) {
          this.answer = { kind: "noName" };
          this.focus = "name";
          return Effect.Hit;
        }
        this.answer = { kind: "greeted", secretLength: this.secret.length() };
        return Effect.Bell;
      case "other":
        return null;
    }
  }

  screen(): Screen {
    const screen = page("A form", Colour.Blue, [
      ["Input", MENU],
      ["Guestbook", GUESTBOOK],
      ["", null],
      ["Index", INDEX],
    ]);
    screen.coloured(3, NOTE_COLUMN, Colour.Cyan, "Type, then Tab to the next field");
    const masked = MASK.repeat(this.secret.length());
    const fields = [
      [NAME_ROW, "Name", "name", codesOf(this.name.text()), NAME_LIMIT],
      [SECRET_ROW, "Secret", "secret", codesOf(masked), SECRET_LIMIT],
    ] as const;
    for (const [row, label, focus, codes, limit] of fields) {
      const here = this.focus === focus;
      screen.coloured(row, 0, here ? Colour.Yellow : Colour.White, here ? "→" : " ");
      screen.text(row, LABEL_COLUMN, label);
      band(screen, row, BAND_COLUMN, limit, Colour.Blue, Colour.Yellow, codes);
    }
    const [field, row, flags, flagNames] =
      this.focus === "name" ? [this.name, NAME_ROW, CursorFlag.Echo, "ECHO"] : [this.secret, SECRET_ROW, CursorFlag.Echo | CursorFlag.Masked, "ECHO + MASKED"];
    field.cursor(screen, row, TEXT_COLUMN, flags);
    screen.coloured(10, NOTE_COLUMN, Colour.Cyan, "What this page asks of the receiver");
    screen.coloured(11, NOTE_COLUMN, Colour.White, "Cursor:");
    screen.coloured(11, VALUE_COLUMN, Colour.Yellow, `${flagNames}, width ${field.room()}`);
    screen.coloured(13, NOTE_COLUMN, Colour.White, "Accepted:");
    screen.coloured(13, VALUE_COLUMN, Colour.Yellow, `${this.typed} characters`);
    screen.coloured(14, NOTE_COLUMN, Colour.Green, "Your keys echo at once in amber; the");
    screen.coloured(15, NOTE_COLUMN, Colour.Green, "page's own text replaces the echo.");
    if (this.answer.kind === "noName") {
      screen.coloured(17, NOTE_COLUMN, Colour.Red, "Type a name first, then press Enter");
    } else if (this.answer.kind === "greeted") {
      screen.doubleHeight(17, NOTE_COLUMN, Colour.Yellow, `Hello, ${this.name.text().trim()}!`);
      screen.coloured(19, NOTE_COLUMN, Colour.White, `Your secret has ${this.answer.secretLength} characters;`);
      screen.coloured(20, NOTE_COLUMN, Colour.White, "the page only ever showed stars.");
    }
    screen.coloured(22, NOTE_COLUMN, Colour.Magenta, "Tab next  Backspace  Esc clears");
    screen.coloured(23, NOTE_COLUMN, Colour.Magenta, "Enter submits");
    return screen;
  }
}

/** Every viewer's form. */
export class Form {
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
    context.publish(session, FORM, this.#state(session).screen());
  }

  input(session: number, input: Input, sounds: Sounds, context: Context): void {
    const state = this.#state(session);
    const effect = state.edit(editOf(input));
    if (effect !== null) {
      context.publish(session, FORM, state.screen());
      sounds.play(session, effect, context);
    }
  }

  ended(session: number): void {
    this.#sessions.delete(session);
  }
}
