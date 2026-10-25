// What every showcase page shares: the header, the title band and the
// Fastext row (crates/teletext-showcase/src/common.rs).

import { Colour, ROWS, Screen } from "./kit.ts";

/** The cells between Fastext labels: the next label's colour code and a space. */
const LABEL_GAP = 2;

/** The row of the Fastext prompts, the page's last (EN 300 706 clause 9.6). */
export const FASTEXT_ROW = ROWS - 1;

/** The service's name in each header. */
export const SERVICE = "NeoTeletext";

/** The index's page, which every page's index link leads to. */
export const INDEX = 0x100;

/** A Fastext prompt: its label, and the page it links to or null. */
export type Prompt = readonly [label: string, page: number | null];
/** The four prompts of row 24: red, green, yellow and cyan. */
export type Prompts = readonly [Prompt, Prompt, Prompt, Prompt];

/** The colours of the four prompts, in order (EN 300 706 clause 9.6). */
const PROMPT_COLOURS = [Colour.Red, Colour.Green, Colour.Yellow, Colour.Cyan] as const;

/**
 * A screen with the showcase's header, its title band on rows 1 and 2,
 * and a Fastext row 24 naming the four coloured links.
 */
export function page(title: string, band: Colour, fastext: Prompts): Screen {
  const screen = new Screen();
  screen.header(SERVICE);
  screen.background(1, 0, band);
  screen.doubleHeight(1, 2, Colour.White, title);
  const columns = labelColumns(labels(fastext));
  fastext.forEach(([label], index) => {
    const span = columns[index];
    const colour = PROMPT_COLOURS[index];
    if (span !== null && span !== undefined && colour !== undefined) {
      screen.coloured(FASTEXT_ROW, span[0], colour, label);
    }
  });
  screen.fastext([fastext[0][1], fastext[1][1], fastext[2][1], fastext[3][1]]);
  screen.indexLink(INDEX);
  return screen;
}

/** The four labels of `prompts`. */
export function labels(prompts: Prompts): readonly [string, string, string, string] {
  return [prompts[0][0], prompts[1][0], prompts[2][0], prompts[3][0]];
}

/**
 * Where each Fastext label goes on row 24: the colour code's column and
 * the column after the label's last character, or null for an empty
 * label. The labels go one after another, each after its colour code, so
 * a long label is never overwritten by the next label's colour.
 */
export function labelColumns(labels: readonly [string, string, string, string]): readonly ([number, number] | null)[] {
  let column = 0;
  return labels.map((label) => {
    if (label.length === 0) {
      return null;
    }
    const characters = [...label].length;
    // The colour code takes the label's first cell.
    const span: [number, number] = [column, column + 1 + characters];
    column += characters + LABEL_GAP;
    return span;
  });
}

/**
 * The Fastext label, 0 to 3, drawn at `column` of row 24 by {@link page}
 * with these labels: where a page that takes the mouse finds which prompt
 * was clicked.
 */
export function labelAt(labels: readonly [string, string, string, string], column: number): number | undefined {
  const found = labelColumns(labels).findIndex((span) => span !== null && column >= span[0] && column < span[1]);
  return found === -1 ? undefined : found;
}
