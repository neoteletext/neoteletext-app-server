// Game sprites: the sci-fi enemies taken from their gallery pictures, and
// hand-drawn ones for the player and for when there are no pictures
// (crates/teletext-showcase/src/games/sprites.rs).
//
// Each picture is a whole Level 1 page of mosaic graphics showing one
// sprite. The page's rows are read back into sixels as a receiver would
// show them (EN 300 706 clause 12.2, the spacing attributes of Table 26,
// and the block mosaics of Table 47), the sprite is cut out of its page,
// and it is scaled down to a size that leaves room for a wave of them.

// The kit's TTI reader, as PORTING.md section 3.6 reads a sprite.
import { readTti } from "../../../../kits/typescript/src/read-tti.ts";
import { Attribute, COLUMNS, Colour, SIXELS_ACROSS, SIXELS_DOWN, SPACE } from "../kit.ts";
import type { Picture } from "../pictures.ts";
import { COLOURS, MOSAIC_BIT5, SIXEL_BITS } from "./canvas.ts";

/** The picture group of the sci-fi enemy sprites (pictures.ts). */
const SPRITE_GROUP = "s";
/** The picture level read: Level 1.5's rows are the Level 1 page, which is all this reads (the enhancements are not needed for a sprite). */
const SPRITE_LEVEL = "l15";
/** The first and last rows of a picture's page that hold the picture: below the header row 0 and above the Fastext row 24 (EN 300 706 clause 9.3). */
const FIRST_PICTURE_ROW = 1;
const LAST_PICTURE_ROW = 23;
/** Rows 1 to 23. */
const PICTURE_ROW_COUNT = LAST_PICTURE_ROW - FIRST_PICTURE_ROW + 1;
/** The picture's size in sixels. */
const PICTURE_WIDTH = COLUMNS * SIXELS_ACROSS;
const PICTURE_HEIGHT = PICTURE_ROW_COUNT * SIXELS_DOWN;
/**
 * Empty sixel rows that part one piece of a picture from the next. The
 * sprite sheets carry each sprite's name on one row of cells two to five
 * sixel rows under it, and a stray sixel or two; a sprite's own parts may
 * lie up to three rows apart, so the sprite is everything down to its
 * last piece taller than a row of cells (measured on the twenty
 * pictures; see `spriteRows`).
 */
const PIECE_GAP = 2;
/**
 * A scaled sixel is lit when at least this share of the sixels it covers
 * is: an eighth keeps the pictures' thin line art as solid creatures
 * (chosen by eye on the twenty pictures; a third lost most of them).
 */
const LIT_NUMERATOR = 1;
const LIT_DENOMINATOR = 8;
/** The widest sprite, in sixels: a row is sixteen bits. */
export const MAX_WIDTH = 16;
/** The tallest sprite, in sixels. */
export const MAX_HEIGHT = 12;
/** The seven bits of a code (EN 300 706 clause 8.2: the eighth is parity). */
const CODE_MASK = 0x7f;
/** Codes 0/0 to 1/F are spacing attributes (Table 26). */
const ATTRIBUTE_LAST = 0x1f;
/** Alpha White, in which every row starts (clause 12.2). */
const ALPHA_WHITE = 0x07;
/** The colour number in a colour attribute (Table 26). */
const COLOUR_ATTRIBUTE_MASK = 0x07;
/** The bit that makes a colour attribute a mosaic one (Table 26: 0x10 to 0x17). */
const MOSAIC_ATTRIBUTE_BIT = 0x10;
/** The character a sprite is drawn in as text: a lit sixel. */
const LIT_CHARACTER = "#";

/** The number of bits set in `bits`. */
function countOnes(bits: number): number {
  let count = 0;
  for (let rest = bits; rest !== 0; rest &= rest - 1) {
    count += 1;
  }
  return count;
}

/** A small one-colour mosaic sprite. */
export class Sprite {
  /** Its name, for the screens that list the enemies. */
  name: string;
  /** Its width in sixels. */
  readonly width: number;
  /** Its height in sixels. */
  readonly height: number;
  /** Its colour: the colour most of the picture's lit sixels have. */
  readonly colour: Colour;
  /** Its rows, bit `x` the sixel `x` across. */
  readonly rows: readonly number[];

  constructor(name: string, width: number, height: number, colour: Colour, rows: readonly number[]) {
    this.name = name;
    this.width = width;
    this.height = height;
    this.colour = colour;
    this.rows = rows;
  }

  /** A sprite drawn as text, `#` a lit sixel, one string a row; rows and columns past the largest sprite are cut. */
  static drawn(name: string, colour: Colour, art: readonly string[]): Sprite {
    const rows = new Array<number>(MAX_HEIGHT).fill(0);
    let width = 0;
    art.slice(0, MAX_HEIGHT).forEach((line, row) => {
      const characters = [...line];
      characters.slice(0, MAX_WIDTH).forEach((character, x) => {
        if (character === LIT_CHARACTER) {
          rows[row] = (rows[row] ?? 0) | (1 << x);
        }
      });
      width = Math.max(width, Math.min(characters.length, MAX_WIDTH));
    });
    return new Sprite(name, width, Math.min(art.length, MAX_HEIGHT), colour, rows);
  }

  /** Whether the sixel at `x`, `y` is lit; outside the sprite, not. */
  lit(x: number, y: number): boolean {
    const row = this.rows[y];
    return x >= 0 && x < this.width && row !== undefined && ((row >> x) & 1) === 1;
  }

  /** The sprite's lit sixels. */
  count(): number {
    return this.rows.reduce((sum, row) => sum + countOnes(row), 0);
  }
}

/** The enemies: the sci-fi sprites among `pictures`, in name order, each scaled to fit `width` by `height` sixels; hand-drawn ones when there are none. */
export function enemies(pictures: readonly Picture[], width: number, height: number): Sprite[] {
  const chosen = pictures.filter((picture) => picture.group === SPRITE_GROUP);
  // Code unit order, which is the Rust's byte order for the ASCII names.
  chosen.sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0));
  const sprites = chosen.flatMap((picture) => {
    const tti = picture.levels.find(({ level }) => level === SPRITE_LEVEL) ?? picture.levels[0];
    const sprite = tti === undefined ? undefined : extract(tti.tti, width, height);
    if (sprite === undefined) {
      return [];
    }
    sprite.name = title(picture.caption);
    return [sprite];
  });
  return sprites.length === 0 ? drawnEnemies() : sprites;
}

/** A caption such as `01_razor_crawler (sci_fi_enemy_sprites_individual)` as a name: `Razor crawler`. */
function title(caption: string): string {
  const name = (caption.split(" ")[0] ?? "").replace(/^[0-9_]+/u, "");
  return [...name]
    .map((character, index) => {
      if (character === "_") {
        return " ";
      }
      // ASCII letters only, as the Rust's to_ascii_uppercase.
      return index === 0 && character >= "a" && character <= "z" ? character.toUpperCase() : character;
    })
    .join("");
}

/** An explosion. */
export function burst(): Sprite {
  return Sprite.drawn("Burst", Colour.Yellow, ["#..#..#", ".#.#.#.", "..#.#..", ".#.#.#.", "#..#..#"]);
}

/** The hand-drawn enemies, for a channel without the pictures. */
function drawnEnemies(): Sprite[] {
  return [
    Sprite.drawn("Squid", Colour.Magenta, ["...##...", "..####..", ".##..##.", "########", "..#..#..", ".#.##.#."]),
    Sprite.drawn("Crab", Colour.Cyan, [".#....#.", "..#..#..", ".######.", "##.##.##", "########", "#.#..#.#"]),
    Sprite.drawn("Octopus", Colour.Green, ["..####..", ".######.", "##.##.##", "########", ".#.##.#.", "#......#"]),
    Sprite.drawn("Saucer", Colour.Yellow, ["..####..", ".##..##.", "########", ".#.##.#.", "........", "........"]),
    Sprite.drawn("Beetle", Colour.Red, ["#..##..#", ".######.", "##.##.##", ".######.", "#.#..#.#", "#......#"]),
  ];
}

/** The sixels of a picture, each the colour number of its foreground, 0 where it is not lit. */
class PictureSixels {
  readonly colours = new Uint8Array(PICTURE_WIDTH * PICTURE_HEIGHT);

  at(x: number, y: number): number {
    return x < PICTURE_WIDTH ? (this.colours[y * PICTURE_WIDTH + x] ?? 0) : 0;
  }

  set(x: number, y: number, colour: number): void {
    const slot = y * PICTURE_WIDTH + x;
    if (x < PICTURE_WIDTH && slot < this.colours.length) {
      this.colours[slot] = colour;
    }
  }

  /** Lights the sixels of mosaic `code` in the cell at `column`, `row` (Table 47: bits 0 to 4 and 6, top-left first). */
  mosaic(column: number, row: number, code: number, colour: number): void {
    const [x, y] = [column * SIXELS_ACROSS, row * SIXELS_DOWN];
    SIXEL_BITS.forEach((bit, index) => {
      if ((code & bit) !== 0) {
        this.set(x + (index % SIXELS_ACROSS), y + Math.floor(index / SIXELS_ACROSS), colour);
      }
    });
  }
}

/** Whether `code` shows as block mosaic in mosaic mode: bit 5 set (Table 47; codes with bit 5 clear and bit 6 set are blast-through capitals). */
function isBlockMosaic(code: number): boolean {
  return (code & MOSAIC_BIT5) !== 0;
}

/**
 * Reads one row of a page as a Level 1 receiver shows it (EN 300 706
 * clause 12.2): each row starts in alphanumeric white with mosaics
 * released; colour and mode changes are set-after; Hold Mosaics is
 * set-at, and while it holds an attribute's cell shows the last mosaic
 * character instead of a space; a change between alphanumeric and mosaic
 * or of size forgets the held character.
 */
function readRow(codes: readonly number[], row: number, picture: PictureSixels): void {
  let mosaic = false;
  let colour = ALPHA_WHITE;
  let holding = false;
  let held = SPACE;
  for (let column = 0; column < COLUMNS; column += 1) {
    const code = (codes[column] ?? SPACE) & CODE_MASK;
    if (code > ATTRIBUTE_LAST) {
      if (mosaic && isBlockMosaic(code)) {
        picture.mosaic(column, row, code, colour);
        held = code;
      }
      continue;
    }
    if (code === Attribute.HoldMosaics) {
      holding = true;
    }
    if (holding && mosaic) {
      picture.mosaic(column, row, held, colour);
    }
    if ((code & ~COLOUR_ATTRIBUTE_MASK) === 0) {
      if (mosaic) {
        held = SPACE;
      }
      mosaic = false;
      colour = code;
    } else if ((code & ~COLOUR_ATTRIBUTE_MASK) === MOSAIC_ATTRIBUTE_BIT) {
      if (!mosaic) {
        held = SPACE;
      }
      mosaic = true;
      colour = code & COLOUR_ATTRIBUTE_MASK;
    } else if (code === Attribute.NormalHeight || code === Attribute.DoubleHeight) {
      held = SPACE;
    } else if (code === Attribute.ReleaseMosaics) {
      holding = false;
    }
  }
}

/** The first page of `tti`, its rows as a receiver shows them in sixels. */
function read(tti: string): PictureSixels | undefined {
  const page = readTti(tti)[0];
  if (page === undefined) {
    return undefined;
  }
  const picture = new PictureSixels();
  for (let row = 0; row < PICTURE_ROW_COUNT; row += 1) {
    readRow(page.rows[FIRST_PICTURE_ROW + row] ?? [], row, picture);
  }
  return picture;
}

/**
 * The rows `top` to `bottom` (not included) that hold the sprite: from
 * the first lit row to the end of the last piece taller than one row of
 * cells. Pieces are parted by `PIECE_GAP` empty rows; a piece no taller
 * than a row of cells below the sprite is its label or a stray, never the
 * sprite.
 */
function spriteRows(picture: PictureSixels): readonly [top: number, bottom: number] | undefined {
  const litRow = (y: number): boolean => {
    for (let x = 0; x < PICTURE_WIDTH; x += 1) {
      if (picture.at(x, y) !== 0) {
        return true;
      }
    }
    return false;
  };
  let first: number | undefined;
  let bottom: number | undefined;
  let piece: [number, number] | undefined;
  let empty = 0;
  const close = (done: readonly [number, number]): void => {
    if (done[1] - done[0] > SIXELS_DOWN) {
      bottom = done[1];
    }
  };
  for (let y = 0; y < PICTURE_HEIGHT; y += 1) {
    if (!litRow(y)) {
      empty += 1;
      if (empty >= PIECE_GAP && piece !== undefined) {
        close(piece);
        piece = undefined;
      }
      continue;
    }
    empty = 0;
    first ??= y;
    piece = piece === undefined ? [y, y + 1] : [piece[0], y + 1];
  }
  if (piece !== undefined) {
    close(piece);
  }
  return first === undefined || bottom === undefined ? undefined : [first, bottom];
}

/**
 * The sprite on the page of `tti`, cut out and scaled to fit `width` by
 * `height` sixels, keeping its shape; undefined when the page cannot be
 * read or shows nothing.
 */
export function extract(tti: string, width: number, height: number): Sprite | undefined {
  const [boxWidth, boxHeight] = [Math.min(Math.max(width, 1), MAX_WIDTH), Math.min(Math.max(height, 1), MAX_HEIGHT)];
  const picture = read(tti);
  const rows = picture === undefined ? undefined : spriteRows(picture);
  if (picture === undefined || rows === undefined) {
    return undefined;
  }
  const [top, bottom] = rows;
  const litColumns: number[] = [];
  for (let x = 0; x < PICTURE_WIDTH; x += 1) {
    for (let y = top; y < bottom; y += 1) {
      if (picture.at(x, y) !== 0) {
        litColumns.push(x);
        break;
      }
    }
  }
  const left = litColumns[0];
  const last = litColumns[litColumns.length - 1];
  if (left === undefined || last === undefined) {
    return undefined;
  }
  const [sourceWidth, sourceHeight] = [last + 1 - left, bottom - top];
  // Scaled down to fit the box, keeping the shape: the side that fills it
  // first sets the scale. Never up: a sixel cannot be split.
  const [fitWidth, fitHeight] = [Math.min(boxWidth, sourceWidth), Math.min(boxHeight, sourceHeight)];
  const clamp = (value: number, least: number, most: number): number => Math.min(Math.max(value, least), most);
  const share = (numerator: number, denominator: number): number => (denominator === 0 ? 1 : Math.floor(numerator / denominator));
  const [outWidth, outHeight] =
    sourceWidth * fitHeight >= sourceHeight * fitWidth
      ? [fitWidth, clamp(share(sourceHeight * fitWidth, sourceWidth), 1, fitHeight)]
      : [clamp(share(sourceWidth * fitHeight, sourceHeight), 1, fitWidth), fitHeight];
  const counts = new Array<number>(COLOURS.length).fill(0);
  const spriteRowBits = new Array<number>(MAX_HEIGHT).fill(0);
  for (let outY = 0; outY < outHeight; outY += 1) {
    const yFrom = top + Math.floor((outY * sourceHeight) / outHeight);
    const yTo = Math.max(top + Math.floor(((outY + 1) * sourceHeight) / outHeight), yFrom + 1);
    for (let outX = 0; outX < outWidth; outX += 1) {
      const xFrom = left + Math.floor((outX * sourceWidth) / outWidth);
      const xTo = Math.max(left + Math.floor(((outX + 1) * sourceWidth) / outWidth), xFrom + 1);
      let lit = 0;
      let area = 0;
      for (let y = yFrom; y < yTo; y += 1) {
        for (let x = xFrom; x < xTo; x += 1) {
          area += 1;
          const colour = picture.at(x, y);
          if (colour !== 0) {
            lit += 1;
            counts[colour] = (counts[colour] ?? 0) + 1;
          }
        }
      }
      if (lit * LIT_DENOMINATOR >= area * LIT_NUMERATOR && lit > 0) {
        spriteRowBits[outY] = (spriteRowBits[outY] ?? 0) | (1 << outX);
      }
    }
  }
  symmetrical(spriteRowBits, outWidth);
  const sprite = new Sprite("", outWidth, outHeight, dominant(counts), spriteRowBits);
  return sprite.count() > 0 ? sprite : undefined;
}

/**
 * Makes each row its own mirror image: scaled down sixfold, the
 * pictures' line art loses the symmetry their creatures have, and a wave
 * reads as a wave of creatures only with it back.
 */
function symmetrical(rows: number[], width: number): void {
  rows.forEach((row, index) => {
    let mirrored = 0;
    for (let x = 0; x < width; x += 1) {
      if (((row >> x) & 1) === 1) {
        mirrored |= 1 << (width - 1 - x);
      }
    }
    rows[index] = row | mirrored;
  });
}

/**
 * The colour with the most lit sixels. Never black, which has no
 * function as a foreground at Level 1 (the engine's decision E.18), and
 * blue only when nothing else is lit: a small blue sprite is hard to see
 * on black.
 */
function dominant(counts: readonly number[]): Colour {
  let [most, best] = [0, Colour.Blue as Colour];
  COLOURS.forEach((colour, index) => {
    const count = counts[index] ?? 0;
    if (index > 0 && count > most && colour !== Colour.Blue) {
      [most, best] = [count, colour];
    }
  });
  return best;
}
