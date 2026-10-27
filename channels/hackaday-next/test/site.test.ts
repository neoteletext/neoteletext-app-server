// The site, driven as the server drives it, over a small archive made here:
// every page is published and reports nothing; a list opens the reader on
// the article chosen; an article's podcast view lists the stories about it;
// a story plays as a live sound; a subject's page lists its articles; the
// sound game's rounds and buttons are controls.

import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";

import { Harness, text } from "../../../kits/typescript/src/testing.ts";
import { BAR_CELLS, Button, ComponentChannel, readControls } from "../../../kits/typescript-components/src/index.ts";
import { Clips } from "../src/clips.ts";
import { COLUMNS, Key } from "../src/kit.ts";
import { Library } from "../src/library.ts";
import { pages } from "../src/pages.ts";
import { PAGES, page, SiteMap } from "../src/site.ts";

/** A small archive: two writers, three posts, the 36 areas each with a subject, one episode of two stories. */
function archive(): DatabaseSync {
  const db = new DatabaseSync(":memory:");
  db.exec(`
    CREATE TABLE articles (id INTEGER PRIMARY KEY, kind TEXT, title TEXT, slug TEXT, link TEXT, date_gmt TEXT, modified_gmt TEXT, writer INTEGER, format TEXT, sticky INTEGER, excerpt TEXT, text TEXT, words INTEGER, links TEXT);
    CREATE TABLE writers (id INTEGER PRIMARY KEY, name TEXT);
    CREATE TABLE readings (article INTEGER, prompt TEXT, answer TEXT);
    CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT);
    CREATE TABLE article_tags (article INTEGER, tag INTEGER);
    CREATE TABLE areas (id INTEGER PRIMARY KEY, name TEXT);
    CREATE TABLE subjects (id INTEGER PRIMARY KEY, area INTEGER, name TEXT, scope TEXT);
    CREATE TABLE article_subjects (article INTEGER, subject INTEGER, main INTEGER);
    CREATE TABLE podcast_episodes (episode INTEGER PRIMARY KEY, number INTEGER, published TEXT, title TEXT, post INTEGER, duration REAL);
    CREATE TABLE podcast_hosts (episode INTEGER, host TEXT);
    CREATE TABLE podcast_stories (episode INTEGER, story INTEGER, title TEXT, start REAL, end REAL, summary TEXT, kind TEXT, articles TEXT, audio TEXT);
    CREATE TABLE story_subjects (episode INTEGER, story INTEGER, subject INTEGER, main INTEGER);
    CREATE TABLE story_tags (episode INTEGER, story INTEGER, tag INTEGER);
  `);
  db.exec(`INSERT INTO writers VALUES (1, 'Ada Fernsby'), (2, 'Tom Hallett');`);
  const article = db.prepare("INSERT INTO articles VALUES (?, 'post', ?, '', ?, ?, '', ?, 'standard', 0, '', ?, 100, ?)");
  article.run(11, "A Teletext Inserter", "https://hackaday.com/a", "2026-10-25T10:00:00", 1, "First paragraph.\nSecond paragraph.", JSON.stringify([{ href: "https://example.com", text: "a link" }]));
  article.run(12, "Mosaic Graphics Explained", "https://hackaday.com/b", "2026-10-24T10:00:00", 2, "Six sixels to a cell.", "[]");
  article.run(13, "Show Notes", "https://hackaday.com/c", "2026-07-24T10:00:00", 1, "The notes.", "[]");
  db.exec(`INSERT INTO readings VALUES (11, 'haiku-1', '{"summary":"An inserter for teletext.","kind":"project","topics":["inserter"],"themes":["broadcast"]}');`);
  db.exec(`INSERT INTO tags VALUES (1, 'teletext'); INSERT INTO article_tags VALUES (11, 1), (12, 1);`);
  const area = db.prepare("INSERT INTO areas VALUES (?, ?)");
  const subject = db.prepare("INSERT INTO subjects VALUES (?, ?, ?, ?)");
  for (let id = 1; id <= 36; id += 1) {
    area.run(id, `Area ${id}`);
    subject.run(id, id, `Subject ${id}`, `What subject ${id} covers.`);
  }
  db.exec(`INSERT INTO article_subjects VALUES (11, 1, 1), (12, 1, 1), (13, 2, 1);`);
  db.exec(`INSERT INTO podcast_episodes VALUES (1, 379, '2026-07-24', 'Ep 379: Teletext Lives', 13, 3600);`);
  db.exec(`INSERT INTO podcast_hosts VALUES (1, 'Elliot Williams');`);
  db.exec(`INSERT INTO podcast_stories VALUES (1, 1, 'The Teletext Inserter', 60, 180, 'They talk about the inserter.', 'project', '[11]', 'audio/0001-01.opus'), (1, 2, 'Mosaics', 200, 260, 'Sixels.', 'news', '[12]', 'audio/0001-02.opus');`);
  db.exec(`INSERT INTO story_subjects VALUES (1, 1, 1, 1), (1, 2, 1, 1);`);
  db.exec(`INSERT INTO story_tags VALUES (1, 1, 1);`);
  return db;
}

function site(manifest: string | null = null): { harness: Harness; map: SiteMap } {
  const library = new Library(archive(), manifest);
  const map = new SiteMap(library);
  const specs = pages({ library, map, clips: new Clips("ffmpeg-not-here"), analysis: "missing", roundClips: "missing" });
  return { harness: new Harness([new ComponentChannel(specs)], { epochMs: Date.UTC(2026, 9, 27) }), map };
}

test("every page is published at the start and nothing is reported", () => {
  const { harness } = site();
  for (const number of [PAGES.front, PAGES.latest, PAGES.sections, PAGES.podcast, PAGES.episodes, PAGES.rounds, page(200), page(210)]) {
    assert.ok(harness.sharedPages(number).length >= 1, number.toString(16));
  }
  assert.deepEqual(harness.errors, []);
  assert.deepEqual(harness.sent.filter((message) => message["type"] === "log"), []);
});

test("choosing an article in the latest opens the reader on it, and its podcast view lists the story about it", () => {
  const { harness } = site();
  harness.join(1, PAGES.latest);
  harness.press(1, Key.Enter);
  assert.deepEqual(harness.gotos(1), [PAGES.reader]);
  harness.open(1, PAGES.reader);
  assert.match(text(harness.seen(1, PAGES.reader).rows, 5), /A Teletext Inserter/u);
  harness.press(1, Key.Yellow);
  assert.ok(harness.seen(1, PAGES.reader).rows.some((_, row) => /The Teletext Inserter/u.test(text(harness.seen(1, PAGES.reader).rows, row))));
});

test("a story's Listen starts a live sound bound to its page", () => {
  const { harness } = site();
  harness.join(2, PAGES.episodes);
  harness.press(2, Key.Enter);
  harness.open(2, PAGES.episode);
  harness.press(2, Key.Enter);
  harness.open(2, PAGES.story);
  harness.press(2, Key.Enter);
  const live = harness.sounds(2).filter((message) => "live" in message || "play" in message);
  assert.equal(live.length, 2, "the live object and its play");
});

test("a subject's page lists its articles, newest first", () => {
  const { harness, map } = site();
  const subjectPage = map.subjectPage.get(1) ?? 0;
  harness.join(3, subjectPage);
  const rows = harness.seen(3, subjectPage).rows;
  const lines = rows.map((_, row) => text(rows, row));
  const first = lines.findIndex((line) => line.includes("Teletext Inserter"));
  const second = lines.findIndex((line) => line.includes("Mosaic Graphics"));
  assert.ok(first >= 0 && second > first, lines.join("\n"));
});

/** Two rounds of the sound game, as whatsthatsound's manifest lists them. */
const ROUNDS = [377, 378].map((played, index) => {
  const clip = (name: string, duration: number) => ({ opus: `${name}${played}.opus`, duration });
  return { round: index + 1, play_episode: { number: played, published: `2026-07-${10 + 7 * index}` }, reveal_episode: { number: played + 1 }, answer: "a modem", winner: null, clips: { play: clip("play", 30), sound: clip("sound", 5), reveal: clip("reveal", 20) } };
});

test("the sound game declares its rounds' rows and its three buttons as controls, each button's its band, and its idle player's buttons none", () => {
  const folder = mkdtempSync(join(tmpdir(), "hackaday-rounds-"));
  try {
    const manifest = join(folder, "manifest.json");
    writeFileSync(manifest, JSON.stringify(ROUNDS));
    const { harness } = site(manifest);
    harness.join(1, PAGES.rounds);
    const published = harness.publications().findLast((message) => Number.parseInt(String(message["page"]), 16) === PAGES.rounds);
    const [controls, ...more] = readControls(String(published?.["tti"]));
    assert.deepEqual(more, [], "one sub-page");
    const rows = harness.seen(1, PAGES.rounds).rows;
    const lines = rows.map((_, row) => text(rows, row));
    // The rounds' rows beside the scroll bar, the band's cells from New Background to the cell before Black Background.
    const roundRows = lines.flatMap((line, row) => (/Podcast episode 37[78]/u.test(line) ? [row] : []));
    const buttonRows = lines.flatMap((line, row) => (/play (the challenge|the reveal|just the sound)/u.test(line) ? [row] : []));
    assert.equal(roundRows.length, 2, lines.join("\n"));
    assert.equal(buttonRows.length, 3, lines.join("\n"));
    const band = (row: number): { row: number; column: number; width: number; height: number } => {
      const line = lines[row] ?? "";
      const column = line.indexOf("\x1d");
      return { row, column, width: line.indexOf("\x1c") - column, height: 1 };
    };
    assert.deepEqual(controls, [...roundRows.map((row) => ({ row, column: 1, width: COLUMNS - BAR_CELLS - 2, height: 1 })), ...buttonRows.map(band)]);
    assert.deepEqual(band(roundRows[0] ?? 0), controls?.[0], "the chosen round's control is its band");
    assert.ok(buttonRows.every((row) => band(row).width === Button.widthOf("play just the sound", 2, 2) - 2), "every button one width, its band all but its colour's and Black Background's cells");
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
});
