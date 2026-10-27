// Page 110, the reader: one article, the one the viewer was sent here with
// (the library's page parameters). The coloured keys choose what of it to
// see, as Fastext's prompts say: red its text, which scrolls; green what it
// is (its summary, writer, subjects and tags); yellow the podcast stories
// that talk about it; cyan its links.

import { Colour } from "../kit.ts";
import {
  articleLines,
  BAR_CELLS,
  Badges,
  Button,
  type Child,
  colourKeyOf,
  Columns,
  Control,
  type ControlProps,
  Gap,
  h,
  type KeyEvent,
  type Line,
  LinesBlock,
  listStep,
  type PageNeeds,
  type PageSpec,
  Paragraph,
  Placeholder,
  Rows,
  ScrollBar,
  Table,
  TabStrip,
  Text,
  WheelLines,
  type WheelEvent,
  NowPlaying,
  DATE_FORMATS,
  formatDate,
} from "../../../../kits/typescript-components/src/index.ts";
import type { Article, Story } from "../library.ts";
import { frame, prompts, type Site, StoryList } from "../parts.ts";
import { HOME_LOOK, PAGES } from "../site.ts";

const TABS = ["Text", "About", "Podcast", "Links"] as const;
const TEXT = 0;
const ABOUT = 1;
const PODCAST = 2;
const LINKS = 3;
/** The rows the text shows, under the tabs. */
const TEXT_ROWS = 18;
/** The text's width: the page less its colour cell and the scroll bar. */
const TEXT_WIDTH = 40 - 1 - BAR_CELLS - 1;

interface ReaderProps extends ControlProps {
  readonly site: Site;
  readonly article: Article;
}

interface ReaderState {
  readonly tab: number;
  readonly top: number;
}

class Reader extends Control<ReaderProps, ReaderState> {
  override state: ReaderState = { tab: TEXT, top: 0 };
  readonly #wheel = new WheelLines();
  #lines: Line[] | null = null;

  protected override keyNeeds(): PageNeeds {
    return { colourKeys: true };
  }

  protected override keysHint(): string {
    return this.state.tab === TEXT ? "Up Down PgUp PgDn scroll; colours choose" : "The coloured keys choose what to see";
  }

  /** On the podcast tab its list has the keys; the colours still come here. */
  override focusable(): boolean {
    return this.state.tab !== PODCAST || this.#stories().length === 0;
  }

  #stories(): readonly Story[] {
    return this.props.site.library.storiesOf.get(this.props.article.id) ?? [];
  }

  #text(): Line[] {
    if (this.#lines === null) {
      const { article } = this.props;
      const blocks = article.text
        .split("\n")
        .filter((line) => line.trim() !== "")
        .map((text) => ({ kind: "paragraph" as const, text }));
      this.#lines = [
        ...articleLines([{ kind: "heading", text: article.title }], TEXT_WIDTH),
        { foreground: Colour.Cyan, text: `${this.props.site.library.writers.get(article.writer)?.name ?? ""}, ${formatDate(article.date, DATE_FORMATS.long)}` },
        { foreground: Colour.White, text: "" },
        ...articleLines(blocks, TEXT_WIDTH),
      ];
    }
    return this.#lines;
  }

  #choose(tab: number): void {
    this.setState({ tab, top: 0 });
    if (tab !== PODCAST) {
      this.focus();
    }
  }

  override render(): Child {
    const { site, article } = this.props;
    const { library, map } = site;
    const stories = this.#stories();
    const strip = h(TabStrip, { tabs: TABS.map((tab, index) => (index === PODCAST ? `${tab} ${stories.length}` : tab)), active: this.state.tab, style: "fastext", onChange: (tab: number) => this.#choose(tab) });
    switch (this.state.tab) {
      case TEXT: {
        const lines = this.#text();
        const top = Math.min(this.state.top, Math.max(0, lines.length - TEXT_ROWS));
        return h(
          Rows,
          {},
          strip,
          h(Columns, { widths: [null, BAR_CELLS], height: TEXT_ROWS }, h(LinesBlock, { lines: lines.slice(top, top + TEXT_ROWS) }), h(ScrollBar, { top, total: lines.length, shown: TEXT_ROWS, onScroll: (to) => this.setState({ top: to }) })),
        );
      }
      case ABOUT: {
        const writer = library.writers.get(article.writer);
        const subjects = (library.subjectsOf.get(article.id) ?? []).map((id) => library.subjects.get(id)).filter((subject) => subject !== undefined);
        return h(
          Rows,
          {},
          strip,
          h(Paragraph, { text: article.summary ?? article.title, foreground: Colour.White, lines: 3 }),
          h(Badges, { badges: [{ text: (article.kind ?? "post").toUpperCase(), background: Colour.Blue }], text: article.themes.join(", ") }),
          h(Gap, {}),
          h(Table, { rows: [["Written by", writer?.name ?? ""], ["On", formatDate(article.date, DATE_FORMATS.long)], ["Words", String(article.words)], ["On the podcast", stories.length === 0 ? "not yet" : `${stories.length} stories`]], labelCells: 16 }),
          h(Gap, {}),
          h(Text, { text: "Its subjects: click one to go there", foreground: Colour.Cyan }),
          ...subjects.slice(0, 4).map((subject, index) =>
            h(Text, { key: subject.id, text: `${(map.subjectPage.get(subject.id) ?? 0).toString(16)} ${subject.name}${index === 0 ? " (main)" : ""}`, foreground: index === 0 ? Colour.Yellow : Colour.White }),
          ),
          h(Gap, {}),
          h(Paragraph, { text: `About: ${article.topics.join(", ") || "-"}`, foreground: Colour.Cyan, lines: 2 }),
          h(Paragraph, { text: `Tags: ${article.tags.join(", ") || "none"}`, foreground: Colour.Green, lines: 2 }),
          writer === undefined ? null : h(Button, { label: `More by ${writer.name}`, shared: true, onPress: (session: number) => this.services.goto(session, PAGES.writer, { writer: writer.id }) }),
        );
      }
      case PODCAST:
        return h(
          Rows,
          {},
          strip,
          h(Paragraph, { text: stories.length === 0 ? "No podcast story talks about this article." : "The podcast talked about it: Enter opens the story, where you can hear it.", foreground: Colour.Cyan, lines: 2 }),
          h(StoryList, { key: "stories", site, stories, shown: 12, itemRows: 2, autoFocus: true }),
          h(Gap, { rows: 0, grow: 1 }),
          h(NowPlaying, {}),
        );
      case LINKS:
      default:
        return h(
          Rows,
          {},
          strip,
          h(Text, { text: "On Hackaday:", foreground: Colour.Cyan }),
          h(Paragraph, { text: article.link, foreground: Colour.White, lines: 2 }),
          h(Gap, {}),
          h(Text, { text: `Its links (${article.links.length}):`, foreground: Colour.Cyan }),
          h(LinesBlock, { lines: article.links.slice(0, 14).map((link) => ({ foreground: Colour.White, text: `${link.text.slice(0, 12)}: ${link.href.replace(/^https?:\/\//u, "")}` })), empty: "none" }),
        );
    }
  }

  override onKey(event: KeyEvent): boolean {
    const key = colourKeyOf(event);
    if (key !== null) {
      this.#choose(key);
      return true;
    }
    if (this.state.tab !== TEXT) {
      return false;
    }
    const step = listStep(event, TEXT_ROWS);
    if (step === null) {
      return false;
    }
    const last = Math.max(0, this.#text().length - TEXT_ROWS);
    this.setState({ top: Math.max(0, Math.min(last, Number.isFinite(step) ? this.state.top + step : step > 0 ? last : 0)) });
    return true;
  }

  override onWheel(event: WheelEvent): boolean {
    const last = Math.max(0, this.#text().length - TEXT_ROWS);
    this.setState({ top: Math.max(0, Math.min(last, this.state.top + this.#wheel.lines(event.dy))) });
    return true;
  }
}

export function readerPage(site: Site): PageSpec {
  return {
    page: PAGES.reader,
    title: "Reader",
    perViewer: true,
    ownsColourKeys: true,
    keepWhileAway: true,
    root: (params) => {
      const id = Number(params?.["article"] ?? site.library.newest[0] ?? 0);
      const article = site.library.article(id);
      const links = prompts(["Text", null], ["About", null], ["Podcast", null], ["Links", null]);
      if (article === undefined) {
        return frame(HOME_LOOK, "Reader", links, undefined, h(Placeholder, { text: "Choose an article from a list" }));
      }
      return frame(HOME_LOOK, "Reader", links, undefined, h(Badges, { badges: [{ text: formatDate(article.date, DATE_FORMATS.long), background: Colour.Blue }], text: `Article ${article.id}` }), h(Reader, { key: article.id, site, article }));
    },
  };
}
