// The kitchensink's look: each magazine's band, the Fastext prompts, and a
// frame in a magazine's look, which every page of the channel is drawn in.

import { Colour } from "./kit.ts";
import { type Child, type Element, Frame, h, type Prompts } from "../../../kits/typescript-components/src/index.ts";

/** The index: every page's Index key goes here. */
export const INDEX = 0x100;

/** Each magazine's band and the ink on it, as the browser colours magazines. */
export const LOOK = {
  home: { background: Colour.Red, foreground: Colour.Yellow },
  text: { background: Colour.Green, foreground: Colour.Black },
  lists: { background: Colour.Yellow, foreground: Colour.Blue },
  figures: { background: Colour.Cyan, foreground: Colour.Blue },
  graphics: { background: Colour.Magenta, foreground: Colour.White },
  input: { background: Colour.Blue, foreground: Colour.White },
  play: { background: Colour.White, foreground: Colour.Blue },
  articles: { background: Colour.Red, foreground: Colour.White },
} as const;

export type Look = (typeof LOOK)[keyof typeof LOOK];

export function prompts(...given: (readonly [string, number])[]): Prompts {
  const four = [...given, ["", null], ["", null], ["", null], ["", null]] as const;
  return [four[0], four[1], four[2], four[3]] as unknown as Prompts;
}

/** A frame in a magazine's look. */
export function frame(look: Look, title: string, links: Prompts, hint: string | undefined, ...body: Child[]): Element {
  return h(Frame, { title, background: look.background, foreground: look.foreground, prompts: links, index: INDEX, ...(hint === undefined ? {} : { hint }) }, ...body);
}
