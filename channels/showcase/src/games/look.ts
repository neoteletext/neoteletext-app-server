// The games' screens: the frame every game page shares (header, status
// bar, playfield, hint and Fastext rows), banners, and the menu and title
// pages (crates/teletext-showcase/src/games/look.rs).

import { FASTEXT_ROW, INDEX, page } from "../common.ts";
import { Attribute, COLUMNS, Colour, Mode, SIXELS_ACROSS, SIXELS_DOWN, SPACE, type Screen, pageText } from "../kit.ts";
import { Canvas } from "./canvas.ts";
import { BREAKOUT, CAVE, GAMES, Game, HALL, INVADERS, MENU, SNAKE, nameOf, pageOf } from "./pages.ts";
import type { Sprite } from "./sprites.ts";

/** The status bar's row: the title band of other pages. */
export const STATUS_ROW = 1;
/** The playfield's first row and column: rows 2 to 22 and columns 1 to 39, column 0 kept for each row's first colour attribute. */
export const FIELD_ROW = 2;
export const FIELD_COLUMN = 1;
/** The playfield's rows and columns of cells. */
const FIELD_ROWS = 21;
const FIELD_COLUMNS = COLUMNS - FIELD_COLUMN;
/** The playfield's size in sixels: 78 by 63. */
export const FIELD_SIXELS_ACROSS = FIELD_COLUMNS * SIXELS_ACROSS;
export const FIELD_WIDTH = FIELD_SIXELS_ACROSS;
export const FIELD_HEIGHT = FIELD_ROWS * SIXELS_DOWN;
/** The playfield's middle row, where a banner goes by default. */
export const MIDDLE_ROW = 11;
/** The row under the playfield, for the controls. */
const HINT_ROW = 23;
/** The widest Fastext label: the house style puts the four ten cells apart, a colour attribute before each. */
const LABEL_WIDTH = 9;

/** A canvas the size of the playfield. */
export function field(): Canvas {
  return new Canvas(FIELD_WIDTH, FIELD_HEIGHT);
}

/** A canvas as wide as the playfield and `height` sixels tall, for a title page's picture. */
export function art(height: number): Canvas {
  return new Canvas(FIELD_WIDTH, height);
}

/** Each game's colour, for its title band and status bar. */
const BANDS: Readonly<Record<Game, Colour>> = {
  invaders: Colour.Blue,
  snake: Colour.Green,
  breakout: Colour.Magenta,
  cave: Colour.Red,
};

/** Each game's colour, for its title band and status bar. */
export function band(game: Game): Colour {
  return BANDS[game];
}

/** A game's name as a Fastext label. */
function label(game: Game): string {
  const name = nameOf(game);
  return name.length <= LABEL_WIDTH ? name : (name.split(" ")[0] ?? name);
}

/** The game after `game`, for the green link. */
function next(game: Game): Game {
  return GAMES[GAMES.indexOf(game) + 1] ?? Game.Invaders;
}

/**
 * A game page's frame: the house header, title band and Fastext row,
 * asking for every key and its release, which held arrows need (INPUT.md
 * section 6.3, ALL_KEYS and KEY_RELEASE). No cursor: nothing is typed.
 */
export function gamePage(game: Game): Screen {
  const following = next(game);
  const screen = page(nameOf(game), band(game), [
    ["Menu", MENU],
    [label(following), pageOf(following)],
    ["Scores", HALL],
    ["Index", INDEX],
  ]);
  screen.modes(Mode.AllKeys | Mode.KeyRelease);
  return screen;
}

/** A game page in play: the frame with the title band's rows cleared for the status bar and the playfield. */
export function playPage(game: Game): Screen {
  const screen = gamePage(game);
  for (let row = STATUS_ROW; row < FASTEXT_ROW; row += 1) {
    screen.fill(row, 0, COLUMNS, SPACE);
  }
  return screen;
}

/** A status bar item: its colour and text. */
export type Item = readonly [colour: Colour, text: string];

/** The status bar's first item's column, after the band's colour and New Background. */
const STATUS_COLUMN = 2;

/** The status bar: the game's band with `items`, each in its colour. */
export function status(screen: Screen, game: Game, items: readonly Item[]): void {
  screen.background(STATUS_ROW, 0, band(game));
  let column = STATUS_COLUMN;
  for (const [colour, text] of items) {
    screen.coloured(STATUS_ROW, column, colour, text);
    column += [...text].length + 1;
  }
}

/** The controls, under the playfield. */
export function hint(screen: Screen, text: string): void {
  screen.fill(HINT_ROW, 0, COLUMNS, SPACE);
  screen.centred(HINT_ROW, Colour.Cyan, text);
}

/** A banner's rows: the double-height title's two and the line's. */
const BANNER_ROWS = 3;
/** The band's colour and New Background take a banner's first two cells. */
const BAND_CELLS = 2;

/**
 * A banner across the playfield at `row`: `title` in double height on a
 * band of `colour` (rows `row` and `row + 1`, EN 300 706 clause 12.3),
 * and `line` on the row under it.
 */
export function banner(screen: Screen, row: number, colour: Colour, title: string, line: string): void {
  for (let each = row; each < row + BANNER_ROWS; each += 1) {
    screen.fill(each, 0, COLUMNS, SPACE);
  }
  screen.background(row, 0, colour);
  // Double Height and the text's colour take a cell each.
  const width = [...title].length + 2;
  const column = Math.max(Math.floor(Math.max(0, COLUMNS - width) / 2), BAND_CELLS);
  screen.code(row, column, Attribute.DoubleHeight);
  screen.coloured(row, column + 1, Colour.Yellow, title);
  screen.centred(row + BANNER_ROWS - 1, Colour.White, line);
}

/** The digits arcades show a score in. */
const SCORE_DIGITS = 6;

/** A score as six digits, as arcades show them. */
export function digits(score: number): string {
  return score.toString().padStart(SCORE_DIGITS, "0");
}

/** The best score of a game and who made it. */
export type Best = readonly [score: number, player: string];

/** The best line of a title page. */
function bestLine(best: Best | undefined): string {
  return best === undefined ? "No score yet: be the first" : `Best ${digits(best[0])} by ${best[1]}`;
}

/** The first row under a page's title band (the house style's band is rows 1 and 2). */
const BODY_ROW = 4;
/** The menu: an entry every three rows, its page number at column 1, its name and what it is at column 5. */
const MENU_STEP = 3;
const NUMBER_COLUMN = 1;
const NAME_COLUMN = 5;
/** The menu's parade of enemies: five, sixteen sixels apart, on the three rows of cells from row 20. */
const PARADE = 5;
const PARADE_STEP = 16;
const PARADE_ROW = 20;
const PARADE_HEIGHT = 3 * SIXELS_DOWN;
/** Title pages: the picture from row 4, lines of text every two rows from row 14, then how to start and the best score. */
const TEXT_ROW = 14;
const TEXT_STEP = 2;
const CONTROLS_ROW = 20;
const START_ROW = 21;
const BEST_ROW = 22;
/** The invaders' title: a portrait every four rows, its name and worth beside it. */
const PORTRAIT_COLUMN = 3;
const PORTRAIT_STEP = 4;
const PORTRAIT_TEXT_COLUMN = 13;

/** The menu: each game with its page, and enemies across the foot. */
export function menu(enemies: readonly Sprite[]): Screen {
  const screen = page("Action games", Colour.Red, [
    ["Invaders", INVADERS],
    ["Snake", SNAKE],
    ["Breakout", BREAKOUT],
    ["Scores", HALL],
  ]);
  const entries: readonly (readonly [number, string, string])[] = [
    [INVADERS, "Space invaders", "twenty sci-fi foes, wave on wave"],
    [SNAKE, "Snake", "eat, grow, never bite your tail"],
    [BREAKOUT, "Breakout", "a paddle, a ball, five walls"],
    [CAVE, "Cave flyer", "an endless cave, ever narrower"],
    [HALL, "Hall of fame", "every player's best, live"],
  ];
  let row = BODY_ROW;
  for (const [number, name, about] of entries) {
    screen.coloured(row, NUMBER_COLUMN, Colour.Yellow, pageText(number));
    screen.coloured(row, NAME_COLUMN, Colour.White, name);
    screen.coloured(row + 1, NAME_COLUMN, Colour.Cyan, about);
    row += MENU_STEP;
  }
  // The enemies across the foot, a cell or more apart so each keeps its
  // own colour, the row of them centred.
  const parade = art(PARADE_HEIGHT);
  const shown = enemies.slice(0, PARADE);
  const widest = widestOf(shown);
  const span = PARADE_STEP * (shown.length - 1) + widest;
  const left = Math.trunc((FIELD_WIDTH - span) / 2);
  shown.forEach((enemy, index) => {
    const inset = Math.trunc((widest - enemy.width) / 2);
    const x = left + PARADE_STEP * index + inset;
    const y = Math.floor(Math.max(0, PARADE_HEIGHT - enemy.height) / 2);
    parade.sprite(enemy, x, y, enemy.colour);
  });
  parade.draw(screen, PARADE_ROW, FIELD_COLUMN);
  screen.centred(HINT_ROW, Colour.Green, "Your game is yours; scores are shared");
  return screen;
}

/** The widest of `sprites`, in sixels. */
function widestOf(sprites: readonly Sprite[]): number {
  return sprites.reduce((widest, sprite) => Math.max(widest, sprite.width), 0);
}

/** The invaders' title page: the first wave's enemies, large, with their names and points. */
export function invadersTitle(portraits: readonly Sprite[], points: readonly number[], best: Best | undefined): Screen {
  const screen = gamePage(Game.Invaders);
  let row = BODY_ROW;
  const widest = widestOf(portraits);
  const tallest = portraits.reduce((most, portrait) => Math.max(most, portrait.height), 0);
  portraits.slice(0, points.length).forEach((portrait, index) => {
    const picture = new Canvas(widest, tallest);
    const inset = Math.trunc((widest - portrait.width) / 2);
    picture.sprite(portrait, inset, 0, portrait.colour);
    picture.draw(screen, row, PORTRAIT_COLUMN);
    screen.coloured(row, PORTRAIT_TEXT_COLUMN, portrait.colour, portrait.name.toUpperCase());
    screen.coloured(row + 1, PORTRAIT_TEXT_COLUMN, Colour.White, `${points[index] ?? 0} points`);
    row += PORTRAIT_STEP;
  });
  screen.centred(CONTROLS_ROW, Colour.White, "← → move    Space fires");
  screen.centred(START_ROW, Colour.Green, "Press Space to start");
  screen.centred(BEST_ROW, Colour.Yellow, bestLine(best));
  return screen;
}

/** A line of a title page: its colour and text. */
export type Line = readonly [colour: Colour, text: string];

/** A plain title page: `art` drawn from row 4, then `lines`. */
export function title(game: Game, picture: Canvas, lines: readonly Line[], best: Best | undefined): Screen {
  const screen = gamePage(game);
  picture.draw(screen, BODY_ROW, FIELD_COLUMN);
  let row = TEXT_ROW;
  for (const [colour, line] of lines) {
    screen.centred(row, colour, line);
    row += TEXT_STEP;
  }
  screen.centred(START_ROW, Colour.Green, "Press Space to start");
  screen.centred(BEST_ROW, Colour.Yellow, bestLine(best));
  return screen;
}

/** The height of a title page's picture, in sixels: rows 4 to 11. */
export const ART_HEIGHT = 8 * SIXELS_DOWN;
