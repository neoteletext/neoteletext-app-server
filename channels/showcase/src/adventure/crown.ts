// Page 402, The Lost Crown: a choose-your-path story in the old teletext
// way, on one page. Each viewer's place in the story is their own
// (SERVER.md section 2: the page is the session's); the coloured keys
// choose (F1 to F4, the remote's colour keys, or r, g, y and c as in the
// engine's viewer), and so does a click on a choice or its prompt. A
// viewer who has not chosen yet sees the shared first node
// (crates/teletext-showcase/src/adventure/crown.rs).

import { FASTEXT_ROW, type Prompts, labelAt, page } from "../common.ts";
import { type Application, Button, COLUMNS, Colour, type Context, type Input, Key, Mode, type PageEntry, type Screen, character, isCharacterKey, mosaicColour, pressed, textColour } from "../kit.ts";
import { ROWS as PICTURE_ROWS } from "./crown/pictures.ts";
import { CROSSROADS, Ending, type Id, NODES, type Node } from "./crown/story.ts";
import { CROWN } from "./pages.ts";
import { TEXT_COLUMN, draw as drawText } from "./text.ts";

/** The story's name, in the page's title band and the contents. */
const TITLE = "The Lost Crown";
/** The row of the node's title, in double height. */
const NODE_TITLE_ROW = 3;
/** The picture's first row. */
const PICTURE_ROW = 5;
/** The picture's width in cells. */
const PICTURE_CELLS = 20;
/** The picture's first column: centred. */
const PICTURE_COLUMN = Math.floor((COLUMNS - PICTURE_CELLS) / 2);
/** The story text's first row. */
export const TEXT_ROW = PICTURE_ROW + PICTURE_ROWS + 1;
/** Rows the story text may take. */
export const TEXT_ROWS = 7;
/** The width of the story text: after the colour attribute in column 0, less a cell's margin at the right. */
export const TEXT_WIDTH = COLUMNS - 2;
/** The first choice's row. */
export const CHOICE_ROW = TEXT_ROW + TEXT_ROWS;
/** The row of the words at the end. */
const THE_END_ROW = CHOICE_ROW;
/** The first choice's row at an ending, below the words. */
export const ENDING_CHOICE_ROW = THE_END_ROW + 2;
/** The hint's row. */
const HINT_ROW = 23;
/** The cells before a double-height title: Double Height and its colour. */
const TITLE_ATTRIBUTES = 2;
/** A mosaic with every sixel set (EN 300 706 Table 47): the key's colour. */
const BLOCK = 0x7f;
/** The Fastext colours, in the order of a node's choices. */
const KEY_COLOURS = [Colour.Red, Colour.Green, Colour.Yellow, Colour.Cyan] as const;
/**
 * The keys of the choices, in their order: each choice's function key,
 * remote colour key (INPUT.md section 5.6) and the engine viewer's letter.
 */
const CHOICE_KEYS: readonly (readonly [functionKey: string, remote: string, letter: string])[] = [
  [Key.F1, Key.Red, "r"],
  [Key.F2, Key.Green, "g"],
  [Key.F3, Key.Yellow, "y"],
  [Key.F4, Key.Blue, "c"],
];
/** The most choices a node has: one a coloured key. */
const MOST_CHOICES = CHOICE_KEYS.length;
/** The cells a picture's background and attributes take before it. */
const PICTURE_ATTRIBUTES = 4;

if (CHOICE_ROW + MOST_CHOICES > HINT_ROW) {
  throw new Error("four choices fit above the hint");
}
if (PICTURE_COLUMN < PICTURE_ATTRIBUTES) {
  throw new Error("a picture's background and attributes fit before it");
}

/** The band's colour: how the node ends, if it does. */
function band(ending: Ending): Colour {
  switch (ending) {
    case Ending.Not:
      return Colour.Magenta;
    case Ending.Won:
      return Colour.Green;
    case Ending.Lost:
      return Colour.Red;
    case Ending.Other:
      return Colour.Blue;
  }
}

/** The node `id`: every node's id is its place in {@link NODES}. */
export function node(id: Id): Node | undefined {
  return NODES[id];
}

/** The labels of `node`'s choices on row 24. They are prompts, not links: a choice is the page's own, made by the channel. */
function labels(shown: Node): readonly [string, string, string, string] {
  const label = (place: number): string => shown.choices[place]?.label ?? "";
  return [label(0), label(1), label(2), label(3)];
}

/** The first row of `node`'s choices. */
function choiceRow(shown: Node): number {
  return shown.ending === Ending.Not ? CHOICE_ROW : ENDING_CHOICE_ROW;
}

/** The page of `node`. */
export function draw(shown: Node): Screen {
  const [red, green, yellow, cyan] = labels(shown);
  const prompts: Prompts = [
    [red, null],
    [green, null],
    [yellow, null],
    [cyan, null],
  ];
  const screen = page(TITLE, band(shown.ending), prompts);
  screen.modes(Mode.MouseButtons);
  const titleWidth = [...shown.title].length + TITLE_ATTRIBUTES;
  screen.doubleHeight(NODE_TITLE_ROW, Math.floor(Math.max(0, COLUMNS - titleWidth) / 2), Colour.Yellow, shown.title);
  shown.picture.draw(screen, PICTURE_ROW, PICTURE_COLUMN);
  const block = { firstRow: TEXT_ROW, rows: TEXT_ROWS, column: TEXT_COLUMN, narrowRows: 0, narrowWidth: 0, width: TEXT_WIDTH };
  drawText(screen, shown.text, block, { plain: Colour.White, highlight: Colour.Yellow });

  if (shown.ending !== Ending.Not) {
    const [words, colour] = theEnd(shown.ending);
    const width = [...words].length + TITLE_ATTRIBUTES;
    screen.doubleHeight(THE_END_ROW, Math.floor(Math.max(0, COLUMNS - width) / 2), colour, words);
  }
  const first = choiceRow(shown);
  shown.choices.forEach((choice, offset) => {
    const colour = KEY_COLOURS[offset];
    if (colour === undefined) {
      return;
    }
    const row = first + offset;
    const marker = [mosaicColour(colour), BLOCK, textColour(Colour.White)];
    screen.codes(row, TEXT_COLUMN, marker);
    screen.text(row, TEXT_COLUMN + marker.length, choice.text);
  });
  screen.centred(HINT_ROW, Colour.Cyan, "Choose with the coloured keys");
  return screen;
}

/** The words at an ending, and their colour. */
function theEnd(ending: Ending): readonly [string, Colour] {
  switch (ending) {
    case Ending.Won:
      return ["You have won", Colour.Green];
    case Ending.Lost:
      return ["The End", Colour.Red];
    case Ending.Not:
    case Ending.Other:
      return ["The End", Colour.Cyan];
  }
}

/** Rust's `char::to_ascii_lowercase`: ASCII capitals lowered, every other character as it is. */
function asciiLowercase(typed: string): string {
  return typed.replace(/[A-Z]/gu, (capital) => capital.toLowerCase());
}

/**
 * The choice `input` makes, 0 to 3, on `node`: a coloured key, or a press
 * of the left button on a choice's line or its prompt.
 */
function chosen(shown: Node, input: Input): number | undefined {
  let place: number | undefined;
  if (input.kind === "key" && !isCharacterKey(input.key) && pressed(input, input.key)) {
    const found = CHOICE_KEYS.findIndex(([functionKey, remote]) => input.key === functionKey || input.key === remote);
    place = found === -1 ? undefined : found;
  } else if (input.kind === "mouse" && input.pressed && input.button === Button.Left) {
    if (input.row === FASTEXT_ROW) {
      place = labelAt(labels(shown), input.column);
    } else {
      const offset = input.row - choiceRow(shown);
      place = offset >= 0 ? offset : undefined;
    }
  } else {
    const typed = character(input);
    if (typed === undefined) {
      return undefined;
    }
    const lowered = asciiLowercase(typed);
    const found = CHOICE_KEYS.findIndex(([, , letter]) => letter === lowered);
    place = found === -1 ? undefined : found;
  }
  return place !== undefined && place < shown.choices.length ? place : undefined;
}

/** The Lost Crown: where each viewer who has chosen is in the story. */
export class Crown implements Application {
  readonly #places = new Map<number, Id>();

  /** Where `session` is in the story. */
  #place(session: number): Id {
    return this.#places.get(session) ?? CROSSROADS;
  }

  // The coloured keys and F1 to F4 choose, so they are the page's; its
  // prompts are not links. Digits select pages, which the Rust story never
  // used.
  pages(): readonly PageEntry[] {
    return [{ page: CROWN, title: TITLE, ownsColourKeys: true }];
  }

  start(context: Context): void {
    // What a viewer sees before they choose.
    const first = node(CROSSROADS);
    if (first !== undefined) {
      context.publish("channel", CROWN, draw(first));
    }
  }

  input(session: number, _page: number, input: Input, context: Context): void {
    const here = node(this.#place(session));
    if (here === undefined) {
      return;
    }
    const place = chosen(here, input);
    const choice = place === undefined ? undefined : here.choices[place];
    if (choice === undefined) {
      return;
    }
    let next: Id;
    if ("node" in choice.to) {
      next = choice.to.node;
    } else {
      // Leaving the story begins it again, as the old pages did: the menu's
      // link led to the first.
      context.goto(session, choice.to.page);
      next = CROSSROADS;
    }
    const shown = node(next);
    if (shown === undefined) {
      return;
    }
    this.#places.set(session, next);
    context.publish(session, CROWN, draw(shown));
  }

  ended(session: number): void {
    this.#places.delete(session);
  }
}
