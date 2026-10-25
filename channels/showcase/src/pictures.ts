// The showcase's pictures from a folder on the host (decision 6: they
// stay out of git and are copied at deploy by scripts/deploy.sh): the
// engine's levels conversion, NAME_l15.tti, NAME_l25.tti and NAME_l35.tti,
// with captions.tsv giving each name's group and caption
// (crates/teletext-native/src/pictures.rs). The program is given the
// folder with --pictures in its command in channels.json.

import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/** The levels each picture is shown at, as sub-pages, by the names the folder gives them. */
export const LEVELS = ["l15", "l25", "l35"] as const;
/** A picture's level. */
export type Level = (typeof LEVELS)[number];

/** A converted picture: its name (such as e01), its caption with the attribution, its group's letter, and its TTI at each level converted. */
export interface Picture {
  readonly name: string;
  readonly caption: string;
  /** e your examples, s sci-fi sprites, c characters, a art, p people, t travel, g game art. */
  readonly group: string;
  readonly levels: readonly { readonly level: Level; readonly tti: string }[];
}

/** The longest picture read: a page of TTI with every enhancement packet is a few kilobytes, so anything far larger is not a picture. */
const LONGEST_PICTURE = 256 * 1024;
/** The longest captions.tsv read: a few thousand pictures' lines of a few hundred characters. */
const LONGEST_CAPTIONS = 4 * 1024 * 1024;
/** The fields of a captions.tsv line: name, group and caption. */
const CAPTION_FIELDS = 3;

/** A file of the folder as text, or undefined when it is missing or too large. */
function readSmall(path: string, encoding: "utf8" | "latin1", longest: number): string | undefined {
  try {
    if (statSync(path).size > longest) {
      return undefined;
    }
    return readFileSync(path, encoding);
  } catch {
    return undefined;
  }
}

/** The pictures in `folder`; a picture without its files is left out, and no folder is no pictures. */
export function loadPictures(folder: string): Picture[] {
  const captions = readSmall(join(folder, "captions.tsv"), "utf8", LONGEST_CAPTIONS);
  if (captions === undefined) {
    return [];
  }
  const pictures: Picture[] = [];
  for (const line of captions.split("\n")) {
    const fields = line.split("\t");
    if (fields.length < CAPTION_FIELDS) {
      continue;
    }
    const [name = "", group = "", ...caption] = fields;
    // A name is a file name's stem, never a path.
    if (!/^[A-Za-z0-9_-]+$/u.test(name)) {
      continue;
    }
    const levels = LEVELS.flatMap((level) => {
      // TTI files are seven-bit text; latin1 keeps any other byte as one
      // character, for publishTti to refuse.
      const tti = readSmall(join(folder, `${name}_${level}.tti`), "latin1", LONGEST_PICTURE);
      return tti === undefined ? [] : [{ level, tti }];
    });
    if (levels.length > 0) {
      pictures.push({ name, caption: caption.join("\t"), group: group.charAt(0) || "a", levels });
    }
  }
  return pictures;
}
