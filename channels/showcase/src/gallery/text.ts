// Captions as teletext can show them (crates/teletext-showcase/src/
// gallery/text.rs): folded to the English national option set, wrapped
// to a width, shortened to fit, and cut to the 32 characters of a
// contents.txt title.

import { LONGEST_TITLE, UNSHOWN, foldCharacter } from "../kit.ts";

/** The longest title contents.txt keeps (the engine's contents rule). */
export const TITLE_CHARACTERS = LONGEST_TITLE;
/** What marks a cut. */
const ELLIPSIS = "..";
/** The separator between a caption's parts in captions.tsv: title, author, licence (scripts/pictures.py, from the levels page). */
const PART_SEPARATOR = "·";
/** The most of a contents title the author takes, so the picture's own title keeps the rest. */
const AUTHOR_IN_TITLE = 14;
/** The extensions of the source files' names, left out of the titles. */
const EXTENSIONS = [".jpg", ".jpeg", ".png", ".gif", ".tif", ".tiff", ".webp"] as const;
/**
 * What joins the words of a licence, such as "CC BY-SA 4.0", while
 * wrapping, so that it is never broken over two lines: a control
 * character, which no folded caption holds and {@link wrap} does not
 * break at.
 */
const GLUE = "\u001F";
/** How much shorter the author becomes at each try to fit. */
const AUTHOR_STEP = 8;
/** What joins a caption's parts in a credit, and a contents title's. */
const JOIN = ", ";
/** Whitespace as Rust's char::is_whitespace has it: Unicode's White_Space property. */
const WHITESPACE = /^\p{White_Space}$/u;
/** Whitespace as JavaScript's \s has it, which the kit folds to a space: U+FEFF is not White_Space, so the Rust showed it as unshown. */
const SCRIPT_WHITESPACE = /^\s$/u;
/** Runs of whitespace, as Rust's split_whitespace splits at them. */
const WHITESPACE_RUNS = /\p{White_Space}+/u;
/** Combining diacritical marks, U+0300 to U+036F: left out, the letter before shown without them. */
const COMBINING_FIRST = 0x0300;
const COMBINING_LAST = 0x036f;
/** The printable ASCII range a fold may give, and the pound sign the English set has at 0x23 (EN 300 706 Table 36). */
const FIRST_PRINTABLE = " ";
const LAST_PRINTABLE = "~";
const POUND = "£";
/**
 * Where the gallery's fold differs from the kit's: the Rust gallery
 * folded the em dash to a hyphen, where the kit shows the English set's
 * own dash (Table 36, 0x60).
 */
const GALLERY_FOLDS: ReadonlyMap<string, string> = new Map([["—", "-"]]);

/** Whether `text` is what the Rust runtime's Screen::text showed as itself: printable ASCII, and the pound sign. */
function showable(text: string): boolean {
  return [...text].every((character) => (character >= FIRST_PRINTABLE && character <= LAST_PRINTABLE) || character === POUND);
}

/**
 * One character as the gallery shows it: the kit's fold (EN 300 706
 * Table 36) where that is printable ASCII or the pound sign, so the
 * English set's arrows and fractions, which the Rust gallery could not
 * show, stay unshown; undefined for what it cannot show. The kit folds
 * five dashes and quotes the Rust left unshown (U+2011, U+2012, U+2212,
 * U+201A and U+2033); no caption holds one.
 */
function shownAs(character: string): string | undefined {
  const own = GALLERY_FOLDS.get(character);
  if (own !== undefined) {
    return own;
  }
  if (SCRIPT_WHITESPACE.test(character)) {
    return undefined;
  }
  const folded = foldCharacter(character);
  return folded !== undefined && showable(folded) ? folded : undefined;
}

/**
 * `text` in printable ASCII but for the codes the English set uses for
 * other signs, and the pound sign. Whitespace runs become one space; a
 * run of letters of other scripts becomes one `?`.
 */
export function fold(text: string): string {
  let out = "";
  let unshown = false;
  for (const character of text) {
    const point = character.codePointAt(0) ?? 0;
    if (WHITESPACE.test(character)) {
      if (!out.endsWith(" ") && out.length > 0) {
        out += " ";
      }
      unshown = false;
    } else if (point >= COMBINING_FIRST && point <= COMBINING_LAST) {
      // A mark on the letter before, which is shown without it.
    } else {
      const shown = shownAs(character);
      if (shown !== undefined) {
        out += shown;
        unshown = false;
      } else if (!unshown) {
        out += UNSHOWN;
        unshown = true;
      }
    }
  }
  return out.trimEnd();
}

/** `text` cut to `limit` characters, the cut marked with {@link ELLIPSIS}. */
export function cut(text: string, limit: number): string {
  const characters = [...text];
  if (characters.length <= limit) {
    return text;
  }
  const kept = limit - [...ELLIPSIS].length;
  if (kept < 0) {
    return characters.slice(0, limit).join("");
  }
  return characters.slice(0, kept).join("").trimEnd() + ELLIPSIS;
}

/** `text` wrapped into lines of at most `width` characters, at spaces; a word longer than a line is broken. */
export function wrap(text: string, width: number): string[] {
  const lines: string[] = [];
  if (width === 0) {
    return lines;
  }
  let line = "";
  let length = 0;
  for (const word of text.split(WHITESPACE_RUNS).filter((piece) => piece.length > 0)) {
    let rest = [...word];
    while (rest.length > 0) {
      const size = rest.length;
      const needed = length === 0 ? size : length + 1 + size;
      if (needed <= width) {
        if (length > 0) {
          line += " ";
        }
        line += rest.join("");
        length = needed;
        break;
      }
      if (length > 0) {
        lines.push(line);
        line = "";
        length = 0;
        continue;
      }
      lines.push(rest.slice(0, width).join(""));
      rest = rest.slice(width);
    }
  }
  if (length > 0) {
    lines.push(line);
  }
  return lines;
}

/** A caption's parts: its title, without the file's extension, and the rest (author and licence, where the caption names them), each folded. */
export function parts(caption: string): string[] {
  const folded = caption
    .split(PART_SEPARATOR)
    .map(fold)
    .filter((part) => part.length > 0);
  const [title, ...rest] = folded;
  if (title === undefined) {
    return folded;
  }
  // ASCII letters only, as Rust's to_ascii_lowercase, so no length changes.
  const lower = title.replace(/[A-Z]/gu, (letter) => letter.toLowerCase());
  const extension = EXTENSIONS.find((candidate) => lower.endsWith(candidate));
  return [extension === undefined ? title : title.slice(0, title.length - extension.length), ...rest];
}

/** A caption as one line of reading: "title, by author, licence" where it names them, its parts joined with commas otherwise. */
function creditText(captionParts: readonly string[], titleLimit: number, authorLimit: number): string {
  const [first, ...rest] = captionParts;
  const title = first === undefined ? "" : cut(first, titleLimit);
  const [author, licence] = rest;
  if (rest.length === 2 && author !== undefined && licence !== undefined) {
    return `${title}, by ${cut(author, authorLimit)}, ${glued(licence)}`;
  }
  return [title, ...rest].join(JOIN);
}

/** `text` with its spaces glued. */
function glued(text: string): string {
  return text.replaceAll(" ", GLUE);
}

/** `lines` with their glued spaces as spaces again. */
function unglued(lines: readonly string[]): string[] {
  return lines.map((line) => line.replaceAll(GLUE, " "));
}

/**
 * A caption as the lines of a credits entry, at most `maxLines` of
 * `width`: whole where it fits; otherwise its title and then its author
 * are shortened, and the licence is always kept whole.
 */
export function creditLines(caption: string, width: number, maxLines: number): string[] {
  const captionParts = parts(caption);
  const longest = Math.max(0, ...captionParts.map((part) => [...part].length));
  const whole = wrap(creditText(captionParts, longest, longest), width);
  if (whole.length <= maxLines) {
    return unglued(whole);
  }
  // The title keeps two lines; the author what is left.
  const titleLimit = width * 2;
  let authorLimit = longest;
  for (;;) {
    const lines = wrap(creditText(captionParts, titleLimit, authorLimit), width);
    if (lines.length <= maxLines || authorLimit === 0) {
      return unglued(lines.slice(0, maxLines));
    }
    authorLimit = Math.max(0, authorLimit - AUTHOR_STEP);
  }
}

/** A caption as a contents.txt title of at most 32 characters: the picture's title and, where the caption names one, its author. */
export function contentsTitle(caption: string): string {
  const captionParts = parts(caption);
  const title = captionParts[0] ?? "";
  const author = captionParts[1];
  if (captionParts.length === 3 && author !== undefined) {
    const shortAuthor = cut(author, AUTHOR_IN_TITLE);
    const room = Math.max(0, TITLE_CHARACTERS - [...shortAuthor].length - JOIN.length);
    return `${cut(title, room)}${JOIN}${shortAuthor}`;
  }
  // A caption of one part ends with its source's folder in brackets,
  // which says nothing a viewer needs.
  const bracket = title.lastIndexOf(" (");
  const plain = title.endsWith(")") && bracket !== -1 ? title.slice(0, bracket) : title;
  return cut(plain, TITLE_CHARACTERS);
}
