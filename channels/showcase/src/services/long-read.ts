// Page 110, the long read (crates/teletext-showcase/src/services/
// long_read.rs): an article far longer than a page's 25 rows, scrolled
// within the page. Each viewer's place is their own page (SERVER.md
// section 2), published as they scroll; a viewer who has not scrolled sees
// the shared start.
//
// - Keys: Page Down (or Space) and Page Up a screen, keeping one line for
//   context; Down and Up a line; Home and End.
// - The wheel: three lines a notch.
// - The scroll bar, in the last column: a white block of whole cells, the
//   part shown in proportion to the whole, on a thin blue track of mosaic
//   sixels. A cell has one foreground colour (EN 300 706 clause 12.2), so a
//   block that ended part-way through a cell would paint that cell's track
//   white too; the block moves by whole cells. A click above or below the
//   block moves a screen; a press on the block grips it, the pointer's
//   moves with the button held pull it (every move arrives, INPUT.md
//   section 6.3) and the release lets go.
// - Fastext: the services' links. The page takes the mouse, so the
//   receiver sends a click on a prompt here, and the kit follows it.

import { FASTEXT_ROW, page } from "../common.ts";
import {
  type Application,
  Button,
  COLUMNS,
  Held,
  Colour,
  type Context,
  type Input,
  Key,
  type PageEntry,
  SIXELS_ACROSS,
  SIXELS_DOWN,
  Screen,
  Sixels,
  character,
  mosaicColour,
  pressed,
} from "../kit.ts";
import { BODY_COLUMN, FIRST_ROW, right, wrap } from "./draw.ts";
import { LONG_READ, navigation } from "./pages.ts";

/** The page's title. */
const TITLE = "The long read";
/** The title band's colour. */
const BAND = Colour.Blue;
/** The first row of the article. */
export const VIEW_FIRST_ROW = FIRST_ROW;
/** Rows of the article shown at once. */
export const VIEW_ROWS = 19;
/** The row saying which lines are shown. */
export const POSITION_ROW = VIEW_FIRST_ROW + VIEW_ROWS;
/** The row naming the keys. */
const HINT_ROW = POSITION_ROW + 1;
/** The scroll bar's column: the last. */
export const BAR_COLUMN = COLUMNS - 1;
/** The cell before it, which holds its mosaic colour. */
export const BAR_ATTRIBUTE_COLUMN = BAR_COLUMN - 1;
/** The characters of an article line after its colour attribute, ending a cell short of the scroll bar's colour. */
export const TEXT_WIDTH = BAR_ATTRIBUTE_COLUMN - BODY_COLUMN - 2;
/** The bar's height in cells. */
export const TRACK_CELLS = VIEW_ROWS;
/** The shortest block, one cell, so it can always be seen and hit. */
const SHORTEST_THUMB = 1;
/** The sixel column of the bar's thin track: the left of the cell's two. */
const TRACK_SIXEL = 0;
/** Lines a wheel notch scrolls, as desktop browsers do. */
const WHEEL_LINES = 3;
/** A wheel notch in the protocol's units (INPUT.md section 5.4: 1/120ths of a line). */
const NOTCH = 120;
/** A whole, in per cent. */
const PERCENT = 100;

if (HINT_ROW >= FASTEXT_ROW || BODY_COLUMN + 1 + TEXT_WIDTH >= BAR_ATTRIBUTE_COLUMN) {
  throw new Error("the hint is above the Fastext row, with a gap between the text and the bar");
}

/** A part of the article: a heading or a paragraph. */
type Part = { readonly heading: string } | { readonly paragraph: string };

/** The article. Its facts are kept to what is well established about teletext and its standard, ETSI EN 300 706. */
const ARTICLE: readonly Part[] = [
  { heading: "Longer than a page" },
  {
    paragraph:
      "This page is longer than any teletext page could be. A page has 25 rows of 40 characters, and this article runs to well over a hundred lines. The server keeps all of it and sends you the part you are reading, as a page of your own.",
  },
  {
    paragraph:
      "Page Down and Page Up move a screen at a time, the arrow keys a line, and Home and End go to the start and the end. The wheel scrolls too. The bar on the right shows where you are: pull its block, or click above or below it to move a screen.",
  },
  { heading: "The page" },
  {
    paragraph:
      "Row 0 is the header: the page number, the service's name, the date and a clock that keeps running while you wait. Rows 1 to 23 are the page itself. Row 24 shows the Fastext prompts, one for each coloured key.",
  },
  { heading: "Packets" },
  {
    paragraph:
      "Every row travels as a packet of 42 bytes. The first two give the magazine and the row, coded so that a receiver can correct one wrong bit in each. The other 40 are the row's characters: seven bits each, with an eighth for odd parity, so a damaged character can at least be spotted.",
  },
  { heading: "Magazines and page numbers" },
  {
    paragraph:
      "Pages are numbered from 100 to 899. The first digit is the magazine, one of eight. The other two are hexadecimal, so pages such as 1A0 exist, though a remote control with only digits cannot reach them.",
  },
  { heading: "The carousel" },
  {
    paragraph:
      "A broadcaster sends its pages one after another in an endless loop, the carousel. A receiver that wants a page waits for it to come round. On a big service that could take many seconds, which is why later sets kept pages in memory.",
  },
  { heading: "Hidden in the picture" },
  {
    paragraph:
      "The packets ride in the lines at the top of each television frame that a set does not show: the vertical blanking interval. A few of those lines in every field were enough for a whole service.",
  },
  { heading: "Control bits" },
  {
    paragraph:
      "The header carries switches for the page: erase the old copy before showing the new, a newsflash or a subtitle to be boxed into the picture, an update, and others. Every page this server sends asks the receiver to erase the old copy first.",
  },
  { heading: "Spacing attributes" },
  {
    paragraph:
      "At Level 1 every change of colour or style is a control character, and each one takes a cell of the row. Red text, a mosaic, a new background, double height or flashing each cost a space. That is why teletext text so often has a gap before a change of colour.",
  },
  { heading: "Mosaics" },
  {
    paragraph:
      "A mosaic character splits its cell into six blocks, two across and three down, called sixels. Contiguous mosaics fill the cell; separated mosaics leave a thin gap round each block. Pictures, maps and the bar on the right of this page are all drawn with them.",
  },
  { paragraph: "Hold mosaics shows the last mosaic again in the cells that attributes take, so a picture need not have holes where its colours change." },
  { heading: "Fastext" },
  {
    paragraph:
      "Fastext added a packet that links a page to four others, one for each coloured key, and to an index. A receiver that had stored those pages could show them at once, and the coloured prompts on row 24 told you where each key led.",
  },
  { heading: "Higher levels" },
  {
    paragraph:
      "Levels 2.5 and 3.5 added packets that place characters and colours without spending a cell, a palette of 32 colours, and characters a service defines itself. A Level 1 set ignores what it does not understand and still shows the page.",
  },
  { heading: "Ceefax" },
  { paragraph: "The BBC's Ceefax service began in 1974 and closed in 2012." },
  { heading: "NeoTeletext" },
  {
    paragraph:
      "NeoTeletext carries the same pages over the web instead of the air. A channel sends a page when it changes, and the browser shows it as a television would, at whichever level the page was made for.",
  },
  {
    paragraph:
      "Scrolling is new. On the air every viewer saw the same page; here the server can send each viewer their own, so one article can be as long as it needs to be.",
  },
  { heading: "The end" },
  { paragraph: "You have reached the end. Home takes you back to the start, and the Fastext keys lead to the other services." },
];

/** One line of the article as shown: its colour and its text, at most {@link TEXT_WIDTH} characters. */
export interface Line {
  readonly colour: Colour;
  readonly text: string;
}

/** The article laid out: each heading in yellow, each paragraph in white wrapped to the width, a blank line after each paragraph. */
export function lines(): Line[] {
  return ARTICLE.flatMap((part): Line[] =>
    "heading" in part
      ? [{ colour: Colour.Yellow, text: part.heading }]
      : [...wrap(part.paragraph, TEXT_WIDTH).map((text) => ({ colour: Colour.White, text })), { colour: Colour.White, text: "" }],
  );
}

/** `numerator` / `denominator`, rounded to the nearest whole number; 0 for a denominator of 0. */
function rounded(numerator: number, denominator: number): number {
  return denominator === 0 ? 0 : Math.floor((numerator * 2 + denominator) / (denominator * 2));
}

/** The first line shown when the view is at the end. */
function lastTop(total: number): number {
  return Math.max(0, total - VIEW_ROWS);
}

/**
 * Where the scroll bar's block lies, in cells from the top of the track:
 * its first cell and its length. The block's length is the share of the
 * article shown; it reaches the track's top exactly at the first line and
 * its bottom exactly at the last.
 */
export function thumb(top: number, total: number): readonly [start: number, length: number] {
  const shown = Math.min(VIEW_ROWS, total);
  const length = Math.min(TRACK_CELLS, Math.max(SHORTEST_THUMB, rounded(TRACK_CELLS * shown, total)));
  const last = lastTop(total);
  const room = TRACK_CELLS - length;
  return [Math.min(room, rounded(room * Math.min(top, last), last)), length];
}

/** The two cells of the bar: the block, every sixel, and the track, the thin line. */
function barCells(): readonly [full: number, track: number] {
  const cells = new Sixels(SIXELS_ACROSS * 2, SIXELS_DOWN);
  for (let y = 0; y < SIXELS_DOWN; y += 1) {
    for (let x = 0; x < SIXELS_ACROSS; x += 1) {
      cells.set(x, y, true);
    }
    cells.set(SIXELS_ACROSS + TRACK_SIXEL, y, true);
  }
  return [cells.cell(0, 0), cells.cell(SIXELS_ACROSS, 0)];
}

/** Draws the scroll bar for the view from line `top` of `total`. */
function drawBar(screen: Screen, top: number, total: number): void {
  const [start, length] = thumb(top, total);
  const [full, track] = barCells();
  for (let cellRow = 0; cellRow < TRACK_CELLS; cellRow += 1) {
    const inBlock = cellRow >= start && cellRow < start + length;
    const row = VIEW_FIRST_ROW + cellRow;
    screen.code(row, BAR_ATTRIBUTE_COLUMN, mosaicColour(inBlock ? Colour.White : Colour.Blue));
    screen.code(row, BAR_COLUMN, inBlock ? full : track);
  }
}

/**
 * The first line shown when the bar's block is pulled to start at cell
 * `start` of the track: {@link thumb} turned round, the nearest line
 * whose block starts there, held within the article.
 */
export function topAt(start: number, total: number): number {
  const [, length] = thumb(0, total);
  const room = TRACK_CELLS - length;
  const most = lastTop(total);
  if (room <= 0 || most <= 0) {
    return 0;
  }
  return Math.min(most, Math.max(0, rounded(start * most, room)));
}

/** The page with the article from line `top`. */
export function draw(all: readonly Line[], top: number): Screen {
  const screen = page(TITLE, BAND, navigation(LONG_READ));
  const first = Math.min(top, lastTop(all.length));
  all.slice(first, first + VIEW_ROWS).forEach((line, offset) => {
    if (line.text.length > 0) {
      screen.coloured(VIEW_FIRST_ROW + offset, BODY_COLUMN, line.colour, line.text);
    }
  });
  drawBar(screen, first, all.length);
  const last = Math.min(first + VIEW_ROWS, all.length);
  screen.coloured(POSITION_ROW, BODY_COLUMN, Colour.Cyan, `Lines ${first + 1}-${last} of ${all.length}`);
  const percent = all.length === 0 ? PERCENT : Math.floor((last * PERCENT) / all.length);
  right(screen, POSITION_ROW, Colour.Cyan, `${percent}%`);
  screen.coloured(HINT_ROW, BODY_COLUMN, Colour.Green, "PgDn PgUp, arrows, Home, End, wheel");
  return screen;
}

/** One viewer: the first line they see, the wheel's turn not yet a whole line, and the cell of the bar's block they hold, if they hold it. */
interface Reader {
  top: number;
  wheel: number;
  grip: number | null;
}

/** What an input asks of the view: lines down (up when negative), the start, the end, or a line the bar was pulled to. */
type Move = { readonly by: number } | { readonly to: "start" | "end" } | { readonly at: number };

/** A screen's move: one line short of the view, kept for context. */
const SCREENFUL = VIEW_ROWS - 1;

/** The page 110 application. */
export class LongRead implements Application {
  readonly #lines = lines();
  readonly #readers = new Map<number, Reader>();

  pages(): readonly PageEntry[] {
    return [{ page: LONG_READ, title: "The long read" }];
  }

  start(context: Context): void {
    context.publish("channel", LONG_READ, draw(this.#lines, 0));
  }

  /** What `input` asks, from the reader at `top` with its wheel's turn held over. */
  #wanted(input: Input, top: number, reader: Reader): Move | null {
    if (pressed(input, Key.PageDown) || character(input) === " ") {
      return { by: SCREENFUL };
    }
    if (pressed(input, Key.PageUp)) {
      return { by: -SCREENFUL };
    }
    if (pressed(input, Key.Down)) {
      return { by: 1 };
    }
    if (pressed(input, Key.Up)) {
      return { by: -1 };
    }
    if (pressed(input, Key.Home)) {
      return { to: "start" };
    }
    if (pressed(input, Key.End)) {
      return { to: "end" };
    }
    if (input.kind === "wheel") {
      // Whole lines; what is left of a notch waits for the next turn, so
      // a trackpad's small steps add up.
      const turned = (reader.wheel + input.dy) * WHEEL_LINES;
      const moved = Math.trunc(turned / NOTCH);
      reader.wheel = Math.trunc((turned % NOTCH) / WHEEL_LINES);
      return moved === 0 ? null : { by: moved };
    }
    return input.kind === "mouse" ? this.#dragged(input, top, reader) : null;
  }

  /**
   * What the mouse does to the bar: a press on the block grips it where it
   * was pressed and moves with the left button held pull it; a press above
   * or below it turns a screen, as a desktop scroll bar does; the release
   * lets go. Anything else is not the bar's.
   */
  #dragged(input: Input & { kind: "mouse" }, top: number, reader: Reader): Move | null {
    const total = this.#lines.length;
    const cellRow = input.row - VIEW_FIRST_ROW;
    if (input.action === "press") {
      reader.grip = null;
      const onBar = input.button === Button.Left && input.column >= BAR_ATTRIBUTE_COLUMN && cellRow >= 0 && cellRow < VIEW_ROWS && total > VIEW_ROWS;
      return onBar ? this.#barPressed(cellRow, top, reader) : null;
    }
    const grip = reader.grip;
    if (grip === null) {
      return null;
    }
    if (input.action === "move" && (input.buttons & Held.Left) !== 0) {
      return { at: topAt(cellRow - grip, total) };
    }
    reader.grip = null;
    return { by: 0 };
  }

  /** Where a press on the bar's cell `cellRow` takes the view at `top`, gripping the block when it is on it. */
  #barPressed(cellRow: number, top: number, reader: Reader): Move {
    const [start, length] = thumb(top, this.#lines.length);
    if (cellRow < start) {
      return { by: -SCREENFUL };
    }
    if (cellRow >= start + length) {
      return { by: SCREENFUL };
    }
    reader.grip = cellRow - start;
    return { by: 0 };
  }

  /** Answers whether the page took `input`: a click elsewhere than the bar, such as on a Fastext prompt, is the kit's to follow. */
  input(session: number, _page: number, input: Input, context: Context): boolean {
    const reader = this.#readers.get(session) ?? { top: 0, wheel: 0, grip: null };
    this.#readers.set(session, reader);
    const top = reader.top;
    const wanted = this.#wanted(input, top, reader);
    const last = lastTop(this.#lines.length);
    if (wanted === null) {
      return false;
    }
    const next = "by" in wanted ? Math.min(last, Math.max(0, top + wanted.by)) : "at" in wanted ? wanted.at : wanted.to === "start" ? 0 : last;
    if (next !== top) {
      reader.top = next;
      context.publish(session, LONG_READ, draw(this.#lines, next));
    }
    return true;
  }

  /** A viewer who left lets go of the bar: its release went elsewhere. */
  leftPage(session: number): void {
    const reader = this.#readers.get(session);
    if (reader !== undefined) {
      reader.grip = null;
    }
  }

  ended(session: number): void {
    this.#readers.delete(session);
  }
}
