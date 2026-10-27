// Text as a teletext row holds it: measured in the cells the English
// option set gives each character (the kit's englishCodes), broken at
// spaces, cut with a mark.

import { englishCodes } from "../kit.ts";

/** The cells `text` takes. */
export function cells(text: string): number {
  return englishCodes(text).length;
}

/** `text` broken at spaces into lines of at most `width` cells; a word longer than a line is cut across lines. */
export function wrap(text: string, width: number): string[] {
  if (width <= 0) {
    return [];
  }
  const lines: string[] = [];
  let line = "";
  for (const found of text.split(/\s+/u)) {
    let word = found;
    if (word === "") {
      continue;
    }
    while (cells(word) > width) {
      if (line !== "") {
        lines.push(line);
        line = "";
      }
      lines.push(word.slice(0, width));
      word = word.slice(width);
    }
    const candidate = line === "" ? word : `${line} ${word}`;
    if (cells(candidate) > width) {
      lines.push(line);
      line = word;
    } else {
      line = candidate;
    }
  }
  if (line !== "") {
    lines.push(line);
  }
  return lines;
}

/** The mark a text cut short ends with, in characters the English set has. */
export const CUT_MARK = "..";

/** `text` cut to `width` cells, ending in the mark when cut. */
export function fit(text: string, width: number): string {
  if (cells(text) <= width) {
    return text;
  }
  return width <= CUT_MARK.length ? text.slice(0, Math.max(0, width)) : `${text.slice(0, width - CUT_MARK.length)}${CUT_MARK}`;
}

/** What ends a headline cut short. */
export const HEADLINE_CUT = "...";

/** A title on at most `most` lines of `width`, the last cut with an ellipsis, whole words first, when it runs over. */
export function headline(title: string, width: number, most: number): string[] {
  const lines = wrap(title, width);
  if (lines.length <= most) {
    return lines;
  }
  const kept = lines.slice(0, most);
  let last = kept[kept.length - 1] ?? "";
  while (last !== "" && cells(`${last}${HEADLINE_CUT}`) > width) {
    last = last.includes(" ") ? last.slice(0, last.lastIndexOf(" ")) : last.slice(0, -1);
  }
  kept[kept.length - 1] = `${last.trimEnd()}${HEADLINE_CUT}`;
  return kept;
}

/** `text` padded with spaces, or cut, to exactly `width` cells. */
export function pad(text: string, width: number): string {
  const shown = fit(text, width);
  return shown + " ".repeat(Math.max(0, width - cells(shown)));
}

/** `text` right-aligned in `width` cells. */
export function padStart(text: string, width: number): string {
  const shown = fit(text, width);
  return " ".repeat(Math.max(0, width - cells(shown))) + shown;
}
