// The Hackaday archive as the channel reads it: the analysis database made
// on 26 October 2026 (the host's ~/hackaday-analysis/analysis.sqlite, read
// only), and the What's That Sound rounds (whatsthatsound's clips
// manifest). What every list needs is loaded once at the start, small:
// each article's title, date and writer; each subject's and tag's articles,
// newest first; each episode's stories, and each article's stories. An
// article's text, summary and tags are read when a viewer opens it.

import { readFileSync } from "node:fs";
import { DatabaseSync, type StatementSync } from "node:sqlite";

export interface ArticleHead {
  readonly id: number;
  readonly title: string;
  /** Milliseconds since the Unix epoch, UTC. */
  readonly date: number;
  readonly writer: number;
  readonly words: number;
}

export interface Article extends ArticleHead {
  readonly link: string;
  readonly text: string;
  readonly links: readonly { readonly href: string; readonly text: string }[];
  readonly summary: string | null;
  /** Haiku's reading: what kind of post it is, what it is about, and its themes. */
  readonly kind: string | null;
  readonly topics: readonly string[];
  readonly themes: readonly string[];
  readonly tags: readonly string[];
}

export interface Writer {
  readonly id: number;
  readonly name: string;
  readonly articles: readonly number[];
}

export interface Subject {
  readonly id: number;
  readonly area: number;
  readonly name: string;
  readonly scope: string;
  /** Its articles, newest first: those it is the main subject of. */
  readonly articles: readonly number[];
  /** Its stories, newest first: those it is the main subject of. */
  readonly stories: readonly Story[];
}

export interface Area {
  readonly id: number;
  readonly name: string;
  readonly subjects: readonly Subject[];
}

export interface Tag {
  readonly id: number;
  readonly name: string;
  readonly articles: readonly number[];
}

export interface Story {
  readonly episode: number;
  readonly story: number;
  readonly title: string;
  /** Seconds into the episode. */
  readonly start: number;
  readonly end: number;
  readonly summary: string;
  readonly kind: string;
  readonly articles: readonly number[];
  /** The clip, relative to the analysis folder. */
  readonly audio: string;
  /** Its main subject, if it has one. */
  readonly subject: number | null;
}

export interface Episode {
  /** The archive's number, newest first: 1 is the latest. */
  readonly episode: number;
  /** The show's own number, or null for a special. */
  readonly number: number | null;
  readonly published: string;
  readonly title: string;
  /** Its show notes, an article. */
  readonly post: number | null;
  readonly seconds: number;
  readonly hosts: readonly string[];
  readonly stories: readonly Story[];
}

export interface Round {
  readonly round: number;
  /** The show's numbers of the episodes it was played and revealed in. */
  readonly played: number;
  readonly revealed: number;
  readonly playedOn: string;
  readonly answer: string;
  readonly winner: string;
  /** The clips, relative to the clips folder: the segment it was played in, the sound alone, the reveal. */
  readonly clips: { readonly play: string; readonly sound: string; readonly reveal: string };
  readonly soundSeconds: number;
}

/** The fewest articles a tag has to have a page of its own. */
const TAG_LEAST = 25;
/** Haiku's reading of every article, which holds its summary. */
const READING = "haiku-1";

interface ManifestClip {
  readonly opus: string;
  readonly duration: number;
}

interface ManifestRound {
  readonly round: number;
  readonly play_episode: { readonly number: number; readonly published: string };
  readonly reveal_episode: { readonly number: number };
  readonly answer: string;
  readonly winner: string | null;
  readonly clips: { readonly play: ManifestClip; readonly sound: ManifestClip; readonly reveal: ManifestClip };
}

/** Adds `value` to the list `map` holds for `key`. */
function add<K, V>(map: Map<K, V[]>, key: K, value: V): void {
  const list = map.get(key);
  if (list === undefined) {
    map.set(key, [value]);
  } else {
    list.push(value);
  }
}

function parseIds(text: string): number[] {
  const value: unknown = JSON.parse(text);
  return Array.isArray(value) ? value.filter((item): item is number => typeof item === "number") : [];
}

export class Library {
  readonly #db: DatabaseSync;
  readonly #article: StatementSync;
  readonly #summary: StatementSync;
  readonly #tagsOf: StatementSync;
  readonly #storyTags: StatementSync;
  readonly heads: ReadonlyMap<number, ArticleHead>;
  /** Every post, newest first. */
  readonly newest: readonly number[];
  readonly writers: ReadonlyMap<number, Writer>;
  readonly areas: readonly Area[];
  readonly subjects: ReadonlyMap<number, Subject>;
  /** Each article's subjects, its main one first. */
  readonly subjectsOf: ReadonlyMap<number, readonly number[]>;
  /** Tags with a page of their own, most used first. */
  readonly tags: readonly Tag[];
  readonly episodes: readonly Episode[];
  readonly stories: readonly Story[];
  /** The stories that talk about each article. */
  readonly storiesOf: ReadonlyMap<number, readonly Story[]>;
  readonly rounds: readonly Round[];

  /** The archive at `database` (a path, opened read only, or an open database), and the rounds in `manifest` if given. */
  constructor(database: string | DatabaseSync, manifest: string | null) {
    this.#db = typeof database === "string" ? new DatabaseSync(database, { readOnly: true }) : database;
    this.#article = this.#db.prepare("SELECT id, title, date_gmt, writer, words, link, text, links FROM articles WHERE id = ?");
    this.#summary = this.#db.prepare("SELECT answer FROM readings WHERE article = ? AND prompt = ?");
    this.#storyTags = this.#db.prepare("SELECT t.name FROM story_tags s JOIN tags t ON t.id = s.tag WHERE s.episode = ? AND s.story = ? ORDER BY t.name");
    this.#tagsOf = this.#db.prepare("SELECT t.name FROM article_tags a JOIN tags t ON t.id = a.tag WHERE a.article = ? ORDER BY t.name");

    const heads = new Map<number, ArticleHead>();
    for (const row of this.#db.prepare("SELECT id, title, date_gmt, writer, words FROM articles WHERE kind = 'post' ORDER BY date_gmt DESC").iterate()) {
      const head = { id: Number(row["id"]), title: String(row["title"]), date: Date.parse(`${String(row["date_gmt"])}Z`), writer: Number(row["writer"]), words: Number(row["words"]) };
      heads.set(head.id, head);
    }
    this.heads = heads;
    this.newest = [...heads.keys()];

    const byWriter = new Map<number, number[]>();
    for (const id of this.newest) {
      const writer = heads.get(id)?.writer ?? 0;
      add(byWriter, writer, id);
    }
    this.writers = new Map(
      this.#db
        .prepare("SELECT id, name FROM writers")
        .all()
        .map((row) => [Number(row["id"]), { id: Number(row["id"]), name: String(row["name"]), articles: byWriter.get(Number(row["id"])) ?? [] }]),
    );

    const subjectArticles = new Map<number, number[]>();
    const subjectsOf = new Map<number, number[]>();
    for (const row of this.#db.prepare("SELECT article, subject, main FROM article_subjects ORDER BY main DESC").iterate()) {
      const article = Number(row["article"]);
      const subject = Number(row["subject"]);
      add(subjectsOf, article, subject);
      if (Number(row["main"]) === 1 && heads.has(article)) {
        add(subjectArticles, subject, article);
      }
    }
    this.subjectsOf = subjectsOf;
    const newestFirst = (ids: number[]): number[] => ids.sort((a, b) => (heads.get(b)?.date ?? 0) - (heads.get(a)?.date ?? 0));

    const hosts = new Map<number, string[]>();
    for (const row of this.#db.prepare("SELECT episode, host FROM podcast_hosts").iterate()) {
      add(hosts, Number(row["episode"]), String(row["host"]));
    }
    const storySubject = new Map<string, number>();
    for (const row of this.#db.prepare("SELECT episode, story, subject FROM story_subjects WHERE main = 1").iterate()) {
      storySubject.set(`${String(row["episode"])}/${String(row["story"])}`, Number(row["subject"]));
    }
    const stories: Story[] = [];
    const byEpisode = new Map<number, Story[]>();
    for (const row of this.#db.prepare("SELECT episode, story, title, start, end, summary, kind, articles, audio FROM podcast_stories ORDER BY episode, story").iterate()) {
      const episode = Number(row["episode"]);
      const story: Story = {
        episode,
        story: Number(row["story"]),
        title: String(row["title"]),
        start: Number(row["start"]),
        end: Number(row["end"]),
        summary: String(row["summary"]),
        kind: String(row["kind"]),
        articles: parseIds(String(row["articles"])),
        audio: String(row["audio"]),
        subject: storySubject.get(`${episode}/${String(row["story"])}`) ?? null,
      };
      stories.push(story);
      add(byEpisode, episode, story);
    }
    this.stories = stories;
    const storiesOf = new Map<number, Story[]>();
    for (const story of stories) {
      for (const article of story.articles) {
        add(storiesOf, article, story);
      }
    }
    this.storiesOf = storiesOf;
    this.episodes = this.#db
      .prepare("SELECT episode, number, published, title, post, duration FROM podcast_episodes ORDER BY episode")
      .all()
      .map((row) => ({
        episode: Number(row["episode"]),
        number: row["number"] === null ? null : Number(row["number"]),
        published: String(row["published"]),
        title: String(row["title"]),
        post: row["post"] === null ? null : Number(row["post"]),
        seconds: Number(row["duration"]),
        hosts: hosts.get(Number(row["episode"])) ?? [],
        stories: byEpisode.get(Number(row["episode"])) ?? [],
      }));

    const subjectStories = new Map<number, Story[]>();
    for (const story of stories) {
      if (story.subject !== null) {
        add(subjectStories, story.subject, story);
      }
    }
    const subjects = new Map<number, Subject>();
    for (const row of this.#db.prepare("SELECT id, area, name, scope FROM subjects ORDER BY area, id").iterate()) {
      const id = Number(row["id"]);
      subjects.set(id, { id, area: Number(row["area"]), name: String(row["name"]), scope: String(row["scope"]), articles: newestFirst(subjectArticles.get(id) ?? []), stories: subjectStories.get(id) ?? [] });
    }
    this.subjects = subjects;
    this.areas = this.#db
      .prepare("SELECT id, name FROM areas ORDER BY id")
      .all()
      .map((row) => ({ id: Number(row["id"]), name: String(row["name"]), subjects: [...subjects.values()].filter((subject) => subject.area === Number(row["id"])) }));

    const tagArticles = new Map<number, number[]>();
    for (const row of this.#db.prepare("SELECT tag, article FROM article_tags WHERE tag IN (SELECT tag FROM article_tags GROUP BY tag HAVING count(*) >= ?)").iterate(TAG_LEAST)) {
      const tag = Number(row["tag"]);
      if (heads.has(Number(row["article"]))) {
        add(tagArticles, tag, Number(row["article"]));
      }
    }
    const tagNames = new Map(
      this.#db
        .prepare("SELECT id, name FROM tags")
        .all()
        .map((row) => [Number(row["id"]), String(row["name"])]),
    );
    this.tags = [...tagArticles.entries()].map(([id, ids]) => ({ id, name: tagNames.get(id) ?? "", articles: newestFirst(ids) })).sort((a, b) => b.articles.length - a.articles.length || a.name.localeCompare(b.name));

    this.rounds = manifest === null ? [] : (JSON.parse(readFileSync(manifest, "utf8")) as ManifestRound[]).map((round) => ({
      round: round.round,
      played: round.play_episode.number,
      revealed: round.reveal_episode.number,
      playedOn: round.play_episode.published,
      answer: round.answer,
      winner: round.winner ?? "nobody",
      clips: { play: round.clips.play.opus, sound: round.clips.sound.opus, reveal: round.clips.reveal.opus },
      soundSeconds: round.clips.sound.duration,
    }));
  }

  /** An article, whole, or undefined. */
  article(id: number): Article | undefined {
    const row = this.#article.get(id);
    if (row === undefined) {
      return undefined;
    }
    const answer = this.#summary.get(id, READING)?.["answer"];
    const reading: Record<string, unknown> = typeof answer === "string" ? (JSON.parse(answer) as Record<string, unknown>) : {};
    const strings = (value: unknown): string[] => (Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []);
    const summary = reading["summary"];
    const links: unknown = JSON.parse(String(row["links"]));
    return {
      id,
      title: String(row["title"]),
      date: Date.parse(`${String(row["date_gmt"])}Z`),
      writer: Number(row["writer"]),
      words: Number(row["words"]),
      link: String(row["link"]),
      text: String(row["text"]),
      links: Array.isArray(links) ? (links as { href: string; text: string }[]) : [],
      summary: typeof summary === "string" ? summary : null,
      kind: typeof reading["kind"] === "string" ? reading["kind"] : null,
      topics: strings(reading["topics"]),
      themes: strings(reading["themes"]),
      tags: this.#tagsOf.all(id).map((tag) => String(tag["name"])),
    };
  }

  /** A story's tags. */
  storyTags(story: Story): string[] {
    return this.#storyTags.all(story.episode, story.story).map((row) => String(row["name"]));
  }

  /** The episode the show numbered `number`. */
  episodeNumbered(number: number): Episode | undefined {
    return this.episodes.find((episode) => episode.number === number);
  }

  /** Posts whose titles hold every word of `words`, newest first, at most `most`. */
  search(words: string, most: number): number[] {
    const wanted = words.toLowerCase().split(/\s+/u).filter((word) => word !== "");
    const found: number[] = [];
    for (const id of this.newest) {
      const title = this.heads.get(id)?.title.toLowerCase() ?? "";
      if (wanted.every((word) => title.includes(word))) {
        found.push(id);
        if (found.length >= most) {
          break;
        }
      }
    }
    return found;
  }
}
