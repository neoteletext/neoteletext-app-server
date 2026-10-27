// Magazine 1's archive pages: the front (100), the latest (101), search
// (102), every section as a tree (103), tags (104), writers (105), on this
// day (106), the years with a calendar to choose a day by (107), the
// figures (108), the pages that show one writer, tag or day (111 to 113),
// and what this is (190).

import { Colour } from "../kit.ts";
import {
  Autocomplete,
  Badges,
  Band,
  BarChart,
  Calendar,
  type CalendarDate,
  type Child,
  Columns,
  Component,
  DataTable,
  DoubleHeight,
  Gap,
  Grid,
  h,
  Heatmap,
  KeyHelp,
  LineChart,
  PageLink,
  type PageSpec,
  Paragraph,
  Placeholder,
  Rows,
  Sparkline,
  Table,
  Text,
  TextField,
  Ticker,
  Tree,
  type TreeNode,
  DATE_FORMATS,
  formatDate,
} from "../../../../kits/typescript-components/src/index.ts";
import { ArticleList, episodeName, episodeTitle, frame, prompts, type Site } from "../parts.ts";
import { decimal, HOME_LOOK, MAGAZINES, PAGES, SiteMap } from "../site.ts";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
/** Hours a heat map cell covers. */
const HOURS_A_CELL = 1;
const HOURS = 24;

/** The pages the front lists, by what they are. */
function frontLinks(): readonly (readonly [number, string])[] {
  return [
    [PAGES.latest, "Latest"],
    [PAGES.search, "Search"],
    [PAGES.sections, "Sections"],
    [PAGES.tags, "Tags"],
    [PAGES.writers, "Writers"],
    [PAGES.onThisDay, "This day"],
    [PAGES.years, "Years"],
    [PAGES.figures, "Figures"],
    [PAGES.podcast, "Podcast"],
    [PAGES.episodes, "Episodes"],
    [PAGES.storyFinder, "Stories"],
    [PAGES.rounds, "The sound"],
    [PAGES.player, "Playing"],
    [PAGES.about, "About"],
    ...MAGAZINES.map((magazine) => [SiteMap.magazinePage(magazine.number, 0), magazine.name.split(" ")[0] ?? ""] as const),
  ];
}

class Front extends Component<{ readonly site: Site }> {
  override render(): Child {
    const { library } = this.props.site;
    const latest = library.newest.slice(0, 12);
    const episode = library.episodes[0];
    return [
      h(Ticker, { items: latest.map((id) => library.heads.get(id)?.title ?? ""), everyMs: 6_000, background: Colour.Blue }),
      h(Band, { text: `LATEST: ${library.newest.length} ARTICLES`, background: Colour.Red, foreground: Colour.White }),
      h(ArticleList, { site: this.props.site, ids: latest, shown: 4 }),
      episode === undefined ? null : h(Badges, { badges: [{ text: "PODCAST", background: Colour.Magenta }], text: `${episodeName(library, episode.episode)} ${episodeTitle(episode.title)}` }),
      h(Gap, {}),
      h(Grid, { columns: 3, height: 8 }, ...frontLinks().map(([page, label]) => h(PageLink, { key: page, page, label }))),
    ];
  }
}

class Search extends Component<{ readonly site: Site }, { readonly asked: string; readonly found: readonly number[] }> {
  override state = { asked: "", found: [] as readonly number[] };

  override render(): Child {
    return [
      h(Paragraph, { text: "Words in a title: every word must be there", foreground: Colour.Cyan }),
      h(TextField, { longest: 30, placeholder: "type, then Enter", onSubmit: (words: string) => this.setState({ asked: words, found: this.props.site.library.search(words, 300) }) }),
      this.state.asked === "" ? null : h(Text, { text: `${this.state.found.length === 300 ? "300 or more" : this.state.found.length} for "${this.state.asked}"`, foreground: Colour.Green }),
      h(ArticleList, { key: this.state.asked, site: this.props.site, ids: this.state.found, shown: 14, itemRows: 2, empty: this.state.asked === "" ? "nothing asked yet" : "none found" }),
    ];
  }
}

function sectionsTree(site: Site): TreeNode[] {
  const { library, map } = site;
  return MAGAZINES.map((magazine) => ({
    label: `${magazine.number}00 ${magazine.name}`,
    page: SiteMap.magazinePage(magazine.number, 0),
    children: magazine.areas.flatMap((areaId) => {
      const area = library.areas.find((each) => each.id === areaId);
      return area === undefined
        ? []
        : [
            {
              label: `${decimal(map.areaPage.get(area.id) ?? 0)} ${area.name}`,
              children: [
                { label: `${decimal(map.areaPage.get(area.id) ?? 0)} All of ${area.name}`, page: map.areaPage.get(area.id) ?? PAGES.front },
                ...area.subjects.map((subject) => ({ label: `${decimal(map.subjectPage.get(subject.id) ?? 0)} ${subject.name} ${subject.articles.length}`, page: map.subjectPage.get(subject.id) ?? PAGES.front })),
              ],
            },
          ];
    }),
  }));
}

class Tags extends Component<{ readonly site: Site }> {
  override render(): Child {
    const { library } = this.props.site;
    const names = library.tags.map((tag) => tag.name);
    const go = (name: string, session: number): void => {
      const tag = library.tags.find((each) => each.name === name);
      if (tag !== undefined) {
        this.services.goto(session, PAGES.tag, { tag: tag.id });
      }
    };
    return [
      h(Autocomplete, { label: "Find a tag", labelCells: 12, items: names, shown: 4, anywhere: true, onChange: (name: string, session: number) => go(name, session) }),
      h(Gap, {}),
      h(DataTable, {
        columns: [
          { title: "Tag", width: 26 },
          { title: "Articles", width: 10, align: "right", foreground: Colour.Cyan },
        ],
        rows: library.tags.map((tag) => [tag.name, String(tag.articles.length)]),
        shown: 10,
        selectable: true,
        sortable: true,
        onChoose: (row: readonly string[], session: number) => go(row[0] ?? "", session),
      }),
    ];
  }
}

class Writers extends Component<{ readonly site: Site }> {
  override render(): Child {
    const { library } = this.props.site;
    const writers = [...library.writers.values()].filter((writer) => writer.articles.length > 0).sort((a, b) => b.articles.length - a.articles.length);
    const year = (id: number | undefined): string => (id === undefined ? "" : String(new Date(library.heads.get(id)?.date ?? 0).getUTCFullYear()));
    return h(DataTable, {
      columns: [
        { title: "Writer", width: 18 },
        { title: "Posts", width: 7, align: "right", foreground: Colour.Cyan },
        { title: "From", width: 5, foreground: Colour.Green },
        { title: "To", width: 5, foreground: Colour.Green },
      ],
      rows: writers.map((writer) => [writer.name, String(writer.articles.length), year(writer.articles.at(-1)), year(writer.articles[0]), String(writer.id)]),
      shown: 17,
      selectable: true,
      sortable: true,
      onChoose: (row: readonly string[], session: number) => this.services.goto(session, PAGES.writer, { writer: Number(row[4]) }),
    });
  }
}

class OnThisDay extends Component<{ readonly site: Site }, { readonly now: number }> {
  override state = { now: 0 };

  override componentDidMount(): void {
    this.setState({ now: this.services.epochMs() });
  }

  override render(): Child {
    if (this.state.now === 0) {
      return h(Placeholder, { text: "Finding the day..." });
    }
    const today = new Date(this.state.now);
    const { library } = this.props.site;
    const ids = library.newest.filter((id) => {
      const date = new Date(library.heads.get(id)?.date ?? 0);
      return date.getUTCMonth() === today.getUTCMonth() && date.getUTCDate() === today.getUTCDate() && date.getUTCFullYear() < today.getUTCFullYear();
    });
    return [h(Text, { text: `${formatDate(this.state.now, "DD MMM")} in other years: ${ids.length}`, foreground: Colour.Cyan }), h(ArticleList, { site: this.props.site, ids, shown: 17, empty: "nothing on this day before" })];
  }
}

class Years extends Component<{ readonly site: Site }> {
  override render(): Child {
    const { library } = this.props.site;
    const byYear = new Map<number, number>();
    const days = new Set<string>();
    for (const id of library.newest) {
      const date = new Date(library.heads.get(id)?.date ?? 0);
      byYear.set(date.getUTCFullYear(), (byYear.get(date.getUTCFullYear()) ?? 0) + 1);
      days.add(`${date.getUTCFullYear()}-${date.getUTCMonth()}-${date.getUTCDate()}`);
    }
    const years = [...byYear.keys()].sort((a, b) => a - b);
    const newest = new Date(library.heads.get(library.newest[0] ?? 0)?.date ?? 0);
    const marked: CalendarDate[] = [...days].map((day) => {
      const [year, month, date] = day.split("-").map(Number);
      return { year: year ?? 0, month: month ?? 0, day: date ?? 0 };
    });
    return [
      h(LineChart, { values: years.map((year) => byYear.get(year) ?? 0), plotRows: 5, area: true, foreground: Colour.Yellow, labels: [String(years[0] ?? ""), "", "", String(years.at(-1) ?? "")] }),
      h(Gap, {}),
      h(
        Columns,
        { widths: [29, null], height: 8 },
        h(Calendar, {
          year: newest.getUTCFullYear(),
          month: newest.getUTCMonth(),
          marked,
          selectable: true,
          onChoose: (date: CalendarDate, session: number) => this.services.goto(session, PAGES.day, { day: `${date.year}-${String(date.month + 1).padStart(2, "0")}-${String(date.day).padStart(2, "0")}` }),
        }),
        h(Paragraph, { text: "Green days have posts. Arrows, PgUp PgDn; Enter lists the day.", foreground: Colour.White }),
      ),
    ];
  }
}

class Figures extends Component<{ readonly site: Site }> {
  override render(): Child {
    const { library } = this.props.site;
    const hours = WEEKDAYS.map(() => new Array<number>(HOURS / HOURS_A_CELL).fill(0));
    const months = new Map<string, number>();
    for (const id of library.newest) {
      const date = new Date(library.heads.get(id)?.date ?? 0);
      const weekday = (date.getUTCDay() + 6) % 7;
      const row = hours[weekday];
      if (row !== undefined) {
        row[Math.floor(date.getUTCHours() / HOURS_A_CELL)] = (row[Math.floor(date.getUTCHours() / HOURS_A_CELL)] ?? 0) + 1;
      }
      const month = `${date.getUTCFullYear()}-${String(date.getUTCMonth()).padStart(2, "0")}`;
      months.set(month, (months.get(month) ?? 0) + 1);
    }
    const lastMonths = [...months.keys()].sort().slice(-52, -1).map((month) => months.get(month) ?? 0);
    const areas = library.areas.map((area) => ({ label: area.name, value: area.subjects.reduce((sum, subject) => sum + subject.articles.length, 0) })).sort((a, b) => b.value - a.value).slice(0, 6);
    const hoursOfStories = library.stories.reduce((sum, story) => sum + story.end - story.start, 0) / 3600;
    return [
      h(Table, {
        rows: [
          ["Articles", String(library.newest.length)],
          ["Writers", String([...library.writers.values()].filter((writer) => writer.articles.length > 0).length)],
          ["Episodes, stories", `${library.episodes.length}, ${library.stories.length} (${Math.round(hoursOfStories)} h)`],
        ],
        labelCells: 18,
      }),
      h(BarChart, { bars: areas, labelCells: 16, foreground: Colour.Green }),
      h(Sparkline, { label: "Monthly", values: lastMonths, plotRows: 2, foreground: Colour.Cyan }),
      h(Text, { text: "Posts by weekday and hour, UTC: denser is more", foreground: Colour.Yellow }),
      h(Heatmap, { rows: WEEKDAYS.map((label, index) => ({ label, values: hours[index] ?? [] })), scale: "density", labelCells: 5, foreground: Colour.Yellow }),
    ];
  }
}

/** One writer's page: who, how many, their years, their articles. */
function writerPage(site: Site): PageSpec {
  return {
    page: PAGES.writer,
    title: "A writer",
    perViewer: true,
    root: (params) => {
      const writer = site.library.writers.get(Number(params?.["writer"] ?? -1)) ?? [...site.library.writers.values()].sort((a, b) => b.articles.length - a.articles.length)[0];
      const links = prompts(["Writers", PAGES.writers], ["Latest", PAGES.latest], ["Tags", PAGES.tags], ["Search", PAGES.search]);
      if (writer === undefined) {
        return frame(HOME_LOOK, "A writer", links, undefined, h(Placeholder, { text: "Choose a writer on 105" }));
      }
      const byYear = new Map<number, number>();
      for (const id of writer.articles) {
        const year = new Date(site.library.heads.get(id)?.date ?? 0).getUTCFullYear();
        byYear.set(year, (byYear.get(year) ?? 0) + 1);
      }
      const years = [...byYear.keys()].sort((a, b) => a - b);
      const values = years.length === 0 ? [] : Array.from({ length: (years.at(-1) ?? 0) - (years[0] ?? 0) + 1 }, (_, index) => byYear.get((years[0] ?? 0) + index) ?? 0);
      return frame(
        HOME_LOOK,
        "A writer",
        links,
        undefined,
        h(DoubleHeight, { text: writer.name, foreground: Colour.Yellow }),
        h(Text, { text: `${writer.articles.length} articles, ${years[0] ?? ""} to ${years.at(-1) ?? ""}`, foreground: Colour.Cyan }),
        h(Sparkline, { label: "Yearly", values, foreground: Colour.Green }),
        h(ArticleList, { key: writer.id, site, ids: writer.articles, shown: 14, itemRows: 1 }),
      );
    },
  };
}

function tagPage(site: Site): PageSpec {
  return {
    page: PAGES.tag,
    title: "A tag",
    perViewer: true,
    root: (params) => {
      const tag = site.library.tags.find((each) => each.id === Number(params?.["tag"] ?? -1)) ?? site.library.tags[0];
      const links = prompts(["Tags", PAGES.tags], ["Latest", PAGES.latest], ["Writers", PAGES.writers], ["Search", PAGES.search]);
      return tag === undefined
        ? frame(HOME_LOOK, "A tag", links, undefined, h(Placeholder, { text: "Choose a tag on 104" }))
        : frame(HOME_LOOK, "A tag", links, undefined, h(DoubleHeight, { text: tag.name, foreground: Colour.Yellow }), h(Text, { text: `${tag.articles.length} articles, newest first`, foreground: Colour.Cyan }), h(ArticleList, { key: tag.id, site, ids: tag.articles, shown: 16, itemRows: 2 }));
    },
  };
}

function dayPage(site: Site): PageSpec {
  return {
    page: PAGES.day,
    title: "A day",
    perViewer: true,
    root: (params) => {
      const day = String(params?.["day"] ?? "");
      const ids = site.library.newest.filter((id) => new Date(site.library.heads.get(id)?.date ?? 0).toISOString().startsWith(day));
      const links = prompts(["Years", PAGES.years], ["This day", PAGES.onThisDay], ["Latest", PAGES.latest], ["Search", PAGES.search]);
      return frame(HOME_LOOK, "A day", links, undefined, h(DoubleHeight, { text: day === "" ? "Choose a day on 107" : formatDate(day, DATE_FORMATS.long), foreground: Colour.Yellow }), h(Text, { text: `${ids.length} articles`, foreground: Colour.Cyan }), h(ArticleList, { key: day, site, ids: [...ids].reverse(), shown: 16, itemRows: 2, empty: "no articles that day" }));
    },
  };
}

const ABOUT_TEXT = [
  "This is the Hackaday archive as teletext: every post since 2004, read by Claude Haiku into 36 areas and 268 subjects, each with a page in magazines 2 to 8.",
  "The podcast's 397 episodes are cut into 6,232 stories. A story plays here as a live sound, and links to the articles it talks about; an article links back to its stories.",
  "What's That Sound? (128) is the podcast's game: every round's challenge, its reveal and the sound alone.",
] as const;

export function homePages(site: Site): PageSpec[] {
  const { library } = site;
  return [
    {
      page: PAGES.front,
      title: "Hackaday",
      section: "Hackaday",
      root: () => frame(HOME_LOOK, "Hackaday", prompts(["Latest", PAGES.latest], ["Sections", PAGES.sections], ["Podcast", PAGES.podcast], ["The sound", PAGES.rounds]), "Type a page, or choose a headline", h(Front, { site })),
    },
    { page: PAGES.latest, title: "The latest", root: () => frame(HOME_LOOK, "The latest", prompts(["Front", PAGES.front], ["Search", PAGES.search], ["Sections", PAGES.sections], ["Podcast", PAGES.podcast]), undefined, h(ArticleList, { site, ids: library.newest.slice(0, 500), shown: 20, itemRows: 2 })) },
    { page: PAGES.search, title: "Search", root: () => frame(HOME_LOOK, "Search", prompts(["Front", PAGES.front], ["Tags", PAGES.tags], ["Writers", PAGES.writers], ["Sections", PAGES.sections]), undefined, h(Search, { site })) },
    {
      page: PAGES.sections,
      title: "Sections",
      root: () => frame(HOME_LOOK, "Every section", prompts(["Front", PAGES.front], ["Tags", PAGES.tags], ["Latest", PAGES.latest], ["Figures", PAGES.figures]), undefined, h(Tree, { nodes: sectionsTree(site), shown: 20, guides: true })),
    },
    { page: PAGES.tags, title: "Tags", root: () => frame(HOME_LOOK, "Tags", prompts(["Front", PAGES.front], ["Writers", PAGES.writers], ["Search", PAGES.search], ["Sections", PAGES.sections]), undefined, h(Tags, { site })) },
    { page: PAGES.writers, title: "Writers", root: () => frame(HOME_LOOK, "Writers", prompts(["Front", PAGES.front], ["Tags", PAGES.tags], ["Search", PAGES.search], ["Figures", PAGES.figures]), undefined, h(Writers, { site })) },
    { page: PAGES.onThisDay, title: "On this day", root: () => frame(HOME_LOOK, "On this day", prompts(["Front", PAGES.front], ["Years", PAGES.years], ["Latest", PAGES.latest], ["Figures", PAGES.figures]), undefined, h(OnThisDay, { site })) },
    { page: PAGES.years, title: "The years", root: () => frame(HOME_LOOK, "The years", prompts(["Front", PAGES.front], ["This day", PAGES.onThisDay], ["Figures", PAGES.figures], ["Latest", PAGES.latest]), undefined, h(Years, { site })) },
    { page: PAGES.figures, title: "Figures", root: () => frame(HOME_LOOK, "Figures", prompts(["Front", PAGES.front], ["Years", PAGES.years], ["Writers", PAGES.writers], ["Sections", PAGES.sections]), undefined, h(Figures, { site })) },
    writerPage(site),
    tagPage(site),
    dayPage(site),
    {
      page: PAGES.about,
      title: "About",
      root: () =>
        frame(
          HOME_LOOK,
          "About",
          prompts(["Front", PAGES.front], ["Podcast", PAGES.podcast], ["The sound", PAGES.rounds], ["Sections", PAGES.sections]),
          undefined,
          ...ABOUT_TEXT.map((text, index) => h(Rows, { key: index }, h(Paragraph, { text, foreground: index === 0 ? Colour.White : Colour.Cyan }), h(Gap, {}))),
          h(KeyHelp, {
            columns: 2,
            keys: [
              ["Up Down", "Choose"],
              ["Enter", "Open, play"],
              ["Colours", "Fastext"],
              ["Tab", "Next part"],
              ["Index", "Front"],
              ["Reveal", "Answers"],
            ],
          }),
          h(Gap, {}),
          h(Text, { text: `Latest article ${formatDate(library.heads.get(library.newest[0] ?? 0)?.date ?? 0, DATE_FORMATS.long)}; episodes to ${episodeName(library, 1)}`, foreground: Colour.Green }),
        ),
    },
  ];
}

