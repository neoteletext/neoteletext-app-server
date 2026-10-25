// A gallery (crates/teletext-showcase/src/gallery/application.rs): its
// menu, a page for each picture with the levels as sub-pages, the credits
// pages and, on the showcase, the page that compares the levels.
// Everything is decided when the gallery is made, so the contents name
// every page, and published once at start.

import { page } from "../common.ts";
import { type Application, COLUMNS, Colour, type Context, FIRST_PAGE, type PageEntry, type PageLink, type Screen, tti } from "../kit.ts";
import type { Picture } from "../pictures.ts";
import { link, subpageOf, subpages } from "./levels.ts";
import { COLLECTIONS, type Collection, type Group } from "./pages.ts";
import { TITLE_CHARACTERS, contentsTitle, creditLines, cut } from "./text.ts";

/** Where a gallery's pages are, as decimal page numbers: contents.txt names decimal pages only. */
export interface Plan {
  /** The menu. */
  readonly menu: number;
  /** True when the menu is the channel's own index, page 100. */
  readonly channelIndex: boolean;
  /** The first picture's page. */
  readonly firstPicture: number;
  /** The last page a picture may have; pictures beyond are left out. */
  readonly lastPicture: number;
  /** The page that compares the levels, where there is one. */
  readonly levels: number | null;
  /** The first credits page. */
  readonly firstCredits: number;
  /** The last page credits may have. */
  readonly lastCredits: number;
}

/** The showcase channel's gallery, pages 600 to 699. */
export const SHOWCASE_PLAN: Plan = { menu: 600, channelIndex: false, firstPicture: 601, lastPicture: 689, levels: 690, firstCredits: 695, lastCredits: 699 };
/** A collection's channel: its index at 100, pictures from 101 and credits from 800, so at most 800 pages, the engine's limit. */
export const COLLECTION_PLAN: Plan = { menu: 100, channelIndex: true, firstPicture: 101, lastPicture: 799, levels: null, firstCredits: 800, lastCredits: 899 };

/** Decimal digits are written one to a hexadecimal digit in a page number. */
const DECIMAL = 10;
const HUNDREDS = 100;
const NIBBLE_BITS = 4;
const MAGAZINE_SHIFT = 8;

/** Page `decimal`, such as 601, as the kit numbers pages, 0x601. */
export function pageNumber(decimal: number): number {
  return (Math.floor(decimal / HUNDREDS) << MAGAZINE_SHIFT) | ((Math.floor(decimal / DECIMAL) % DECIMAL) << NIBBLE_BITS) | decimal % DECIMAL;
}

/** The credits' first row, under the title band. */
const CREDITS_FIRST_ROW = 4;
/** The credits' last row: row 23 is left clear above the Fastext row. */
const CREDITS_LAST_ROW = 22;
/** The credits' lines on a sub-page. */
const CREDITS_LINES = CREDITS_LAST_ROW - CREDITS_FIRST_ROW + 1;
/** The column of an entry's text colour: after its page number in columns 1 to 3, whose colour is in column 0. */
const CREDITS_COLOUR_COLUMN = 4;
/** An entry's text, after its colour. */
export const CREDITS_WIDTH = COLUMNS - CREDITS_COLOUR_COLUMN - 1;
/** The most lines a caption takes; longer ones are shortened (text.ts, creditLines). */
export const ENTRY_LINES = 6;
/** The most sub-pages of a credits page: the browser shows at most 64 (its MAX_SUBPAGES), and fewer keep each page quick to fill. */
export const CREDITS_SUBPAGES = 40;
/** The column of the text on the gallery's own pages, after its colour. */
const TEXT_COLUMN = 0;
/** The menus' list: the page numbers' colour, then the names'. */
const NAME_COLUMN = 8;
/** The showcase menu's list of the other channels: the count, then the folder. */
const COUNT_COLUMN = 13;
/** The showcase menu's column of each collection's folder, after its count, as the Rust showcase had it. */
export const FOLDER_COLUMN = 27;
// Each folder is named whole on its row, after its colour attribute.
if (COLLECTIONS.some(({ key }) => FOLDER_COLUMN + 1 + [...key].length > COLUMNS)) {
  throw new Error("every collection's folder fits after the menu's folder column");
}
/** The digits of a credits entry's page number. */
const ENTRY_NUMBER_DIGITS = 3;

/** A picture as the gallery shows it. */
interface Shown {
  /** Its page, in decimal. */
  readonly number: number;
  /** Its contents title. */
  readonly title: string;
  /** Its caption as credits lines. */
  readonly lines: readonly string[];
  /** Its levels: the sub-page and the TTI of each, in sub-page order. */
  readonly levels: readonly { readonly subpage: number; readonly tti: string }[];
  /** Its credits page, in decimal, and the sub-page. */
  credit: { readonly page: number; readonly subpage: number } | null;
}

/** A group's pages, for the menu. */
interface Span {
  readonly group: Group;
  readonly first: number;
  readonly last: number;
}

/** A credits page: its number, in decimal, and the pictures on each of its sub-pages. */
interface Credits {
  readonly number: number;
  readonly subpages: number[][];
}

/** A collection on a channel of its own and its size, for the showcase menu. */
export interface Elsewhere {
  readonly collection: Collection;
  readonly count: number;
}

/** One page as published at start: its number and its sub-pages' TTI. */
export interface Published {
  readonly page: number;
  readonly tti: string;
}

/** One gallery: a menu, its pictures and their credits. */
export class Gallery implements Application {
  readonly #title: string;
  readonly #plan: Plan;
  readonly #pictures: Shown[] = [];
  readonly #spans: Span[] = [];
  readonly #credits: Credits[];
  readonly #elsewhere: readonly Elsewhere[];

  /**
   * A gallery called `title` of `groups`, laid out by `plan`; its menu
   * names `elsewhere`, the collections on channels of their own and
   * their sizes. A picture without a level is left out, as are pictures
   * beyond the plan's last page.
   */
  constructor(title: string, plan: Plan, groups: readonly { readonly group: Group; readonly members: readonly Picture[] }[], elsewhere: readonly Elsewhere[]) {
    this.#title = title;
    this.#plan = plan;
    this.#elsewhere = elsewhere;
    let number = plan.firstPicture;
    for (const { group, members } of groups) {
      const first = number;
      for (const picture of members) {
        if (number > plan.lastPicture) {
          break;
        }
        const levels = picture.levels.map(({ level, tti: text }) => ({ subpage: subpageOf(level), tti: text })).sort((a, b) => a.subpage - b.subpage);
        if (levels.length === 0) {
          continue;
        }
        let title = contentsTitle(picture.caption);
        if (title.length === 0) {
          title = cut(picture.name, TITLE_CHARACTERS);
        }
        this.#pictures.push({ number, title, lines: creditLines(picture.caption, CREDITS_WIDTH, ENTRY_LINES), levels, credit: null });
        number += 1;
      }
      if (number > first) {
        this.#spans.push({ group, first, last: number - 1 });
      }
    }
    this.#credits = credits(this.#pictures, plan);
  }

  /** Every page and its sub-pages, as published at start: the kit's page number of each. */
  published(): Published[] {
    const pages: Published[] = [];
    const menu = pageNumber(this.#plan.menu);
    pages.push({ page: menu, tti: shownPage(this.#plan.channelIndex ? this.#collectionMenu() : this.#showcaseMenu(), menu, 0) });
    if (this.#plan.levels !== null) {
      const number = pageNumber(this.#plan.levels);
      pages.push({ page: number, tti: shownPage(this.#levelsPage(), number, 0) });
    }
    this.#pictures.forEach((shown, index) => pages.push({ page: pageNumber(shown.number), tti: this.#picture(index, shown) }));
    for (const credits of this.#credits) {
      pages.push({ page: pageNumber(credits.number), tti: this.#creditsPage(credits) });
    }
    return pages;
  }

  #firstCredits(): number | null {
    const first = this.#credits[0];
    return first === undefined ? null : pageNumber(first.number);
  }

  /** The Fastext of a span's first page, by its short name. */
  #spanLink(position: number): readonly [string, number | null] {
    const span = this.#spans[position];
    return span === undefined ? ["", null] : [span.group.short, pageNumber(span.first)];
  }

  #showcaseMenu(): Screen {
    const levels = this.#plan.levels === null ? null : pageNumber(this.#plan.levels);
    const credits = this.#firstCredits();
    const screen = page(this.#title, Colour.Blue, [this.#spanLink(0), this.#spanLink(1), ["Levels", levels], [credits === null ? "" : "Credits", credits]]);
    screen.coloured(4, TEXT_COLUMN, Colour.Cyan, "Each picture is one page, and its three");
    screen.coloured(5, TEXT_COLUMN, Colour.Cyan, "sub-pages are Levels 1.5, 2.5 and 3.5.");
    let row = 7;
    if (this.#pictures.length === 0) {
      for (const line of ["No pictures are installed here: they", "are copied in when the server is", "deployed, never kept in git."]) {
        screen.coloured(row, TEXT_COLUMN, Colour.White, line);
        row += 1;
      }
      row += 1;
    }
    for (const span of this.#spans) {
      listLine(screen, row, `${span.first}-${span.last}`, span.group.name);
      row += 1;
    }
    if (this.#plan.levels !== null) {
      listLine(screen, row, `${this.#plan.levels}`, "Levels compared");
      row += 1;
    }
    const firstCredits = this.#credits[0];
    if (firstCredits !== undefined) {
      listLine(screen, row, `${firstCredits.number}`, "Picture credits");
      row += 1;
    }
    if (this.#elsewhere.length > 0) {
      row += 1;
      screen.coloured(row, TEXT_COLUMN, Colour.Cyan, "More pictures, each on its own channel:");
      for (const { collection, count } of this.#elsewhere) {
        row += 1;
        screen.coloured(row, TEXT_COLUMN, Colour.White, collection.group.name);
        screen.coloured(row, COUNT_COLUMN, Colour.Cyan, `${count} pictures`);
        screen.coloured(row, FOLDER_COLUMN, Colour.Yellow, collection.key);
      }
    }
    return screen;
  }

  #collectionMenu(): Screen {
    const first = this.#pictures[0]?.number ?? null;
    const last = this.#pictures[this.#pictures.length - 1]?.number ?? null;
    const screen = page(this.#title, Colour.Blue, [
      ["First", first === null ? null : pageNumber(first)],
      ["Last", last === null ? null : pageNumber(last)],
      ["Credits", this.#firstCredits()],
      ["", null],
    ]);
    if (first !== null && last !== null) {
      screen.coloured(4, TEXT_COLUMN, Colour.White, `${this.#pictures.length} pictures, on pages ${first} to ${last}.`);
    }
    screen.coloured(6, TEXT_COLUMN, Colour.Cyan, "Each picture is one page of three");
    screen.coloured(7, TEXT_COLUMN, Colour.Cyan, "sub-pages, one for each level:");
    const levels = [
      ["1", "Level 1.5", "8 colours"],
      ["2", "Level 2.5", "CLUTs 2, 3 redefined"],
      ["3", "Level 3.5", "CLUTs 0 to 3 redefined"],
    ] as const;
    levels.forEach(([subpage, level, about], index) => {
      const row = 8 + index;
      screen.coloured(row, 1, Colour.Yellow, subpage);
      screen.coloured(row, 4, Colour.White, level);
      screen.coloured(row, 15, Colour.Cyan, about);
    });
    screen.coloured(12, TEXT_COLUMN, Colour.White, "Step through the sub-pages to compare.");
    const firstCredits = this.#credits[0];
    if (firstCredits !== undefined) {
      listLine(screen, 14, `${firstCredits.number}`, "Captions and credits");
    }
    ["On a picture, Fastext: red the one", "before, green the next, yellow this", "page, cyan its credit."].forEach((line, index) => {
      screen.coloured(16 + index, TEXT_COLUMN, Colour.Cyan, line);
    });
    return screen;
  }

  #levelsPage(): Screen {
    const screen = page("Levels compared", Colour.Magenta, [
      this.#spanLink(0),
      this.#spanLink(1),
      ["Gallery", pageNumber(this.#plan.menu)],
      ["Credits", this.#firstCredits()],
    ]);
    const lines: readonly (readonly [number, Colour, string])[] = [
      [4, Colour.White, "Every picture page has three sub-pages:"],
      [6, Colour.Yellow, "1  Level 1.5"],
      [7, Colour.White, "   8 colours. Every colour change is a"],
      [8, Colour.White, "   spacing attribute and takes a cell."],
      [10, Colour.Yellow, "2  Level 2.5"],
      [11, Colour.White, "   32 colours: packet X/28/0 redefines"],
      [12, Colour.White, "   CLUTs 2 and 3 for this picture, and"],
      [13, Colour.White, "   X/26 changes colour without a cell."],
      [15, Colour.Yellow, "3  Level 3.5"],
      [16, Colour.White, "   As 2.5, and X/28/4 redefines CLUTs"],
      [17, Colour.White, "   0 and 1 too: all fitted to it."],
      [19, Colour.Cyan, "Step through the sub-pages to compare."],
      [20, Colour.Cyan, "Each picture is 80 x 75 sixels, rows"],
      [21, Colour.Cyan, "0 to 24; its links are on Fastext."],
    ];
    for (const [row, colour, line] of lines) {
      screen.coloured(row, TEXT_COLUMN, colour, line);
    }
    return screen;
  }

  /**
   * The Fastext of the picture at `index`: red the one before, green the
   * next (each wrapping), yellow the menu, cyan its credit at its
   * sub-page, and the index, sixth.
   */
  #pictureLinks(index: number, shown: Shown): (PageLink | null)[] {
    const count = this.#pictures.length;
    const previous = index === 0 ? Math.max(0, count - 1) : index - 1;
    const next = index + 1 >= count ? 0 : index + 1;
    const at = (position: number): PageLink | null => {
      const other = this.#pictures[position];
      return other === undefined ? null : link(pageNumber(other.number), null);
    };
    const credit = shown.credit === null ? null : link(pageNumber(shown.credit.page), shown.credit.subpage);
    return [at(previous), at(next), link(pageNumber(this.#plan.menu), null), credit, null, link(FIRST_PAGE, null)];
  }

  /** A picture's page: its levels as sub-pages, or, when none reads, a page that says so. */
  #picture(index: number, shown: Shown): string {
    const number = pageNumber(shown.number);
    const links = this.#pictureLinks(index, shown);
    const levels = subpages(shown.levels, number, links);
    if (levels !== undefined) {
      return levels;
    }
    const target = (position: number): number | null => links[position]?.page ?? null;
    const screen = page(shown.title, Colour.Red, [
      ["Previous", target(0)],
      ["Next", target(1)],
      ["Menu", target(2)],
      ["Credits", target(3)],
    ]);
    screen.coloured(6, TEXT_COLUMN, Colour.White, "This picture's files could not be read.");
    return shownPage(screen, number, 0);
  }

  #creditsPage(credits: Credits): string {
    const number = pageNumber(credits.number);
    const total = credits.subpages.length;
    const menu = pageNumber(this.#plan.menu);
    const firstPicture = this.#pictures[0];
    const first = firstPicture === undefined ? null : pageNumber(firstPicture.number);
    return credits.subpages
      .map((entries, offset) => {
        const position = offset + 1;
        const screen = page(`Picture credits ${position}/${total}`, Colour.Blue, [
          ["Menu", menu],
          ["First", first],
          ["", null],
          ["", null],
        ]);
        let row = CREDITS_FIRST_ROW;
        entries
          .flatMap((index) => this.#pictures[index] ?? [])
          .forEach((shown, place) => {
            const colour = place % 2 === 0 ? Colour.White : Colour.Cyan;
            screen.coloured(row, TEXT_COLUMN, Colour.Yellow, `${shown.number}`.padStart(ENTRY_NUMBER_DIGITS, "0"));
            const start = row;
            for (const line of shown.lines) {
              screen.coloured(row, CREDITS_COLOUR_COLUMN, colour, line);
              row += 1;
            }
            row = Math.max(row, start + 1);
          });
        return shownPage(screen, number, position);
      })
      .join("");
  }

  pages(): readonly PageEntry[] {
    const entries: PageEntry[] = [{ page: pageNumber(this.#plan.menu), title: this.#title }];
    for (const shown of this.#pictures) {
      entries.push({ page: pageNumber(shown.number), title: shown.title });
    }
    if (this.#plan.levels !== null) {
      entries.push({ page: pageNumber(this.#plan.levels), title: "Levels compared" });
    }
    this.#credits.forEach((credits, position) => {
      entries.push({ page: pageNumber(credits.number), title: position === 0 ? "Picture credits" : `Picture credits ${position + 1}` });
    });
    return entries;
  }

  start(context: Context): void {
    for (const { page: number, tti: text } of this.published()) {
      context.publishTti("channel", number, text);
    }
  }
}

/**
 * `screen` as sub-page `subpage` of page `number`, its sub-code in
 * decimal digits as a picture's are, with its Fastext row shown: a page
 * with links whose X/27/0 does not say "display row 24" has its row 24
 * hidden (EN 300 706 Table 14, the link control byte's bit 4).
 */
function shownPage(screen: Screen, number: number, subpage: number): string {
  screen.showFastextRow();
  return tti(number, [{ screen, subpage }]);
}

/** A line of a menu's list: a page number in yellow, a name in white. */
function listLine(screen: Screen, row: number, number: string, name: string): void {
  screen.coloured(row, TEXT_COLUMN, Colour.Yellow, number);
  screen.coloured(row, NAME_COLUMN, Colour.White, name);
}

/**
 * Places each picture's caption on a credits sub-page, filling each in
 * turn, and gives each picture its credit; the pages run from the plan's
 * first credits page, each with at most {@link CREDITS_SUBPAGES}.
 */
function credits(pictures: Shown[], plan: Plan): Credits[] {
  const pages: Credits[] = [];
  let used = CREDITS_LINES;
  for (const [index, shown] of pictures.entries()) {
    const lines = Math.min(Math.max(shown.lines.length, 1), CREDITS_LINES);
    if (used + lines > CREDITS_LINES) {
      const last = pages[pages.length - 1];
      if (last === undefined || last.subpages.length >= CREDITS_SUBPAGES) {
        const number = last === undefined ? plan.firstCredits : last.number + 1;
        if (number > plan.lastCredits) {
          break;
        }
        pages.push({ number, subpages: [] });
      }
      pages[pages.length - 1]?.subpages.push([]);
      used = 0;
    }
    const current = pages[pages.length - 1];
    const subpage = current?.subpages[current.subpages.length - 1];
    if (current !== undefined && subpage !== undefined) {
      subpage.push(index);
      shown.credit = { page: current.number, subpage: current.subpages.length };
    }
    used += lines;
  }
  return pages;
}
