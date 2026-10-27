// Every page's frame, as Ceefax laid it out: the server's header on row 0,
// a title band in double height on rows 1 and 2 that names the sub-page
// (the browser lists sub-pages by it), the body on rows 3 to 22, a hint of
// the keys on row 23, and the Fastext prompts on row 24 with their links.

import type { Canvas } from "../core/canvas.ts";
import { type Box, Component, type LayoutChild, type Size } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import { SubPage, type SubPageLinks } from "../core/page.ts";
import { cells, fit } from "../core/text.ts";
import { Colour, COLUMNS, type FastextLinks } from "../kit.ts";
import { Rows } from "../layout/layouts.ts";

/** The rows of the frame. */
export const BAND_ROW = 1;

export const BAND_ROWS = 2;

export const BODY_ROW = 3;

export const BODY_ROWS = 20;

export const HINT_ROW = 23;

export const FASTEXT_ROW = 24;

/** The Fastext colours, red to cyan (EN 300 706 clause 9.6). */
export const KEY_COLOURS = [Colour.Red, Colour.Green, Colour.Yellow, Colour.Cyan] as const;

/** The cells between prompts: the next prompt's colour and a space. */
const PROMPT_GAP = 2;

/** One Fastext prompt: its label and the page it goes to, or none. */
export type Prompt = readonly [label: string, page: number | null];

/** The four prompts. */
export type Prompts = readonly [Prompt, Prompt, Prompt, Prompt];

export interface TitleBandProps {
  readonly title: string;
  readonly background: Colour;
  readonly foreground?: Colour;
}

/** The title band: a background of the band's colour, the title in double height. */
export class TitleBand extends Component<TitleBandProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(BAND_ROWS, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.background(0, 0, this.props.background);
    canvas.doubleHeight(0, 2, this.props.foreground ?? Colour.White, fit(this.props.title, canvas.width - 5));
    canvas.background(1, 0, this.props.background);
  }
}

export interface FastextBarProps {
  readonly prompts: Prompts;
}

/** Where each prompt's colour cell is on row 24, or null for an empty prompt. */
export function promptColumns(prompts: Prompts): (number | null)[] {
  let column = 0;
  return prompts.map(([label]) => {
    if (label === "") {
      return null;
    }
    const at = column;
    column += cells(label) + PROMPT_GAP;
    return at;
  });
}

/** Row 24: the four prompts in their colours. Clicking a prompt follows its link (the kit's navigation). */
export class FastextBar extends Component<FastextBarProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const columns = promptColumns(this.props.prompts);
    this.props.prompts.forEach(([label], index) => {
      const column = columns[index];
      const foreground = KEY_COLOURS[index];
      if (column !== null && column !== undefined && foreground !== undefined) {
        canvas.coloured(0, column, foreground, label);
      }
    });
  }
}

export interface LineProps {
  /** What the keys do here; when absent, what the component with the keys says (core/component.ts, hint). */
  readonly text?: string;
  readonly foreground?: Colour;
}

/** Row 23: what the keys do here. */
export class HintLine extends Component<LineProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const text = this.props.text ?? this.focusedHint;
    if (text !== null) {
      canvas.coloured(0, 0, this.props.foreground ?? Colour.Green, fit(text, canvas.width - 1));
    }
  }
}

export interface PositionProps {
  readonly first: number;
  readonly shown: number;
  readonly total: number;
  readonly noun?: string;
}

/** "Lines 1-18 of 66": where a long list or text is. Nothing when everything shows. */
export class PositionLine extends Component<PositionProps> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(1, space.height) };
  }

  override paint(canvas: Canvas): void {
    const { first, shown, total } = this.props;
    if (total > shown) {
      const last = Math.min(total, first + shown);
      canvas.coloured(0, 0, Colour.Cyan, `${this.props.noun ?? "Lines"} ${first + 1}-${last} of ${total}`);
    }
  }
}

export interface FrameProps {
  /** The title band's text: the sub-page's name. */
  readonly title: string;
  readonly background: Colour;
  readonly foreground?: Colour;
  readonly prompts: Prompts;
  readonly hint?: string;
  /** The Index key's page. */
  readonly index?: number;
  /** No title band: the body starts on row 1, for a page with its own heading (the front page's emblem). */
  readonly bare?: boolean;
}

/** A sub-page with the frame: band, body, hint and prompts. Its children are the body. */
export class Frame extends SubPage<FrameProps> {
  override links(): SubPageLinks {
    const fastext = this.props.prompts.map(([, page]) => page) as unknown as FastextLinks;
    return { fastext, index: this.props.index ?? null };
  }

  override render(): Child {
    return [
      this.props.bare === true ? null : h(TitleBand, { key: "band", title: this.props.title, background: this.props.background, ...(this.props.foreground === undefined ? {} : { foreground: this.props.foreground }) }),
      h(Rows, { key: "body" }, ...(this.props.children ?? [])),
      h(HintLine, { key: "hint", ...(this.props.hint === undefined ? {} : { text: this.props.hint }) }),
      h(FastextBar, { key: "prompts", prompts: this.props.prompts }),
    ];
  }

  override arrange(_box: Box, children: readonly LayoutChild[]): void {
    const bare = this.props.bare === true;
    const [first, ...rest] = children;
    const parts = bare ? children : rest;
    if (!bare && first !== undefined) {
      first.place({ row: BAND_ROW, column: 0, width: COLUMNS, height: BAND_ROWS });
    }
    const [body, ...after] = parts;
    const top = bare ? BAND_ROW : BODY_ROW;
    body?.place({ row: top, column: 0, width: COLUMNS, height: BODY_ROW + BODY_ROWS - top });
    const prompts = after[after.length - 1];
    after[0]?.place({ row: HINT_ROW, column: 0, width: COLUMNS, height: 1 });
    prompts?.place({ row: FASTEXT_ROW, column: 0, width: COLUMNS, height: 1 });
  }
}
