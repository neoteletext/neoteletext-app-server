// What the pages share: the frame in a magazine's look, lists of articles and of podcast stories that open the
// one chosen, and a bar showing the clip a viewer is listening to.

import {
  type Child,
  ChoiceList,
  colourKeyOf,
  Component,
  type Element,
  Frame,
  h,
  type KeyEvent,
  type PageNeeds,
  type Prompts,
  Rows,
  type StreamSource,
  TabStrip,
  DATE_FORMATS,
  formatDate,
} from "../../../kits/typescript-components/src/index.ts";
import type { Clips } from "./clips.ts";
import type { Library, Story } from "./library.ts";
import { type Look, PAGES, type SiteMap } from "./site.ts";

/** What every page reads: the archive, where everything is, the clips and where they are kept. */
export interface Site {
  readonly library: Library;
  readonly map: SiteMap;
  readonly clips: Clips;
  /** The analysis folder, which holds the story clips. */
  readonly analysis: string;
  /** The What's That Sound clips folder. */
  readonly roundClips: string;
}

/** Four Fastext prompts from those given, the rest empty; a page of null is a label for a key the page takes itself. */
export function prompts(...given: (readonly [string, number | null])[]): Prompts {
  const four = [...given, ["", null], ["", null], ["", null], ["", null]] as const;
  return [four[0], four[1], four[2], four[3]] as unknown as Prompts;
}

/** A frame in a magazine's look; its Index key goes to the front. */
export function frame(look: Look, title: string, links: Prompts, hint: string | undefined, ...body: Child[]): Element {
  return h(Frame, { title, background: look.background, foreground: look.foreground, prompts: links, index: PAGES.front, ...(hint === undefined ? {} : { hint }) }, ...body);
}

/**
 * An episode as the show numbers it, "#379", or a special's date. The "#"
 * keeps the browser from taking the number for a page (its hyperlinks.md,
 * section 6.1: a number after "#" is not a page number).
 */
export function episodeName(library: Library, episode: number): string {
  const found = library.episodes[episode - 1];
  return found === undefined ? "" : found.number === null ? formatDate(found.published, DATE_FORMATS.long) : `#${found.number}`;
}

/** An episode's title without the "Ep 379:" it starts with, which would read as a link to page 379. */
export function episodeTitle(title: string): string {
  return title.replace(/^Ep\s*\d+\s*[:-]\s*/u, "");
}

export interface ArticleListProps {
  readonly site: Site;
  readonly ids: readonly number[];
  readonly shown: number;
  /** Rows an article takes: two shows long titles whole. */
  readonly itemRows?: number;
  readonly empty?: string;
  readonly autoFocus?: boolean;
}

/** Articles to choose from, newest first: Enter or a click opens one in the reader. */
export class ArticleList extends Component<ArticleListProps> {
  override render(): Child {
    const { library } = this.props.site;
    return h(ChoiceList, {
      items: this.props.ids.map((id) => {
        const head = library.heads.get(id);
        return head === undefined ? "" : `${formatDate(head.date, DATE_FORMATS.numeric)} ${head.title}`;
      }),
      shown: this.props.shown,
      itemRows: this.props.itemRows ?? 1,
      empty: this.props.empty ?? "no articles",
      ...(this.props.autoFocus === true ? { autoFocus: true } : {}),
      onChoose: (index: number, session: number) => {
        const id = this.props.ids[index];
        if (id !== undefined) {
          this.services.goto(session, PAGES.reader, { article: id });
        }
      },
    });
  }
}

export interface StoryListProps {
  readonly site: Site;
  readonly stories: readonly Story[];
  readonly shown: number;
  readonly itemRows?: number;
  readonly empty?: string;
  readonly autoFocus?: boolean;
  /** With each story's episode before it. */
  readonly episodes?: boolean;
  readonly onMove?: (index: number, session: number) => void;
  /** What Enter does; by default it opens the story's page. */
  readonly onChoose?: (story: Story, session: number) => void;
}

/** Podcast stories to choose from: Enter or a click opens the story's page, where it can be heard. */
export class StoryList extends Component<StoryListProps> {
  override render(): Child {
    const { library } = this.props.site;
    return h(ChoiceList, {
      items: this.props.stories.map((story) => (this.props.episodes === false ? story.title : `${episodeName(library, story.episode)} ${story.title}`)),
      shown: this.props.shown,
      itemRows: this.props.itemRows ?? 1,
      empty: this.props.empty ?? "no stories",
      ...(this.props.autoFocus === true ? { autoFocus: true } : {}),
      ...(this.props.onMove === undefined ? {} : { onMove: this.props.onMove }),
      onChoose: (index: number, session: number) => {
        const story = this.props.stories[index];
        if (story === undefined) {
          return;
        }
        if (this.props.onChoose !== undefined) {
          this.props.onChoose(story, session);
        } else {
          this.services.goto(session, PAGES.story, { episode: story.episode, story: story.story });
        }
      },
    });
  }
}

/** A story's clip as a stream source. */
export function storySource(site: Site, story: Story): StreamSource {
  return site.clips.source(story.title, `${site.analysis}/${story.audio}`, story.end - story.start);
}

export interface KeyTabsProps {
  readonly tabs: readonly string[];
  /** For each tab, whether the tabs keep the keys there: a tab with a list of its own gives them to the list (autoFocus) and hears the colours from it. */
  readonly keepsKeys: readonly boolean[];
}

/** Views chosen with the coloured keys, as Fastext's prompts name them: the children are the views. */
export class KeyTabs extends Component<KeyTabsProps, { readonly active: number }> {
  override state = { active: 0 };

  override needs(): PageNeeds {
    return { colourKeys: true, ownCopy: true };
  }

  override focusable(): boolean {
    return this.props.keepsKeys[this.state.active] === true;
  }

  override hint(): string {
    return "The coloured keys choose what to see";
  }

  #choose(active: number): void {
    this.setState({ active });
    if (this.props.keepsKeys[active] === true) {
      this.focus();
    }
  }

  override render(): Child {
    return h(Rows, {}, h(TabStrip, { tabs: this.props.tabs, active: this.state.active, style: "fastext", onChange: (index: number) => this.#choose(index) }), h(Rows, { key: `tab-${this.state.active}` }, (this.props.children ?? [])[this.state.active] ?? null));
  }

  override onKey(event: KeyEvent): boolean {
    const key = colourKeyOf(event);
    if (key === null || key >= this.props.tabs.length) {
      return false;
    }
    this.#choose(key);
    return true;
  }
}
