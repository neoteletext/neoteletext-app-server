// Magazines 2 to 8: the 36 areas and their 268 subjects, each with a page
// of its own (site.ts). A magazine's front lists its areas and its newest
// articles (N00); its latest (N01) and its podcast stories (N02) follow. An
// area's page lists its subjects with their counts, then its newest
// articles. A subject's page says what it covers; the coloured keys show
// its articles, the podcast stories about it, its years, and the rest of
// its area.

import { Colour } from "../kit.ts";
import {
  Badges,
  Band,
  type Child,
  ColumnChart,
  Component,
  Gap,
  h,
  type NumberedRow,
  NumberedRows,
  type PageSpec,
  Paragraph,
  Text,
  NowPlaying,
} from "../../../../kits/typescript-components/src/index.ts";
import type { Area, Story, Subject } from "../library.ts";
import { ArticleList, frame, KeyTabs, prompts, type Site, StoryList } from "../parts.ts";
import { MAGAZINE_LATEST, MAGAZINE_PODCAST, MAGAZINES, PAGES, SiteMap } from "../site.ts";

type Magazine = (typeof MAGAZINES)[number];

/** Articles newest first, from several newest-first lists. */
function newestOf(site: Site, lists: readonly (readonly number[])[], most: number): number[] {
  const { heads } = site.library;
  return lists
    .flat()
    .sort((a, b) => (heads.get(b)?.date ?? 0) - (heads.get(a)?.date ?? 0))
    .slice(0, most);
}

function areasOf(site: Site, magazine: Magazine): Area[] {
  return magazine.areas.map((id) => site.library.areas.find((area) => area.id === id)).filter((area) => area !== undefined);
}

function storiesOf(subjects: readonly Subject[]): Story[] {
  return subjects.flatMap((subject) => subject.stories).sort((a, b) => a.episode - b.episode || a.story - b.story);
}

/** How many articles an area has: its subjects' together. */
function count(area: Area): number {
  return area.subjects.reduce((sum, subject) => sum + subject.articles.length, 0);
}

/** The most articles a latest list holds. */
const LATEST_MOST = 400;

class SubjectYears extends Component<{ readonly site: Site; readonly subject: Subject }> {
  override render(): Child {
    const byYear = new Map<number, number>();
    for (const id of this.props.subject.articles) {
      const year = new Date(this.props.site.library.heads.get(id)?.date ?? 0).getUTCFullYear();
      byYear.set(year, (byYear.get(year) ?? 0) + 1);
    }
    const years = [...byYear.keys()].sort((a, b) => a - b).slice(-12);
    return [
      h(Text, { text: "Articles a year", foreground: Colour.Cyan }),
      h(ColumnChart, { bars: years.map((year) => ({ label: String(year % 100).padStart(2, "0"), value: byYear.get(year) ?? 0 })), plotRows: 9, figures: true, foreground: Colour.Green }),
    ];
  }
}

function subjectPage(site: Site, magazine: Magazine, area: Area, subject: Subject): PageSpec {
  const { map } = site;
  const page = map.subjectPage.get(subject.id) ?? 0;
  const siblings: NumberedRow[] = area.subjects.map((each) => ({ page: map.subjectPage.get(each.id) ?? 0, label: each.name, figure: each.articles.length, foreground: each.id === subject.id ? Colour.Yellow : Colour.White }));
  return {
    page,
    title: subject.name,
    root: () =>
      frame(
        magazine.look,
        subject.name,
        prompts(["Articles", null], ["Podcast", null], ["Years", null], [area.name.split(" ")[0] ?? "Area", null]),
        undefined,
        h(Paragraph, { text: subject.scope, foreground: Colour.White, lines: 2 }),
        h(Badges, { badges: [{ text: `${subject.articles.length} articles`, background: Colour.Blue }, { text: `${subject.stories.length} stories`, background: Colour.Magenta }] }),
        h(
          KeyTabs,
          { tabs: ["Articles", "Podcast", "Years", "Area"], keepsKeys: [false, subject.stories.length === 0, true, false] },
          h(ArticleList, { site, ids: subject.articles, shown: 14, autoFocus: true, empty: "no articles yet" }),
          [h(StoryList, { site, stories: subject.stories, shown: 12, itemRows: 1, autoFocus: true, empty: "no podcast story about this yet" }), h(Gap, { rows: 0, grow: 1 }), h(NowPlaying, {})],
          h(SubjectYears, { site, subject }),
          [h(Text, { text: `${area.name}: its subjects`, foreground: Colour.Cyan }), h(NumberedRows, { entries: [{ page: map.areaPage.get(area.id) ?? 0, label: `All of ${area.name}`, foreground: Colour.Green }, ...siblings] })],
        ),
      ),
  };
}

function areaPage(site: Site, magazine: Magazine, area: Area): PageSpec {
  const { map } = site;
  const latest = newestOf(site, area.subjects.map((subject) => subject.articles), LATEST_MOST);
  return {
    page: map.areaPage.get(area.id) ?? 0,
    title: area.name,
    root: () =>
      frame(
        magazine.look,
        area.name,
        prompts([magazine.name.split(" ")[0] ?? "Front", SiteMap.magazinePage(magazine.number, 0)], ["Sections", PAGES.sections], ["Podcast", SiteMap.magazinePage(magazine.number, MAGAZINE_PODCAST)], ["Front", PAGES.front]),
        undefined,
        h(NumberedRows, { entries: area.subjects.map((subject) => ({ page: map.subjectPage.get(subject.id) ?? 0, label: subject.name, figure: subject.articles.length })) }),
        h(Band, { text: `NEWEST IN ${area.name.toUpperCase()}`, background: magazine.look.background, foreground: magazine.look.foreground }),
        h(ArticleList, { site, ids: latest, shown: Math.max(4, 18 - area.subjects.length) }),
      ),
  };
}

function magazinePages(site: Site, magazine: Magazine): PageSpec[] {
  const areas = areasOf(site, magazine);
  const subjects = areas.flatMap((area) => area.subjects);
  const latest = newestOf(site, subjects.map((subject) => subject.articles), LATEST_MOST);
  const stories = storiesOf(subjects);
  const front = SiteMap.magazinePage(magazine.number, 0);
  const latestPage = SiteMap.magazinePage(magazine.number, MAGAZINE_LATEST);
  const podcastPage = SiteMap.magazinePage(magazine.number, MAGAZINE_PODCAST);
  return [
    {
      page: front,
      title: magazine.name,
      section: magazine.name,
      root: () =>
        frame(
          magazine.look,
          magazine.name,
          prompts(["Latest", latestPage], ["Podcast", podcastPage], ["Sections", PAGES.sections], ["Front", PAGES.front]),
          undefined,
          h(NumberedRows, { entries: areas.map((area) => ({ page: site.map.areaPage.get(area.id) ?? 0, label: area.name, figure: count(area) })) }),
          h(NumberedRows, {
            entries: [
              { page: latestPage, label: "The newest", figure: latest.length, foreground: Colour.Cyan },
              { page: podcastPage, label: "On the podcast", figure: stories.length, foreground: Colour.Magenta },
            ],
          }),
          h(Band, { text: "NEWEST", background: magazine.look.background, foreground: magazine.look.foreground }),
          h(ArticleList, { site, ids: latest, shown: Math.max(4, 16 - areas.length) }),
        ),
    },
    { page: latestPage, title: `${magazine.name}: newest`, root: () => frame(magazine.look, `${magazine.name}: newest`, prompts(["Front", front], ["Podcast", podcastPage], ["Sections", PAGES.sections], ["Hackaday", PAGES.front]), undefined, h(ArticleList, { site, ids: latest, shown: 20, itemRows: 2 })) },
    {
      page: podcastPage,
      title: `${magazine.name}: podcast`,
      root: () =>
        frame(
          magazine.look,
          `${magazine.name}: podcast`,
          prompts(["Front", front], ["Newest", latestPage], ["Podcast", PAGES.podcast], ["Playing", PAGES.player]),
          undefined,
          h(Text, { text: `${stories.length} stories about ${magazine.name.toLowerCase()}, newest first`, foreground: Colour.Cyan }),
          h(StoryList, { site, stories, shown: 16, itemRows: 2 }),
          h(NowPlaying, {}),
        ),
    },
    ...areas.flatMap((area) => [areaPage(site, magazine, area), ...area.subjects.map((subject) => subjectPage(site, magazine, area, subject))]),
  ];
}

export function sectionPages(site: Site): PageSpec[] {
  return MAGAZINES.flatMap((magazine) => magazinePages(site, magazine));
}
