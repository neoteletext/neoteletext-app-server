// The Hackaday Podcast (120 to 127): its front, every episode, its hosts,
// a finder for its 6,232 stories, one episode (125) and one story (126),
// shown for the one a viewer was sent with, and what is playing (127). A
// story plays as a live sound (parts.ts, storySource), and links to the
// articles it talks about, its subject's page and its episode; an episode
// links to its show notes in the reader.

import { Colour } from "../kit.ts";
import {
  Autocomplete,
  Badges,
  BarChart,
  Button,
  type Child,
  Columns,
  Component,
  DataTable,
  Gap,
  h,
  type PageSpec,
  Paragraph,
  Placeholder,
  Rule,
  Select,
  Text,
  clockTime,
  NowPlaying,
  DATE_FORMATS,
  formatDate,
} from "../../../../kits/typescript-components/src/index.ts";
import type { Episode, Story } from "../library.ts";
import { ArticleList, episodeName, episodeTitle, frame, prompts, type Site, StoryList, storySource } from "../parts.ts";
import { decimal, PAGES, PODCAST_LOOK } from "../site.ts";

/** An episode's line in a list: its number, date and title. */
function episodeLine(episode: Episode): string {
  return `${episode.number === null ? "Special" : `#${episode.number}`}, ${formatDate(episode.published, DATE_FORMATS.long)}: ${episodeTitle(episode.title)}`;
}

class PodcastFront extends Component<{ readonly site: Site }> {
  override render(): Child {
    const { library } = this.props.site;
    const latest = library.episodes[0];
    return [
      latest === undefined ? null : h(Paragraph, { text: `${episodeName(library, latest.episode)} ${episodeTitle(latest.title)}`, foreground: Colour.Yellow, lines: 2 }),
      latest === undefined ? null : h(Badges, { badges: [{ text: "NEWEST", background: Colour.Red, flash: true }, { text: formatDate(latest.published, DATE_FORMATS.long), background: Colour.Blue }, { text: clockTime(latest.seconds), background: Colour.Magenta }], text: latest.hosts.join(", ") }),
      latest === undefined ? null : h(StoryList, { site: this.props.site, stories: latest.stories, shown: 6, episodes: false }),
      h(Rule, { title: "earlier episodes", foreground: Colour.Magenta }),
      h(EpisodeChoice, { site: this.props.site, episodes: library.episodes.slice(1, 7) }),
      h(Gap, {}),
      h(Text, { text: `${library.episodes.length} episodes, ${library.stories.length} stories since 2018`, foreground: Colour.Cyan }),
    ];
  }
}

/** Episodes to choose from: Enter opens one. */
class EpisodeChoice extends Component<{ readonly site: Site; readonly episodes: readonly Episode[] }> {
  override render(): Child {
    return h(DataTable, {
      columns: [
        { title: "Ep", width: 5, foreground: Colour.Yellow },
        { title: "Date", width: 11 },
        { title: "Stories", width: 9, align: "right", foreground: Colour.Cyan },
        { title: "Long", width: 9, align: "right", foreground: Colour.Green },
      ],
      rows: this.props.episodes.map((episode) => [episode.number === null ? "-" : `#${episode.number}`, formatDate(episode.published, DATE_FORMATS.iso), String(episode.stories.length), clockTime(episode.seconds), String(episode.episode)]),
      shown: this.props.episodes.length,
      selectable: true,
      onChoose: (row: readonly string[], session: number) => this.services.goto(session, PAGES.episode, { episode: Number(row[4]) }),
    });
  }
}

class Episodes extends Component<{ readonly site: Site }> {
  override render(): Child {
    const { library } = this.props.site;
    return h(DataTable, {
      columns: [
        { title: "Ep", width: 5, foreground: Colour.Yellow },
        { title: "Date", width: 11 },
        { title: "Title", width: 15 },
        { title: "St", width: 4, align: "right", foreground: Colour.Cyan },
      ],
      rows: library.episodes.map((episode) => [episode.number === null ? "-" : `#${episode.number}`, formatDate(episode.published, DATE_FORMATS.iso), episodeTitle(episode.title), String(episode.stories.length), String(episode.episode)]),
      shown: 18,
      selectable: true,
      sortable: true,
      onChoose: (row: readonly string[], session: number) => this.services.goto(session, PAGES.episode, { episode: Number(row[4]) }),
    });
  }
}

class Hosts extends Component<{ readonly site: Site }> {
  override render(): Child {
    const counts = new Map<string, number>();
    for (const episode of this.props.site.library.episodes) {
      for (const host of episode.hosts) {
        counts.set(host, (counts.get(host) ?? 0) + 1);
      }
    }
    const hosts = [...counts.entries()].sort((a, b) => b[1] - a[1]);
    return [
      h(BarChart, { bars: hosts.slice(0, 6).map(([label, value]) => ({ label, value })), labelCells: 16, foreground: Colour.Magenta }),
      h(Gap, {}),
      h(DataTable, {
        columns: [
          { title: "Host", width: 24 },
          { title: "Episodes", width: 10, align: "right", foreground: Colour.Cyan },
        ],
        rows: hosts.map(([host, episodes]) => [host, String(episodes)]),
        shown: 10,
        sortable: true,
      }),
    ];
  }
}

class StoryFinder extends Component<{ readonly site: Site }, { readonly kind: number }> {
  override state = { kind: 0 };

  override render(): Child {
    const { site } = this.props;
    const { stories } = site.library;
    const kinds = [...new Set(stories.map((story) => story.kind))].sort();
    const chosen = kinds[this.state.kind] ?? "";
    return [
      h(Autocomplete, {
        label: "A story",
        labelCells: 9,
        items: stories.map((story) => story.title),
        shown: 4,
        anywhere: true,
        onChange: (title: string, session: number) => {
          const story = stories.find((each) => each.title === title);
          if (story !== undefined) {
            this.services.goto(session, PAGES.story, { episode: story.episode, story: story.story });
          }
        },
      }),
      h(Gap, {}),
      h(Select, { label: "Or a kind", labelCells: 11, items: kinds.map((kind) => `${kind} (${stories.filter((story) => story.kind === kind).length})`), value: this.state.kind, onChange: (kind: number) => this.setState({ kind }) }),
      h(StoryList, { key: chosen, site, stories: stories.filter((story) => story.kind === chosen), shown: 9 }),
    ];
  }
}

class EpisodeView extends Component<{ readonly site: Site; readonly episode: Episode }, { readonly chosen: number }> {
  override state = { chosen: 0 };

  override render(): Child {
    const { site, episode } = this.props;
    const story = episode.stories[this.state.chosen];
    return [
      h(Paragraph, { text: `${episodeName(site.library, episode.episode)} ${episodeTitle(episode.title)}`, foreground: Colour.Yellow, lines: 2 }),
      h(Badges, { badges: [{ text: formatDate(episode.published, DATE_FORMATS.long), background: Colour.Blue }, { text: clockTime(episode.seconds), background: Colour.Magenta }], text: episode.hosts.join(", ") }),
      h(StoryList, {
        site,
        stories: episode.stories,
        shown: 9,
        episodes: false,
        empty: "this episode was not cut into stories",
        onMove: (chosen: number) => this.setState({ chosen }),
        onChoose: (chosen: Story, session: number) => this.services.goto(session, PAGES.story, { episode: chosen.episode, story: chosen.story }),
      }),
      story === undefined ? null : h(Paragraph, { text: story.summary, foreground: Colour.White, lines: 3 }),
      h(
        Columns,
        { height: 1 },
        episode.post === null ? null : h(Button, { label: "Show notes", background: Colour.Green, onPress: (session: number) => this.services.goto(session, PAGES.reader, { article: episode.post ?? 0 }) }),
        story === undefined ? null : h(Button, { label: "Listen", background: Colour.Magenta, onPress: (session: number) => this.services.stream(storySource(site, story), session) }),
      ),
      h(NowPlaying, {}),
    ];
  }
}

class StoryView extends Component<{ readonly site: Site; readonly story: Story }> {
  override render(): Child {
    const { site, story } = this.props;
    const { library, map } = site;
    const subject = story.subject === null ? undefined : library.subjects.get(story.subject);
    return [
      h(Paragraph, { text: story.title, foreground: Colour.Yellow, lines: 2 }),
      h(Badges, { badges: [{ text: story.kind.toUpperCase(), background: Colour.Blue }, { text: episodeName(library, story.episode), background: Colour.Magenta }], text: `${clockTime(story.start)} to ${clockTime(story.end)}` }),
      h(Paragraph, { text: story.summary, foreground: Colour.White, lines: 4 }),
      h(Paragraph, { text: `Tags: ${library.storyTags(story).join(", ") || "none"}`, foreground: Colour.Green, lines: 2 }),
      h(
        Columns,
        { height: 1 },
        h(Button, { label: "Listen", background: Colour.Magenta, onPress: (session: number) => this.services.stream(storySource(site, story), session) }),
        h(Button, { label: "Stop", background: Colour.White, onPress: (session: number) => this.services.stopStream(session) }),
        h(Button, { label: "Episode", background: Colour.Green, onPress: (session: number) => this.services.goto(session, PAGES.episode, { episode: story.episode }) }),
      ),
      h(NowPlaying, {}),
      subject === undefined ? null : h(Text, { text: `Subject: ${decimal(map.subjectPage.get(subject.id) ?? 0)} ${subject.name}`, foreground: Colour.Green }),
      h(Text, { text: `It talks about ${story.articles.length} article${story.articles.length === 1 ? "" : "s"}:`, foreground: Colour.Cyan }),
      h(ArticleList, { site, ids: story.articles, shown: 5 }),
    ];
  }
}

class Player extends Component<{ readonly site: Site }> {
  override render(): Child {
    const { site } = this.props;
    const newest = site.library.episodes[0]?.stories[0];
    return [
      h(Text, { text: "What you are listening to", foreground: Colour.Cyan }),
      h(NowPlaying, {}),
      h(Gap, {}),
      h(Columns, { height: 1 }, h(Button, { label: "Stop", background: Colour.White, onPress: (session: number) => this.services.stopStream(session) }), newest === undefined ? null : h(Button, { label: "Newest story", background: Colour.Magenta, onPress: (session: number) => this.services.stream(storySource(site, newest), session) })),
      h(Gap, {}),
      h(Paragraph, { text: "A story plays while you stay on its page; leaving the page stops it. Choose stories on 120, 123, an episode, an article's podcast view, or any subject's.", foreground: Colour.White }),
    ];
  }
}

export function podcastPages(site: Site): PageSpec[] {
  const { library } = site;
  const nav = (...links: (readonly [string, number])[]): ReturnType<typeof prompts> => prompts(...links);
  return [
    { page: PAGES.podcast, title: "The podcast", section: "Podcast", root: () => frame(PODCAST_LOOK, "The Hackaday Podcast", nav(["Episodes", PAGES.episodes], ["Stories", PAGES.storyFinder], ["The sound", PAGES.rounds], ["Hosts", PAGES.hosts]), undefined, h(PodcastFront, { site })) },
    { page: PAGES.episodes, title: "Every episode", root: () => frame(PODCAST_LOOK, "Every episode", nav(["Podcast", PAGES.podcast], ["Stories", PAGES.storyFinder], ["Hosts", PAGES.hosts], ["Playing", PAGES.player]), undefined, h(Episodes, { site })) },
    { page: PAGES.hosts, title: "The hosts", root: () => frame(PODCAST_LOOK, "The hosts", nav(["Podcast", PAGES.podcast], ["Episodes", PAGES.episodes], ["Stories", PAGES.storyFinder], ["Playing", PAGES.player]), undefined, h(Hosts, { site })) },
    { page: PAGES.storyFinder, title: "Find a story", root: () => frame(PODCAST_LOOK, "Find a story", nav(["Podcast", PAGES.podcast], ["Episodes", PAGES.episodes], ["Playing", PAGES.player], ["The sound", PAGES.rounds]), undefined, h(StoryFinder, { site })) },
    {
      page: PAGES.episode,
      title: "An episode",
      perViewer: true,
      keepWhileAway: true,
      root: (params) => {
        const episode = library.episodes[Number(params?.["episode"] ?? 1) - 1];
        const links = nav(["Episodes", PAGES.episodes], ["Podcast", PAGES.podcast], ["Playing", PAGES.player], ["Stories", PAGES.storyFinder]);
        return episode === undefined ? frame(PODCAST_LOOK, "An episode", links, undefined, h(Placeholder, { text: "Choose an episode on 121" })) : frame(PODCAST_LOOK, episodeLine(episode).split(":")[0] ?? "An episode", links, undefined, h(EpisodeView, { key: episode.episode, site, episode }));
      },
    },
    {
      page: PAGES.story,
      title: "A story",
      perViewer: true,
      keepWhileAway: true,
      root: (params) => {
        const episode = library.episodes[Number(params?.["episode"] ?? 1) - 1];
        const story = episode?.stories.find((each) => each.story === Number(params?.["story"] ?? 1)) ?? episode?.stories[0];
        const links = nav(["Episode", PAGES.episode], ["Podcast", PAGES.podcast], ["Playing", PAGES.player], ["Stories", PAGES.storyFinder]);
        return story === undefined ? frame(PODCAST_LOOK, "A story", links, undefined, h(Placeholder, { text: "Choose a story on 123" })) : frame(PODCAST_LOOK, "A story", links, undefined, h(StoryView, { key: `${story.episode}/${story.story}`, site, story }));
      },
    },
    { page: PAGES.player, title: "Now playing", perViewer: true, root: () => frame(PODCAST_LOOK, "Now playing", nav(["Podcast", PAGES.podcast], ["Episodes", PAGES.episodes], ["Stories", PAGES.storyFinder], ["The sound", PAGES.rounds]), undefined, h(Player, { site })) },
  ];
}

