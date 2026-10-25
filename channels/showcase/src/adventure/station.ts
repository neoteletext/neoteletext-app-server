// Page 401, Station Zero: a typed text adventure on an abandoned space
// station, each viewer's game their own (SERVER.md section 2: the page is
// the session's). A command line near the foot of the page has an echoing
// cursor (INPUT.md section 7.1); Enter acts on what was typed, and the page
// is published again with the reply
// (crates/teletext-showcase/src/adventure/station.rs).

import { INDEX, type Prompts, page } from "../common.ts";
import { type Application, COLUMNS, Colour, type Context, CursorFlag, type Input, Key, type PageEntry, type Screen, englishCodes, pressed, textColour } from "../kit.ts";
import { Effect, Sounds } from "../sound.ts";
import { CROWN, MENU, STATION } from "./pages.ts";
import { Event, Game, MAX_COMMAND, Tone } from "./station/game.ts";
import * as pictures from "./station/pictures.ts";
import { Section, about, directionName, itemName } from "./station/world.ts";
import { type Block, TEXT_COLUMN, draw as drawText } from "./text.ts";

/** Its title. */
const TITLE = "Station Zero";
/** The title band's colour. */
const BAND = Colour.Blue;
/** Where the room's name starts in the title band, after the title. */
const ROOM_NAME_COLUMN = 18;
/** The row of the room's name: the title band's. */
const ROOM_NAME_ROW = 1;
/** The first row of the description, and of the picture beside it. */
const DESCRIPTION_ROW = 4;
/** Rows the description may take. */
export const DESCRIPTION_ROWS = 10;
/** The picture's first column: it runs to the page's right edge. */
const PICTURE_COLUMN = 26;
/** The description's width beside the picture: up to the picture's two attribute cells, less a gap. */
export const NARROW_WIDTH = 22;
/** The exits' row. */
export const EXITS_ROW = 14;
/** The row listing what lies here. */
export const ITEMS_ROW = 15;
/** The row repeating the last command. */
export const SAID_ROW = 17;
/** The reply's first row. */
const REPLY_ROW = 18;
/** Rows the reply may take. */
export const REPLY_ROWS = 4;
/** The command line's row. */
export const COMMAND_ROW = 22;
/** The command line's prompt. */
const PROMPT = ">";
/** Where the prompt goes: after the band's colour and New Background, and the prompt's own colour. */
const PROMPT_COLUMN = 3;
/** Where typed text starts on the command line: after the prompt and the text's colour. */
export const COMMAND_COLUMN = PROMPT_COLUMN + PROMPT.length + 1;
/** The label of the exits. */
const EXITS_LABEL = "Exits:";
/** The label of what lies here. */
const ITEMS_LABEL = "You see:";
/** The status row. */
export const STATUS_ROW = 23;
/** Where the move count starts on the status row. */
const MOVES_COLUMN = 28;
/** The width of text across the page: after the colour attribute in column 0, less a cell's margin at the right. */
export const TEXT_WIDTH = COLUMNS - 2;
/** The cells a picture's attributes take before it: its colour and Hold Mosaics. */
const PICTURE_ATTRIBUTES = 2;

if (COMMAND_COLUMN + MAX_COMMAND >= COLUMNS) {
  throw new Error("the command line holds the longest command and the cursor after it");
}
if (DESCRIPTION_ROW + DESCRIPTION_ROWS > EXITS_ROW) {
  throw new Error("the description ends above the exits");
}
if (REPLY_ROW + REPLY_ROWS > COMMAND_ROW) {
  throw new Error("the reply ends above the command line");
}
if (NARROW_WIDTH + 1 >= PICTURE_COLUMN - PICTURE_ATTRIBUTES) {
  throw new Error("the description stops short of the picture's attributes");
}

/** The page's Fastext links. */
const FASTEXT: Prompts = [
  ["Menu", MENU],
  ["Crown", CROWN],
  ["", null],
  ["Index", INDEX],
];

/** One viewer: their game and the command they are typing. */
interface Player {
  readonly game: Game;
  typed: string;
}

/** The sound of what a command did. */
function effect(event: Event): Effect | undefined {
  switch (event) {
    case Event.Quiet:
      return undefined;
    case Event.Moved:
      return Effect.Click;
    case Event.Took:
      return Effect.Coin;
    case Event.Dropped:
    case Event.Refused:
      return Effect.Hit;
    case Event.Solved:
      return Effect.Bell;
    case Event.Hurt:
      return Effect.Lose;
    case Event.Won:
      return Effect.Win;
    case Event.Restarted:
      return Effect.Jump;
  }
}

/** What Rust's `char::is_control` takes: the Cc category. */
const CONTROL = /^\p{Cc}$/u;

/**
 * Appends typed `text` to the command line, as far as it holds; a line
 * break in a paste ends the command. Answers what is typed and whether a
 * command ended.
 */
function typeInto(typed: string, text: string): readonly [typed: string, ended: boolean] {
  let line = typed;
  for (const character of text) {
    if (character === "\n" || character === "\r") {
      return [line, true];
    }
    if (!CONTROL.test(character) && [...line].length < MAX_COMMAND) {
      line += character;
    }
  }
  return [line, false];
}

/** Station Zero: every viewer's game, and their sounds. */
export class Station implements Application {
  readonly #players = new Map<number, Player>();
  readonly #sounds = new Sounds();

  /** The game of `session`, if it has one. */
  game(session: number): Game | undefined {
    return this.#players.get(session)?.game;
  }

  #publish(session: number, context: Context): void {
    const player = this.#players.get(session);
    if (player !== undefined) {
      context.publish(session, STATION, draw(player.game, player.typed));
    }
  }

  // Typed commands are text, digits among them; the coloured keys follow
  // the page's links, which the Rust game never used.
  pages(): readonly PageEntry[] {
    return [{ page: STATION, title: TITLE, ownsDigits: true }];
  }

  start(context: Context): void {
    // What a viewer sees before they type: a new game, without the cursor,
    // which is their own page's.
    context.publish("channel", STATION, draw(new Game(), undefined));
  }

  opened(session: number, _page: number, context: Context): void {
    if (!this.#players.has(session)) {
      this.#players.set(session, { game: new Game(), typed: "" });
      this.#publish(session, context);
    }
  }

  input(session: number, _page: number, input: Input, context: Context): void {
    let player = this.#players.get(session);
    if (player === undefined) {
      player = { game: new Game(), typed: "" };
      this.#players.set(session, player);
    }
    let event: Event | undefined;
    if (input.kind === "text" || input.kind === "paste") {
      const [typed, ended] = typeInto(player.typed, input.text);
      player.typed = typed;
      if (ended) {
        event = player.game.command(player.typed);
        player.typed = "";
      }
    } else if (pressed(input, Key.Enter)) {
      event = player.game.command(player.typed);
      player.typed = "";
    } else if (pressed(input, Key.Backspace)) {
      player.typed = [...player.typed].slice(0, -1).join("");
    } else if (pressed(input, Key.Escape)) {
      player.typed = "";
    } else {
      return;
    }
    this.#publish(session, context);
    const sound = event === undefined ? undefined : effect(event);
    if (sound !== undefined) {
      this.#sounds.play(session, sound, context);
    }
  }

  ended(session: number): void {
    this.#players.delete(session);
    this.#sounds.forget(session);
  }
}

/** The column after a label written from column 0: past its colour attribute and its text, where the next colour goes. */
function after(label: string): number {
  return TEXT_COLUMN + label.length;
}

/** A list of names, joined. */
function listed(names: readonly string[]): string {
  return names.join(", ");
}

/** The colour of a reply's plain words, by its tone. */
function toneColour(tone: Tone): Colour {
  switch (tone) {
    case Tone.Plain:
      return Colour.White;
    case Tone.Good:
      return Colour.Green;
    case Tone.Bad:
      return Colour.Red;
  }
}

/**
 * The page of `game`, with the command line holding `typed` and the
 * cursor after it; with no `typed`, no cursor.
 */
export function draw(game: Game, typed: string | undefined): Screen {
  const room = about(game.room());
  const screen = page(TITLE, BAND, FASTEXT);
  screen.coloured(ROOM_NAME_ROW, ROOM_NAME_COLUMN, Colour.Yellow, game.won() ? "Homeward" : room.name);

  const section = game.won() ? Section.Docking : room.section;
  pictures.of(section).draw(screen, DESCRIPTION_ROW, PICTURE_COLUMN);
  const description = game.won() ? game.ending() : game.description();
  const block: Block = {
    firstRow: DESCRIPTION_ROW,
    rows: DESCRIPTION_ROWS,
    column: TEXT_COLUMN,
    narrowRows: pictures.ROWS,
    narrowWidth: NARROW_WIDTH,
    width: TEXT_WIDTH,
  };
  drawText(screen, description, block, { plain: Colour.White, highlight: Colour.Yellow });

  if (game.won()) {
    screen.coloured(EXITS_ROW, 0, Colour.Green, `Mission complete in ${game.moves()} moves`);
  } else {
    screen.coloured(EXITS_ROW, 0, Colour.Cyan, EXITS_LABEL);
    screen.coloured(EXITS_ROW, after(EXITS_LABEL), Colour.White, listed(game.exits().map(directionName)));
    const here = game.here().map(itemName);
    if (here.length > 0) {
      screen.coloured(ITEMS_ROW, 0, Colour.Cyan, ITEMS_LABEL);
      screen.coloured(ITEMS_ROW, after(ITEMS_LABEL), Colour.Green, listed(here));
    }
  }

  if (game.said().length > 0) {
    screen.coloured(SAID_ROW, 0, Colour.Cyan, `> ${game.said()}`);
  }
  const reply: Block = { firstRow: REPLY_ROW, rows: REPLY_ROWS, column: TEXT_COLUMN, narrowRows: 0, narrowWidth: 0, width: TEXT_WIDTH };
  drawText(screen, game.reply(), reply, { plain: toneColour(game.tone()), highlight: Colour.Yellow });

  screen.background(COMMAND_ROW, 0, Colour.Blue);
  screen.code(COMMAND_ROW, PROMPT_COLUMN - 1, textColour(Colour.Yellow));
  screen.text(COMMAND_ROW, PROMPT_COLUMN, PROMPT);
  screen.code(COMMAND_ROW, COMMAND_COLUMN - 1, textColour(Colour.White));
  if (typed === undefined) {
    screen.coloured(COMMAND_ROW, COMMAND_COLUMN - 1, Colour.Cyan, "type a command, then Enter");
  } else {
    screen.text(COMMAND_ROW, COMMAND_COLUMN, typed);
    // After what the row shows, which the English set may write in more or
    // fewer codes than the characters typed; never past the last column.
    const column = Math.min(COMMAND_COLUMN + englishCodes(typed).length, COLUMNS - 1);
    screen.cursor(COMMAND_ROW, column, COLUMNS - column, CursorFlag.Echo);
  }

  screen.coloured(STATUS_ROW, 0, Colour.Magenta, "HELP lists the commands");
  screen.coloured(STATUS_ROW, MOVES_COLUMN, Colour.White, `Moves ${game.moves()}`);
  return screen;
}
