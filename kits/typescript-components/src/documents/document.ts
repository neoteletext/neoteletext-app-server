// Pages of several sub-pages: a document's lines over named sub-pages
// ("Help 2/4"), an article's home (its text, its picture, its links with a
// QR code, and where else it appears), and headlines a sub-page a day. Each
// renders several Frames; the mount publishes them as one page.

import { Component } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import { RawSubPage } from "../core/page.ts";
import { Frame, type Prompts } from "../frame/frame.ts";
import { Picture } from "../graphics/picture.ts";
import { Colour, COLUMNS } from "../kit.ts";
import { Columns, Gap, Rows } from "../layout/layouts.ts";
import { type NumberedRow, NumberedRows } from "../lists/numbered-rows.ts";
import { articleLines, type Block, type Line, LinesBlock } from "../text/lines.ts";
import { Byline, Headline, Placeholder } from "../text/news.ts";
import { Paragraph } from "../text/styles.ts";

/** Rows of a document's text under its band. */
export const DOCUMENT_ROWS = 20;

/** The text's width across the page: after its colour, less a margin. */
export const TEXT_WIDTH = COLUMNS - 2;

/** A story's first sub-page has its headline, byline and labels over its text. */
const STORY_FIRST_ROWS = 14;

/** A later sub-page has the headline, small, over its text. */
const STORY_MORE_ROWS = 18;

/** `lines` in screens: `first` lines on the first, `rest` on each after. */
export function chunks<T>(lines: readonly T[], first: number, rest: number): T[][] {
  if (lines.length <= first) {
    return [[...lines]];
  }
  const out: T[][] = [lines.slice(0, first)];
  for (let start = first; start < lines.length; start += rest) {
    out.push(lines.slice(start, start + rest));
  }
  return out;
}

/** A sub-page's name: `name`, with its place among `count` when there are several. */
export function partName(name: string, place: number, count: number): string {
  return count === 1 ? name : `${name} ${place}/${count}`;
}

interface Framing {
  readonly background: Colour;
  readonly foreground?: Colour;
  readonly prompts: Prompts;
  readonly index?: number;
}

function framed(framing: Framing, title: string, key: string, ...body: Child[]): Child {
  const { background, foreground, prompts, index } = framing;
  return h(Frame, { key, title, background, prompts, ...(foreground === undefined ? {} : { foreground }), ...(index === undefined ? {} : { index }) }, ...body);
}

export interface DocumentProps extends Framing {
  readonly title: string;
  readonly lines: readonly Line[];
  /** The most sub-pages it takes. */
  readonly maxSubPages?: number;
}

/** Lines over as many sub-pages as they need, each named with its place: "Title 2/3". */
export class Document extends Component<DocumentProps> {
  override render(): Child {
    const parts = chunks(this.props.lines, DOCUMENT_ROWS, DOCUMENT_ROWS).slice(0, this.props.maxSubPages ?? Number.POSITIVE_INFINITY);
    return parts.map((part, index) => framed(this.props, partName(this.props.title, index + 1, parts.length), `part-${index}`, h(LinesBlock, { lines: part })));
  }
}

/** What the kitchensink knows of an article (content.ts). */
export interface Article {
  readonly page: number;
  readonly title: string;
  readonly writer: string;
  readonly date: number;
  readonly summary: string;
  readonly blocks: readonly Block[];
  readonly tags: readonly string[];
  readonly link: string;
}

export interface StoryProps extends Framing {
  readonly article: Article;
  /** The lead picture, as the engine's converter made it, whole-page; none, and the sub-page says so. */
  readonly picture?: string;
  /** The QR code of the article's address, from `neottx qr`. */
  readonly qr?: string;
  /** Where else it appears. */
  readonly where: readonly NumberedRow[];
}

/** An article's home: "Story 1/n" and on, "Picture", "Links", "Where it appears". */
export class StoryPages extends Component<StoryProps> {
  override render(): Child {
    const { article } = this.props;
    const parts = chunks(articleLines(article.blocks, TEXT_WIDTH), STORY_FIRST_ROWS, STORY_MORE_ROWS);
    const text = parts.map((part, index) =>
      framed(
        this.props,
        partName("Story", index + 1, parts.length),
        `story-${index}`,
        index === 0
          ? [h(Headline, { title: article.title, foreground: Colour.Yellow }), h(Byline, { writer: article.writer, date: article.date, labels: article.tags }), h(Gap, {})]
          : [h(Headline, { title: article.title, foreground: Colour.Yellow, lines: 1 }), h(Gap, {})],
        h(LinesBlock, { lines: part }),
      ),
    );
    const picture =
      this.props.picture === undefined
        ? framed(this.props, "Picture", "picture", h(Placeholder, { text: "No picture for this one" }))
        : h(RawSubPage, { key: "picture", tti: this.props.picture, title: "Picture", background: this.props.background, foreground: this.props.foreground ?? Colour.White, fastext: this.props.prompts.map(([, page]) => page) as never });
    const links = framed(this.props, "Links", "links", h(QrPanel, { what: "The full article", title: article.title, ...(this.props.qr === undefined ? {} : { qr: this.props.qr }) }));
    const where = framed(this.props, "Where it appears", "where", h(Paragraph, { text: "This article is listed on these pages:", foreground: Colour.Cyan }), h(Gap, {}), h(NumberedRows, { entries: this.props.where }));
    return [...text, picture, links, where];
  }
}

export interface QrPanelProps {
  readonly qr?: string;
  readonly what: string;
  readonly title: string;
}

/** The QR code's columns: `neottx qr` wrote a 60-character address as rows 2 to 15, 22 cells across (measured 26 October 2026). */
const QR_CELLS = 24;

/** A QR code a phone can follow, and beside it what it leads to. */
export class QrPanel extends Component<QrPanelProps> {
  override render(): Child {
    return h(
      Columns,
      { widths: [QR_CELLS, null] },
      this.props.qr === undefined ? h(Placeholder, { text: "(no QR tool)" }) : h(Picture, { tti: this.props.qr, firstRow: 2 }),
      h(
        Rows,
        {},
        h(Gap, {}),
        h(Paragraph, { text: this.props.what, foreground: Colour.Yellow }),
        h(Gap, {}),
        h(Paragraph, { text: "Scan the code with a phone to open it", foreground: Colour.White }),
        h(Gap, {}),
        h(Paragraph, { text: this.props.title, foreground: Colour.Cyan, lines: 3 }),
      ),
    );
  }
}

/** A day's headlines: its name and its articles. */
export interface Day {
  readonly name: string;
  readonly articles: readonly Article[];
}

export interface DayPagesProps extends Framing {
  readonly days: readonly Day[];
}

/** Headlines a sub-page a day, each sub-page named by its day: "Today", "Yesterday", "Thursday 22 Oct". */
export class DayPages extends Component<DayPagesProps> {
  override render(): Child {
    return this.props.days.map((day, index) =>
      framed(this.props, day.name, `day-${index}`, ...day.articles.map((article) => h(Headline, { key: article.page, title: article.title, page: article.page }))),
    );
  }
}
