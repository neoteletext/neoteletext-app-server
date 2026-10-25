// crates/teletext-showcase/src/gallery/tests.rs, ported: the galleries
// built from pictures of the real folder's shape, their pages read back
// from the TTI they publish, and the captions' folding, wrapping and
// cutting; then what the TypeScript port adds: the levels' TTI, the
// menu's folders, page selection, and the collections' own programs and
// their channels.json entries.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { COLUMNS, Key, ROWS, packedLinksLine } from "../../../kits/typescript/src/kit.ts";
import { Harness, type ReadPage, readTti } from "../../../kits/typescript/src/testing.ts";
import { collection, gallery, ofGroup, showcase } from "../src/gallery.ts";
import { CREDITS_SUBPAGES, type Gallery, pageNumber } from "../src/gallery/application.ts";
import { read, subpages } from "../src/gallery/levels.ts";
import { COLLECTIONS, MENU } from "../src/gallery/pages.ts";
import { TITLE_CHARACTERS, contentsTitle, creditLines, cut, fold, wrap } from "../src/gallery/text.ts";
import { LEVELS, type Picture } from "../src/pictures.ts";

/** The showcase's folder, where channels.json starts each program. */
const SHOWCASE = join(dirname(fileURLToPath(import.meta.url)), "..");
/** The repository, which holds channels.json. */
const REPOSITORY = join(SHOWCASE, "..", "..");
/** The most pages a channel declares, the engine's limit, and the most pages and sub-pages a channel's store holds. */
const MOST_PAGES = 800;
const MOST_STORED = 1024;
/** The sub-code of any sub-page (EN 300 706 clause 9.6.1). */
const ANY = null;

/** A level's TTI: one page whose rows 0, 1 and 24 name the level, as the conversion's files fill rows 0 to 24. */
function tti(level: string): string {
  return `DE,Picture\r\nPN,10000\r\nSC,0000\r\nPS,8000\r\nFL,100,100,100,100,100,100\r\nOL,0,${level} top\r\nOL,1,${level} picture\r\nOL,24,${level} bottom\r\n`;
}

function picture(name: string, group: string, caption: string): Picture {
  return { name, caption, group, levels: LEVELS.map((level) => ({ level, tti: tti(level) })) };
}

/** A long Wikimedia caption, as captions.tsv holds some. */
const LONG =
  "Kiril Lazarov 06.jpg · English: Kuebi = Armin Kübelbeck. In case of questions regarding usage of my work, please see here — after you have read the license below.Français: Kuebi = Armin Kübelbeck. Pour toute question concernant l'usage de mes travaux, s.v.p voyez vous référer ici — après avoir lu la licence précédemment citée. · CC BY-SA 3.0";

function two(number: number): string {
  return number.toString().padStart(2, "0");
}

/** The pictures of the real folder's shape: 10 examples, 20 sprites, 221 characters and 133 art pictures with long captions. */
function folder(): Picture[] {
  const pictures: Picture[] = [];
  for (let number = 1; number <= 10; number += 1) {
    pictures.push(picture(`e${two(number)}`, "e", `example ${two(number)}`));
  }
  for (let number = 1; number <= 20; number += 1) {
    pictures.push(picture(`s${two(number)}`, "s", `${two(number)}_razor_crawler (sci_fi_enemy_sprites_individual)`));
  }
  for (let number = 1; number <= 221; number += 1) {
    pictures.push(picture(`c${number.toString().padStart(3, "0")}`, "c", `source-${two(number)}-character-1 (transparent background/all-characters)`));
  }
  for (let number = 1; number <= 133; number += 1) {
    pictures.push(picture(`a${number.toString().padStart(3, "0")}`, "a", LONG));
  }
  return pictures;
}

function byNumber(shown: Gallery): Map<number, ReadPage[]> {
  return new Map(shown.published().map(({ page, tti: text }) => [page, readTti(text)]));
}

function at(pages: Map<number, ReadPage[]>, number: number): ReadPage[] {
  const found = pages.get(number);
  assert.ok(found !== undefined, `page ${number.toString(16)} is published`);
  return found;
}

/** A row's text, its attributes shown as the spaces they take. */
function row(page: ReadPage, number: number): string {
  return String.fromCharCode(...(page.rows[number] ?? []).map((code) => (code < 0x20 ? 0x20 : code)));
}

/** The pages' text as one line, each run of spaces one space, so text wrapped over rows reads on. */
function textOf(pages: readonly ReadPage[]): string {
  const rows = pages.flatMap((page) => Array.from({ length: ROWS }, (_, number) => row(page, number)));
  return rows
    .join(" ")
    .split(/\s+/u)
    .filter((word) => word.length > 0)
    .join(" ");
}

/** A page's six links, each with its sub-code. */
function links(page: ReadPage | undefined): ([number, number | null] | null)[] {
  return (page?.links ?? []).map((link, index) => (link === null ? null : [link, page?.linkSubcodes[index] ?? null]));
}

/** The collection named `key`. */
function spec(key: string): (typeof COLLECTIONS)[number] {
  const found = COLLECTIONS.find((candidate) => candidate.key === key);
  assert.ok(found !== undefined);
  return found;
}

test("page numbers are decimal digits as the runtime numbers them", () => {
  assert.equal(pageNumber(100), 0x100);
  assert.equal(pageNumber(601), 0x601);
  assert.equal(pageNumber(630), 0x630);
  assert.equal(pageNumber(899), 0x899);
});

test("the showcase numbers examples from 601 and sprites from 611", () => {
  const numbers = [...byNumber(showcase(folder())).keys()].sort((a, b) => a - b);
  const expected = [0x600];
  for (let decimal = 601; decimal <= 630; decimal += 1) {
    expected.push(pageNumber(decimal));
  }
  expected.push(0x690, 0x695);
  assert.deepEqual(numbers, expected);
});

test("each picture is one page of its three levels in order", () => {
  const pages = byNumber(showcase(folder()));
  for (let decimal = 601; decimal <= 630; decimal += 1) {
    const number = pageNumber(decimal);
    const levels = at(pages, number);
    assert.equal(levels.length, 3, `page ${decimal}`);
    levels.forEach((subpage, index) => {
      const level = LEVELS[index] ?? "";
      assert.ok(row(subpage, 1).startsWith(`${level} picture`), `page ${decimal}: ${row(subpage, 1)}`);
      assert.ok(row(subpage, 24).startsWith(`${level} bottom`), "the picture keeps row 24");
      assert.equal(subpage.fastextRow, true, "its row 24 shows, under the gallery's links");
    });
    assert.deepEqual(
      levels.map(({ subpage }) => subpage),
      [1, 2, 3],
    );
    // The sub-codes are each sub-page's decimal digits, as its SC line says.
    const text = showcase(folder()).published().find(({ page }) => page === number)?.tti ?? "";
    assert.deepEqual(text.match(/^SC,.*$/gmu), ["SC,0001", "SC,0002", "SC,0003"]);
    assert.deepEqual(text.match(/^PN,.*$/gmu), [1, 2, 3].map((subpage) => `PN,${decimal}0${subpage}`));
  }
});

test("a picture's fastext is previous, next, menu, credit and index", () => {
  const pages = byNumber(showcase(folder()));
  // The first wraps to the last; every level carries the same links; the
  // credit names its sub-page, and the index is the sixth link.
  for (const subpage of at(pages, 0x601)) {
    assert.deepEqual(links(subpage), [[0x630, ANY], [0x602, ANY], [0x600, ANY], [0x695, 0x0001], null, [0x100, ANY]]);
  }
  assert.deepEqual(links(at(pages, 0x630)[0])[1], [0x601, ANY]);
  for (const subpage of at(pages, 0x615)) {
    assert.equal(subpage.links[0], 0x614);
  }
});

test("every credit names its picture on the sub-page its link names", () => {
  const pages = byNumber(collection(folder(), spec("art")));
  for (let decimal = 101; decimal <= 233; decimal += 1) {
    const [page, subcode] = links(at(pages, pageNumber(decimal))[0])[3] ?? [0, null];
    assert.ok(subcode !== null, "the credit names a sub-page");
    // Sub-pages are numbered in decimal digits, so the sub-code's digits are the sub-page's.
    const shown = at(pages, page).find(({ subpage }) => subpage === Number.parseInt(subcode.toString(16), 10));
    assert.ok(shown !== undefined);
    const text = textOf([shown]);
    assert.ok(text.includes(`${decimal} Kiril Lazarov 06, by English: Kuebi = Armin Kubelbeck`), `page ${decimal}:\n${text}`);
    assert.ok(text.includes("CC BY-SA 3.0"), `the licence is kept whole:\n${text}`);
  }
});

test("the credits hold every caption", () => {
  const credits = textOf(at(byNumber(showcase(folder())), 0x695));
  for (let number = 1; number <= 10; number += 1) {
    assert.ok(credits.includes(`example ${two(number)}`));
  }
  assert.ok(credits.includes("01 razor crawler (sci fi enemy sprites individual)"), credits);
});

test("every channel stays within the limits", () => {
  const pictures = folder();
  const galleries = [showcase(pictures), ...COLLECTIONS.filter((candidate) => ofGroup(pictures, candidate.group).length > 0).map((candidate) => collection(pictures, candidate))];
  assert.equal(galleries.length, 3, "the showcase, characters and art");
  for (const shown of galleries) {
    const entries = shown.pages();
    assert.ok(entries.length <= MOST_PAGES, "at most 800 pages a channel");
    for (const entry of entries) {
      const digits = entry.page.toString(16);
      assert.ok(/^[0-9]{3}$/u.test(digits), `contents names decimal pages: ${digits}`);
      assert.ok(entry.page >= 0x100 && entry.page <= 0x899);
      assert.ok(entry.title.length > 0 && [...entry.title].length <= TITLE_CHARACTERS, entry.title);
      assert.ok(/^[ -~]*$/u.test(entry.title), entry.title);
    }
    const published = shown.published();
    const numbers = new Set(published.map(({ page }) => page));
    assert.equal(numbers.size, published.length, "each page published once");
    assert.deepEqual([...numbers].sort(), entries.map(({ page }) => page).sort(), "every page published is in the contents");
    const counts = published.map(({ tti: text }) => readTti(text).length);
    assert.ok(
      counts.every((count) => count <= CREDITS_SUBPAGES),
      "at most 40 sub-pages a page",
    );
    const stored = counts.reduce((sum, count) => sum + count, 0);
    assert.ok(stored <= MOST_STORED, `${stored} pages and sub-pages`);
  }
});

test("pictures beyond the plan are left out", () => {
  const pictures = Array.from({ length: 120 }, (_, index) => picture(`e${index + 1}`, "e", "example"));
  const numbers = showcase(pictures)
    .pages()
    .map(({ page }) => page);
  assert.equal(numbers.filter((number) => number >= 0x601 && number <= 0x689).length, 89);
  assert.ok(!numbers.includes(0x68a) && numbers.includes(0x690) && numbers.includes(0x695));
});

test("a picture without levels is left out and one that does not read says so", () => {
  const second = { ...picture("e02", "e", "second"), levels: [] };
  const third = { ...picture("e03", "e", "third"), levels: [{ level: "l15" as const, tti: "no page here" }] };
  const pages = byNumber(showcase([picture("e01", "e", "first"), second, third]));
  assert.ok(pages.has(0x601) && pages.has(0x602) && !pages.has(0x603));
  assert.ok(textOf(at(pages, 0x602)).includes("could not be read"));
});

test("with no pictures the menu says why and there are no picture pages", () => {
  const pages = byNumber(showcase([]));
  assert.deepEqual([...pages.keys()], [0x600, 0x690]);
  assert.ok(textOf(at(pages, 0x600)).includes("No pictures are installed here"));
});

test("the collections have channels of their own with an index at 100", () => {
  const pictures = folder();
  assert.deepEqual(
    COLLECTIONS.filter((candidate) => ofGroup(pictures, candidate.group).length > 0).map(({ key }) => key),
    ["characters", "art"],
  );
  const pages = byNumber(collection(pictures, spec("characters")));
  const index = textOf(at(pages, 0x100));
  assert.ok(index.includes("221 pictures, on pages 101 to 321."), index);
  assert.equal(at(pages, 0x100)[0]?.links[0], 0x101);
  assert.ok(pages.has(0x321) && !pages.has(0x322) && pages.has(0x800));
  const menu = textOf(at(byNumber(showcase(pictures)), 0x600));
  assert.ok(menu.includes("Characters") && menu.includes("221 pictures") && menu.includes("characters-ts"), menu);
});

test("a channel publishes the pages once at start", () => {
  const harness = new Harness([collection(folder(), spec("characters"))]);
  assert.deepEqual(harness.errors, []);
  const publications = harness.publications();
  assert.equal(publications.length, 1 + 221 + 1);
  assert.ok(publications.every((publication) => publication["owner"] === "channel"));
  const declared = harness.sent[0]?.["pages"] as { page: string; title: string }[];
  assert.ok(declared.some(({ page, title }) => page === "101" && title === "source-01-character-1"), JSON.stringify(declared.slice(0, 3)));
  assert.ok(declared.some(({ page, title }) => page === "800" && title === "Picture credits"));
});

test("captions fold to what the english set shows", () => {
  assert.equal(fold("Armin Kübelbeck · CC"), "Armin Kubelbeck - CC");
  assert.equal(fold("Максат79"), "Maksat79");
  assert.equal(fold("Михайлюк"), "Mikhaylyuk");
  assert.equal(fold("sci_fi [a] {b} a|b ~"), "sci fi (a) (b) a/b -");
  assert.equal(fold("א (Aleph)"), "? (Aleph)");
  assert.equal(fold("ভরত x  \t y "), "? x y");
  // The gallery's own fold: the em dash is a hyphen, and the English
  // set's arrows and fractions, which the Rust could not show, stay unshown.
  assert.equal(fold("a — b ←½ c"), "a - b ? c");
  assert.equal(fold("ভ́র"), "?", "a combining mark does not end a run of the unshown");
  assert.equal(fold("a﻿b"), "a?b", "U+FEFF is not whitespace");
});

test("titles fit the contents and keep the author", () => {
  assert.equal(contentsTitle("Prairie dog Theodore Roosevelt NP ND1.jpg · Acroterion · CC BY-SA 4.0"), "Prairie dog Theodo.., Acroterion");
  assert.equal(contentsTitle("01_razor_crawler (sci_fi_enemy_sprites_individual)"), "01 razor crawler");
  assert.equal(contentsTitle("Kenney tiny-dungeon (CC0), sheet crop of tilemap.png at 1:1"), "Kenney tiny-dungeon (CC0), she..");
  assert.ok([...contentsTitle(LONG)].length <= TITLE_CHARACTERS);
});

test("long captions are shortened but keep their licence", () => {
  const lines = creditLines(LONG, 35, 6);
  assert.ok(lines.length <= 6 && lines.every((line) => [...line].length <= 35), JSON.stringify(lines));
  assert.ok(lines.join(" ").endsWith("CC BY-SA 3.0"), JSON.stringify(lines));
  assert.deepEqual(creditLines("example 01", 35, 6), ["example 01"]);
  assert.equal(
    creditLines("Monitor lizard swallows Uromastyx in Kuwait.jpg · Mfalhajji · CC BY-SA 4.0", 35, 6).at(-1),
    "CC BY-SA 4.0",
    "a licence is never broken",
  );
  assert.deepEqual(wrap("a bb ccc", 4), ["a bb", "ccc"]);
  assert.deepEqual(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
  assert.equal(cut("abcdef", 4), "ab..");
});

test("the index entry names the gallery", () => {
  assert.equal(gallery.sections()[0]?.page, MENU);
});

// What the port adds.

test("a level keeps its own lines but for its number, sub-code and links, and its description is the page's", () => {
  const level = "DE,Drawn\nPN,10000\nSC,0005\nPS,8000\nFL,200,200,200,200,200,200\nOL,27,@" + "O".repeat(39) + "\nXX,unknown\nOL,1,row\nOL,26,@" + "@".repeat(39) + "\n\nPN,10100\nOL,1,second page\n";
  assert.deepEqual(read(level), { head: ["DE,Drawn"], page: ["SC,0005", "PS,8000", "FL,200,200,200,200,200,200", `OL,27,@${"O".repeat(39)}`, "OL,1,row", `OL,26,@${"@".repeat(39)}`] });
  const fastext = [{ page: 0x602, subcode: null }, null, null, null, null, { page: 0x100, subcode: null }];
  assert.equal(subpages([{ subpage: 2, tti: level }], 0x601, fastext), `PN,60102\nSC,0002\nDE,Drawn\nPS,8000\nOL,1,row\nOL,26,@${"@".repeat(39)}\nFL,602,8FF,8FF,8FF,8FF,100\n`);
  const credit = [null, null, null, { page: 0x695, subcode: 0x0003 }, null, null];
  assert.equal(subpages([{ subpage: 1, tti: level }], 0x601, credit)?.split("\n").at(-2), packedLinksLine(0x601, credit, true), "a link to a sub-page is a packed line");
  assert.deepEqual(read("OL,1,before any PN\nDE,late\n"), { head: [], page: ["OL,1,before any PN"] }, "a page without a PN line is a page, as the engine reads it");
  assert.equal(read("no page here"), undefined);
});

test("the menu names each collection's channel by its folder, within the row", () => {
  const screen = at(byNumber(showcase(folder())), 0x600)[0];
  assert.ok(screen !== undefined);
  const folders = COLLECTIONS.map(({ folder: name }) => name);
  assert.ok(folders.every((name) => name.endsWith("-ts")), "the Rust server still serves the plain names");
  const lines = Array.from({ length: ROWS }, (_, number) => row(screen, number));
  for (const name of ["characters-ts", "art-ts"]) {
    const line = lines.find((text) => text.includes(name));
    assert.ok(line !== undefined && line.length === COLUMNS && line.trimEnd().endsWith(name), `${name} whole on its row`);
  }
});

test("digits and the coloured keys select pages from the galleries", () => {
  const harness = new Harness([showcase(folder())]);
  harness.join(1, 0x601);
  harness.typeText(1, "690");
  assert.deepEqual(harness.gotos(1), [0x690], "digits go to a page of the channel");
  harness.open(1, 0x601);
  harness.press(1, Key.Green);
  harness.press(1, Key.Blue);
  harness.press(1, Key.F3);
  assert.deepEqual(harness.gotos(1), [0x690, 0x602, 0x695, 0x600], "next, the credit's page and the menu");
  assert.deepEqual(harness.errors, []);
});

test("channels.json starts each collection's program at its folder, beside the showcase", () => {
  const file = JSON.parse(readFileSync(join(REPOSITORY, "channels.json"), "utf8")) as { channels: { folder: string; name: string; command: string[]; directory: string }[] };
  for (const { key, folder: name, channel } of COLLECTIONS) {
    const entry = file.channels.find((candidate) => candidate.folder === name);
    assert.ok(entry !== undefined, `${name} is in channels.json`);
    assert.equal(entry.name, channel);
    assert.equal(entry.directory, "channels/showcase");
    assert.deepEqual(entry.command.slice(0, 2), ["node", "src/collection.ts"]);
    assert.equal(entry.command[entry.command.indexOf("--collection") + 1], key);
    assert.ok(entry.command.includes("--pictures"));
  }
});

test("a collection's program declares its index, with pictures or without", () => {
  const pictures = mkdtempSync(join(tmpdir(), "gallery-collection-"));
  try {
    writeFileSync(join(pictures, "captions.tsv"), "a001\ta\tA cat · Ada · CC BY 4.0\n");
    writeFileSync(join(pictures, "a001_l15.tti"), tti("l15"));
    const start = '{"type":"start","protocol":1,"epoch_ms":0}\n';
    const run = (folderName: string, key: string): { type: string; pages?: { page: string; title: string }[]; page?: string }[] => {
      const result = spawnSync(process.execPath, ["src/collection.ts", "--pictures", folderName, "--collection", key], { cwd: SHOWCASE, input: start, encoding: "utf8" });
      assert.equal(result.status, 0, result.stderr);
      return result.stdout
        .split("\n")
        .filter((line) => line.length > 0)
        .map((line) => JSON.parse(line) as { type: string });
    };
    const art = run(pictures, "art");
    assert.deepEqual(art[0]?.pages?.map(({ page }) => page), ["100", "101", "800"]);
    assert.deepEqual(art.slice(1).map(({ page }) => page), ["100", "101", "800"], "every page published at start");
    const people = run(pictures, "people");
    assert.deepEqual(people[0]?.pages, [{ page: "100", title: "People" }], "a collection without pictures still has its index");
    const refused = spawnSync(process.execPath, ["src/collection.ts", "--collection", "nothing"], { cwd: SHOWCASE, input: start, encoding: "utf8" });
    assert.notEqual(refused.status, 0, "an unknown collection is refused");
  } finally {
    rmSync(pictures, { recursive: true });
  }
});
