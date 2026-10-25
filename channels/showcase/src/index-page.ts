// Page 100: the showcase's index, with Fastext to its first four sections
// (crates/teletext-showcase/src/index.rs).

import type { Section } from "./area.ts";
import { INDEX, type Prompt, page } from "./common.ts";
import { type Application, Colour, type Context, type PageEntry, pageText } from "./kit.ts";

/** The first row of the list. */
const FIRST_ROW = 4;
/** Rows between sections: the name, then what it holds. */
const ROW_STEP = 2;
/** Where each section's name and what it holds start, after its page number. */
const NAME_COLUMN = 5;
/** The column of each section's page number. */
const NUMBER_COLUMN = 1;

/** The index of `sections`, published once. */
export class Index implements Application {
  readonly #sections: readonly Section[];

  constructor(sections: readonly Section[]) {
    this.#sections = sections;
  }

  pages(): readonly PageEntry[] {
    return [{ page: INDEX, title: "Index" }];
  }

  start(context: Context): void {
    const link = (section: Section | undefined): Prompt => (section === undefined ? ["", null] : [section.name, section.page]);
    const [first, second, third, fourth] = this.#sections;
    const screen = page("NeoTeletext showcase", Colour.Blue, [link(first), link(second), link(third), link(fourth)]);
    let row = FIRST_ROW;
    for (const section of this.#sections) {
      screen.coloured(row, NUMBER_COLUMN, Colour.Yellow, pageText(section.page));
      screen.coloured(row, NAME_COLUMN, Colour.White, section.name);
      screen.coloured(row + 1, NAME_COLUMN, Colour.Cyan, section.about);
      row += ROW_STEP;
    }
    context.publish("channel", INDEX, screen);
  }
}
