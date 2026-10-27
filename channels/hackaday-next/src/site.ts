// Where everything is. Magazine 1 is the archive's home: its front, the
// latest, search, the sections, tags, writers, on this day, the years and
// the figures (100 to 108), the pages that show one of many (110 to 113:
// an article, a writer, a tag, a day), the podcast (120 to 128) and what
// this is (190). Magazines 2 to 8 hold the 36 areas: each magazine's front
// (N00), its latest (N01) and its podcast stories (N02), then from N10 each
// area's page followed by a page for each of its subjects. Pages are
// numbered in decimal, as a viewer keys them.

import { Colour } from "./kit.ts";
import type { Library } from "./library.ts";

/** The page a viewer keys as `decimal`. */
export function page(decimal: number): number {
  return Number.parseInt(String(decimal), 16);
}

/** The decimal a page number is keyed as. */
export function decimal(number: number): number {
  return Number.parseInt(number.toString(16), 10);
}

export const PAGES = {
  front: page(100),
  latest: page(101),
  search: page(102),
  sections: page(103),
  tags: page(104),
  writers: page(105),
  onThisDay: page(106),
  years: page(107),
  figures: page(108),
  reader: page(110),
  writer: page(111),
  tag: page(112),
  day: page(113),
  podcast: page(120),
  episodes: page(121),
  hosts: page(122),
  storyFinder: page(123),
  episode: page(125),
  story: page(126),
  player: page(127),
  rounds: page(128),
  about: page(190),
} as const;

/** A magazine's look: its band and the foreground on it. */
export interface Look {
  readonly background: Colour;
  readonly foreground: Colour;
}

export const HOME_LOOK: Look = { background: Colour.Yellow, foreground: Colour.Black };
export const PODCAST_LOOK: Look = { background: Colour.Magenta, foreground: Colour.White };
export const GAME_LOOK: Look = { background: Colour.Cyan, foreground: Colour.Blue };

/** Magazines 2 to 8: their names, looks and areas (the analysis's area numbers). */
export const MAGAZINES: readonly { readonly number: number; readonly name: string; readonly look: Look; readonly areas: readonly number[] }[] = [
  { number: 2, name: "Making", look: { background: Colour.Green, foreground: Colour.Black }, areas: [1, 2, 3, 23, 24, 30, 32, 33] },
  { number: 3, name: "Electronics", look: { background: Colour.Cyan, foreground: Colour.Blue }, areas: [4, 5, 6, 7, 8, 9, 10] },
  { number: 4, name: "Computers", look: { background: Colour.Blue, foreground: Colour.White }, areas: [11, 13, 14, 15, 16, 17] },
  { number: 5, name: "Security and Radio", look: { background: Colour.Red, foreground: Colour.White }, areas: [12, 18, 19] },
  { number: 6, name: "Machines and Space", look: { background: Colour.Magenta, foreground: Colour.White }, areas: [20, 25, 26] },
  { number: 7, name: "Science and Home", look: { background: Colour.White, foreground: Colour.Blue }, areas: [21, 22, 28, 36] },
  { number: 8, name: "Sound and Play", look: { background: Colour.Yellow, foreground: Colour.Blue }, areas: [27, 29, 31, 34, 35] },
];

/** The first page of the areas and subjects in each magazine: N10. */
const SECTION_START = 10;
/** A magazine's last page: N99. */
const SECTION_END = 99;
/** The pages before the areas: front, latest, podcast stories. */
export const MAGAZINE_FRONT = 0;
export const MAGAZINE_LATEST = 1;
export const MAGAZINE_PODCAST = 2;

/** Each area's and subject's page, and which magazine each area is in. */
export class SiteMap {
  readonly areaPage = new Map<number, number>();
  readonly subjectPage = new Map<number, number>();
  readonly magazineOf = new Map<number, (typeof MAGAZINES)[number]>();

  constructor(library: Library) {
    for (const magazine of MAGAZINES) {
      let next = magazine.number * 100 + SECTION_START;
      for (const areaId of magazine.areas) {
        const area = library.areas.find((each) => each.id === areaId);
        if (area === undefined) {
          continue;
        }
        this.magazineOf.set(area.id, magazine);
        this.areaPage.set(area.id, page(next));
        next += 1;
        for (const subject of area.subjects) {
          this.subjectPage.set(subject.id, page(next));
          next += 1;
        }
      }
      if (next - 1 > magazine.number * 100 + SECTION_END) {
        throw new Error(`magazine ${magazine.number} needs pages past ${magazine.number}99`);
      }
    }
  }

  /** A magazine's own page: its front, latest or podcast. */
  static magazinePage(magazine: number, which: number): number {
    return page(magazine * 100 + which);
  }
}
