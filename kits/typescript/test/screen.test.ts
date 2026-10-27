import assert from "node:assert/strict";
import { test } from "node:test";

import {
  Attribute,
  COLUMNS,
  Colour,
  CursorFlag,
  ROWS,
  Screen,
  Sixels,
  decimalSubcode,
  mosaicColour,
  pageText,
  textColour,
  tti,
} from "../src/screen.ts";
import { readTti, text } from "../src/read-tti.ts";

const ESC = "\u001b";

/** The TTI lines of a screen, without the 25 OL lines of an empty page that the test does not name. */
function lines(screen: Screen, page: number, subpage: number): string[] {
  return screen.toTti(page, subpage).split("\n");
}

test("a blank page is every row, the PN line naming page and sub-page and SC its sub-code", () => {
  const all = lines(new Screen(), 0x1a0, 0);
  assert.equal(all[0], "PN,1A000");
  assert.equal(all[1], "SC,0000");
  assert.deepEqual(all.slice(2, 2 + ROWS), Array.from({ length: ROWS }, (_, row) => `OL,${row},`));
  assert.equal(all.length, 2 + ROWS + 1, "nothing else, and a newline after the last line");
  assert.equal(all[all.length - 1], "");
});

test("sub-pages are numbered in decimal digits", () => {
  assert.equal(decimalSubcode(1), 0x0001);
  assert.equal(decimalSubcode(10), 0x0010);
  assert.equal(decimalSubcode(79), 0x0079);
  const all = lines(new Screen(), 0x101, 12);
  assert.equal(all[0], "PN,10112");
  assert.equal(all[1], "SC,0012");
});

test("control codes are written in the escape form, ESC and the code plus 0x40", () => {
  const screen = new Screen();
  screen.header("NeoTeletext");
  screen.coloured(1, 0, Colour.Red, "Hi");
  screen.doubleHeight(2, 1, Colour.Yellow, "Big");
  screen.background(3, 0, Colour.Blue);
  screen.codes(4, 0, [Attribute.Flash, Attribute.Steady, Attribute.Separated, Attribute.HoldMosaics, 0x00, 0x1f]);
  const all = lines(screen, 0x100, 0);
  assert.equal(all[2], "OL,0,        NeoTeletext");
  assert.equal(all[3], `OL,1,${ESC}AHi`);
  assert.equal(all[4], `OL,2, ${ESC}M${ESC}CBig`);
  assert.equal(all[5], `OL,3,${ESC}D${ESC}]`);
  assert.equal(all[6], `OL,4,${ESC}H${ESC}I${ESC}Z${ESC}^${ESC}@${ESC}_`);
});

test("every control code escapes, every other code is itself, and the text is plain ASCII", () => {
  const screen = new Screen();
  screen.codes(1, 0, Array.from({ length: 32 }, (_, code) => code));
  screen.codes(2, 0, Array.from({ length: COLUMNS }, (_, offset) => 0x20 + offset));
  screen.codes(3, 0, Array.from({ length: 0x60 - COLUMNS }, (_, offset) => 0x20 + COLUMNS + offset));
  const written = screen.toTti(0x100, 0);
  assert.equal(lines(screen, 0x100, 0)[3], `OL,1,${Array.from({ length: 32 }, (_, code) => ESC + String.fromCharCode(code + 0x40)).join("")}`);
  for (const character of written) {
    const code = character.charCodeAt(0);
    assert.ok(code === 0x0a || code === 0x1b || (code >= 0x20 && code <= 0x7f), `code ${code}`);
  }
  const [read] = readTti(written);
  assert.ok(read !== undefined);
  for (const row of [1, 2, 3]) {
    assert.deepEqual(read.rows[row], screen.row(row), `row ${row} reads back as written`);
  }
});

test("codes are seven bits and cut at the row's end", () => {
  const screen = new Screen();
  screen.codes(5, 38, [0xc1, 0x42, 0x43]);
  assert.equal(screen.at(5, 38), 0x41);
  assert.equal(screen.at(5, 39), 0x42);
  screen.text(30, 0, "outside");
  screen.codes(5, -1, [0x41]);
  assert.equal(screen.row(30), undefined);
  assert.equal(screen.at(30, 0), 0x20);
});

test("trailing spaces are left out of an OL line, which the reader restores", () => {
  const screen = new Screen();
  screen.text(6, 2, "x   ");
  assert.equal(lines(screen, 0x100, 0)[8], "OL,6,  x");
});

test("text goes through the English set", () => {
  const screen = new Screen();
  screen.text(1, 0, "#1 £5 [x]");
  assert.equal(screen.rowText(1).trimEnd(), "_1 #5 (x)");
});

test("centred text is centred with its colour cell", () => {
  const screen = new Screen();
  screen.centred(1, Colour.White, "abcd");
  // Five cells, colour first, in forty: from column 17.
  assert.equal(screen.at(1, 17), textColour(Colour.White));
  assert.equal(screen.rowText(1).slice(18, 22), "abcd");
});

test("Fastext links are an FL line, 8FF for none, the index link sixth", () => {
  const screen = new Screen();
  screen.fastext([0x101, null, 0x1ff, 0x800]);
  screen.indexLink(0x100);
  assert.ok(lines(screen, 0x100, 0).includes("FL,101,8FF,1FF,800,8FF,100"));
  const [read] = readTti(screen.toTti(0x100, 0));
  assert.deepEqual(read?.links, [0x101, null, 0x1ff, 0x800, null, 0x100]);
});

test("a page with no links has no FL line unless its row 24 is to show", () => {
  const screen = new Screen();
  assert.ok(!screen.toTti(0x100, 0).includes("FL,"));
  screen.showFastextRow();
  assert.ok(lines(screen, 0x100, 0).includes("FL,8FF,8FF,8FF,8FF,8FF,8FF"));
});

test("the cursor is a CU line, and a page has no IM line: it asks for no input (INPUT.md section 6.3)", () => {
  const screen = new Screen();
  screen.cursor(5, 14, 16, CursorFlag.Echo | CursorFlag.Masked);
  const all = lines(screen, 0x201, 0);
  assert.ok(all.includes("CU,5,14,16,3"));
  assert.ok(!all.some((line) => line.startsWith("IM,")), all.join("\n"));
  const [read] = readTti(screen.toTti(0x201, 0));
  assert.deepEqual(read?.cursor, { row: 5, column: 14, width: 16, flags: 3 });
});

test("several sub-pages are one TTI text, each from its PN line", () => {
  const first = new Screen();
  first.text(1, 0, "one");
  const second = new Screen();
  second.text(1, 0, "two");
  const read = readTti(tti(0x101, [{ screen: first, subpage: 1 }, { screen: second, subpage: 2 }]));
  assert.deepEqual(read.map((page) => [page.subpage, text(page.rows, 1).trimEnd()]), [[1, "one"], [2, "two"]]);
});

test("a sixel canvas draws mosaic cells after its colour (EN 300 706 Table 47)", () => {
  const canvas = new Sixels(4, 6);
  canvas.set(0, 0, true); // top left: bit 0
  canvas.set(1, 2, true); // bottom right: bit 6
  canvas.set(3, 3, true); // the second row's cell to the right, top right: bit 1
  canvas.set(9, 9, true); // outside: nothing
  assert.equal(canvas.cell(0, 0), 0x20 | 0x01 | 0x40);
  assert.equal(canvas.cell(2, 3), 0x20 | 0x02);
  assert.equal(canvas.get(9, 9), false);
  const screen = new Screen();
  canvas.draw(screen, 10, 5, Colour.Green);
  assert.deepEqual(screen.row(10)?.slice(4, 7), [mosaicColour(Colour.Green), 0x61, 0x20]);
  assert.deepEqual(screen.row(11)?.slice(4, 7), [mosaicColour(Colour.Green), 0x20, 0x22]);
  assert.equal(lines(screen, 0x100, 0)[12], `OL,10,    ${ESC}Ra`);
  const full = new Sixels(2, 3);
  for (let y = 0; y < 3; y += 1) {
    full.set(0, y, true);
    full.set(1, y, true);
  }
  assert.equal(full.cell(0, 0), 0x7f, "every sixel set");
  full.clear();
  assert.equal(full.cell(0, 0), 0x20);
});

test("page numbers are three hexadecimal digits", () => {
  assert.equal(pageText(0x100), "100");
  assert.equal(pageText(0x1a0), "1A0");
  assert.equal(pageText(0x8ff), "8FF");
});

test("a clone is drawn on apart", () => {
  const screen = new Screen();
  screen.text(1, 0, "a");
  const copy = screen.clone();
  copy.text(1, 0, "b");
  assert.equal(screen.rowText(1)[0], "a");
  assert.equal(copy.rowText(1)[0], "b");
});
