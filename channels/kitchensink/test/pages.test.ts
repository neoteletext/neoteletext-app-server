// The kitchensink's pages, driven as the server drives the program: every
// page is published and reports nothing, and the pages that take input do
// what their hints say, the pointer pages from their pointer areas.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Harness, shows, text } from "../../../kits/typescript/src/testing.ts";
import { ARTICLES } from "../src/content.ts";
import { Key } from "../src/kit.ts";
import { pages } from "../src/pages.ts";
import { ComponentChannel, readDeclarations } from "../../../kits/typescript-components/src/index.ts";

const SPECS = pages({ pictures: [], qr: new Map() });

function harness(): Harness {
  return new Harness([new ComponentChannel(SPECS)]);
}

test("every page is published at the start, with its title band, and nothing is reported", () => {
  const channel = harness();
  for (const spec of SPECS) {
    const subpages = channel.sharedPages(spec.page);
    assert.ok(subpages.length >= 1, `page ${spec.page.toString(16)}`);
    assert.ok(subpages.length <= 64, `page ${spec.page.toString(16)}: ${subpages.length} sub-pages`);
  }
  assert.deepEqual(channel.errors, []);
  assert.deepEqual(
    channel.sent.filter((message) => message["type"] === "log"),
    [],
  );
});

test("an article's home is its story, its picture, its links and where it appears", () => {
  const channel = harness();
  const article = ARTICLES[0];
  assert.ok(article !== undefined);
  const names = channel.sharedPages(article.page).map((subpage) => text(subpage.rows, 1).trim());
  assert.ok(names[0]?.includes("Story 1/"), names[0]);
  assert.ok(names.some((name) => name.includes("Picture")));
  assert.ok(names.some((name) => name.includes("Links")));
  assert.ok(names.some((name) => name.includes("Where it appears")));
});

test("the greeting is a viewer's own for three seconds, then released", () => {
  const channel = harness();
  channel.join(1, 0x103);
  assert.ok(shows(channel.own(1, 0x103), "Hello, viewer 1"));
  channel.advance(3_000);
  assert.equal(channel.ownPages(1, 0x103), undefined, "released once it matches the shared copy");
});

test("one viewer's vote reaches another viewer's poll through the bus", () => {
  const channel = harness();
  channel.join(1, 0x602);
  channel.join(2, 0x602);
  channel.press(1, Key.Down);
  channel.press(1, Key.Enter);
  assert.ok(shows(channel.own(1, 0x602), "1 votes, live: * is yours"));
  // The shared copy's poll heard the vote too, so viewer 2's copy is the shared one: nothing of its own is sent.
  assert.ok(shows(channel.seen(2, 0x602).rows, "1 votes, live"));
  assert.ok(!shows(channel.seen(2, 0x602).rows, "is yours"), "the mark is the voter's alone");
});

test("the quiz takes the coloured keys and plays the answer's sound", () => {
  const channel = harness();
  channel.join(1, 0x601);
  // The first question's answer is the fourth: the cyan key, which the kit names ColorF3Blue.
  channel.press(1, Key.Blue);
  assert.ok(shows(channel.own(1, 0x601), "score 1 of 1"));
  assert.ok(channel.sent.some((message) => message["type"] === "sound"));
});

test("search: typing echoes at the cursor, Enter finds, Enter on a result reads it", () => {
  const channel = harness();
  channel.join(1, 0x600);
  assert.ok(channel.cursor(1) !== null, "the field places the cursor");
  channel.typeText(1, "snake");
  channel.press(1, Key.Enter);
  assert.ok(shows(channel.own(1, 0x600), "1 found for \"snake\""));
  // The field keeps the keys; a click on the result reads it.
  const rows = channel.own(1, 0x600);
  const row = rows.findIndex((_, index) => index > 3 && text(rows, index).includes("Snake On A Teletext"));
  channel.clickLeft(1, 5, row);
  const snake = ARTICLES.find((article) => article.title.startsWith("Snake"));
  assert.deepEqual(channel.gotos(1), [snake?.page]);
});

test("roulette spins with a sound at each step, stops, and Enter reads where it stopped", () => {
  const channel = harness();
  channel.join(1, 0x700);
  channel.press(1, Key.Enter);
  assert.ok(shows(channel.own(1, 0x700), "Spinning"));
  // Each step's timer sets the next: run the clock through every wake.
  channel.runUntil(channel.now + 5_000);
  assert.ok(shows(channel.own(1, 0x700), "Enter reads it"));
  channel.press(1, Key.Enter);
  assert.equal(channel.gotos(1).length, 1);
});

test("snake moves a frame each tick for its viewer only", () => {
  const channel = harness();
  channel.sharedPublications(0x702);
  channel.join(1, 0x702);
  channel.advance(150);
  const first = channel.own(1, 0x702).map((codes) => codes.join(","));
  channel.advance(150);
  const second = channel.own(1, 0x702).map((codes) => codes.join(","));
  assert.notDeepEqual(first, second);
  assert.equal(channel.sharedPublications(0x702), 0, "the shared copy is the still title screen");
});

test("the shared buttons' count is everyone's", () => {
  const channel = harness();
  channel.join(1, 0x603);
  channel.join(2, 0x603);
  const rows = channel.shared(0x603);
  const press = rows.findIndex((_, index) => text(rows, index).includes("Press me"));
  channel.clickLeft(1, 4, press);
  channel.clickLeft(2, 4, press);
  assert.ok(shows(channel.shared(0x603), "presses, the last by viewer 2"));
  assert.equal(channel.ownPages(1, 0x603), undefined, "nobody has a copy of their own");
});

test("a page can retitle itself in the contents", () => {
  const channel = harness();
  channel.join(1, 0x604);
  const rows = channel.shared(0x604);
  const button = rows.findIndex((_, index) => text(rows, index).includes("Retitle this page"));
  channel.clickLeft(1, 4, button);
  assert.equal(channel.title(0x604), "Retitled 1 times");
});

test("article pages count in decimal, so every one can be keyed", () => {
  const pages = ARTICLES.map((article) => article.page.toString(16));
  assert.ok(pages.every((page) => /^8[0-9]{2}$/u.test(page)), pages.join(" "));
  assert.equal(pages[10], "820");
});

test("the quiz takes F1 to F4 as the coloured keys", () => {
  const channel = harness();
  channel.join(1, 0x601);
  channel.press(1, Key.F4);
  assert.ok(shows(channel.own(1, 0x601), "score 1 of 1"));
});

test("a converted picture's sub-page is named by its band", () => {
  const picture = "PN,10000\r\nOL,1,picture row one\r\nOL,5,picture row five\r\n";
  const channel = new Harness([new ComponentChannel(pages({ pictures: [{ name: "x01", caption: "A caption", group: "a", levels: [{ level: "l25", tti: picture }] }], qr: new Map() }))]);
  const names = channel.sharedPages(0x502).map((subpage) => text(subpage.rows, 1));
  assert.ok(names[0]?.includes("A caption"), names[0]);
  assert.ok(text(channel.sharedPages(0x502)[0]?.rows ?? [], 5).includes("picture row five"));
});

/** What the viewer's latest copy of `page` declares on its first sub-page. */
function declared(channel: Harness, page: number): ReturnType<typeof readDeclarations>[number] | undefined {
  const published = channel.publications().findLast((message) => Number.parseInt(String(message["page"]), 16) === page);
  return readDeclarations(String(published?.["tti"]))[0];
}

test("the pointer page is one whole-page pointer area: a move with no button anywhere on it is marked and said", () => {
  const channel = harness();
  channel.join(1, 0x507);
  assert.deepEqual(declared(channel, 0x507)?.pointer, { areas: [], wholePage: true });
  channel.move(1, 0, 20, 18);
  const rows = channel.own(1, 0x507);
  assert.ok(shows(rows, "The pointer is on row 18, column 20"));
  assert.equal(text(rows, 18)[20], "+");
});

test("the paint page's surface is a pointer area, for the brush's mark", () => {
  const channel = harness();
  channel.join(1, 0x504);
  channel.move(1, 0, 20, 10);
  assert.deepEqual(declared(channel, 0x504)?.pointer, { areas: [{ row: 3, column: 1, width: 39, height: 20 }], wholePage: false });
});

test("the check boxes page says when the pointer comes onto a control and goes off it", () => {
  const channel = harness();
  channel.join(1, 0x605);
  const [first, second] = declared(channel, 0x605)?.controls ?? [];
  assert.ok(first !== undefined && second !== undefined, "its boxes are controls");
  channel.hover(1, "enter", second.column, second.row);
  assert.ok(shows(channel.own(1, 0x605), `Pointer on the control at ${second.row},${second.column}`));
  channel.hover(1, "leave", second.column, second.row);
  assert.ok(shows(channel.own(1, 0x605), `Pointer off the control at ${second.row},${second.column}`));
});
