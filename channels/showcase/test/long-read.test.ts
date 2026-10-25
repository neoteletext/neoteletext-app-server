// The long read's tests of crates/teletext-showcase/src/services/
// tests.rs, ported.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Colour, Mode, mosaicColour } from "../../../kits/typescript/src/kit.ts";
import { Harness, type Rows, text } from "../../../kits/typescript/src/testing.ts";
import { FASTEXT_ROW, labelColumns, labels } from "../src/common.ts";
import { services } from "../src/services.ts";
import {
  BAR_ATTRIBUTE_COLUMN,
  BAR_COLUMN,
  POSITION_ROW,
  TEXT_WIDTH,
  TRACK_CELLS,
  VIEW_FIRST_ROW,
  VIEW_ROWS,
  draw,
  lines,
  thumb,
} from "../src/services/long-read.ts";
import { LONG_READ, navigation } from "../src/services/pages.ts";

const READER = 1;
const OTHER = 2;
/** A mosaic cell with every sixel set (EN 300 706 Table 47). */
const FULL = 0x7f;
/** A mosaic cell with its left column of sixels set: the thin track. */
const TRACK = 0x20 | 0x01 | 0x04 | 0x10;
const LEFT = 0;

/** The first line of the article shown on `rows`: the position row's first number, checked against the view's first row. */
function topOf(rows: Rows): number {
  const position = text(rows, POSITION_ROW);
  const first = Number.parseInt(position.split("Lines ")[1]?.split("-")[0] ?? "", 10);
  const top = first - 1;
  const line = lines()[top];
  assert.ok(line !== undefined && text(rows, VIEW_FIRST_ROW).includes(line.text), `line ${top} is at the top`);
  return top;
}

function opened(): Harness {
  const harness = new Harness(services.applications([]));
  harness.join(READER, LONG_READ);
  return harness;
}

function nowAt(harness: Harness): number {
  return topOf(harness.own(READER, LONG_READ));
}

/** How many copies of the page the reader has been sent. */
function copies(harness: Harness): number {
  return harness.sent.filter((line) => line["type"] === "publish" && line["page"] === "110" && line["owner"] === READER).length;
}

test("the article is long and fits beside the bar", () => {
  const all = lines();
  assert.ok(all.length > 5 * VIEW_ROWS, `${all.length} lines`);
  for (const line of all) {
    assert.ok([...line.text].length <= TEXT_WIDTH, line.text);
    // Only what the English set shows as itself (EN 300 706 Table 36 replaces #, brackets and others).
    assert.match(line.text, /^[A-Za-z0-9 .,:;'()\-!?]*$/u);
  }
});

test("the keys scroll each reader's own page", () => {
  const harness = opened();
  const shared = harness.shared(LONG_READ);
  assert.equal(topOf(shared), 0);
  assert.ok(text(shared, POSITION_ROW).includes(`Lines 1-${VIEW_ROWS} of ${lines().length}`));
  assert.equal(harness.pageModes(READER), Mode.MouseButtons, "the page takes clicks and the wheel");

  const screen = VIEW_ROWS - 1;
  harness.press(READER, "PageDown");
  assert.equal(nowAt(harness), screen);
  harness.typeText(READER, " ");
  assert.equal(nowAt(harness), 2 * screen, "Space pages down too");
  harness.press(READER, "PageUp");
  assert.equal(nowAt(harness), screen);
  harness.press(READER, "ArrowDown");
  assert.equal(nowAt(harness), screen + 1);
  harness.press(READER, "ArrowUp");
  assert.equal(nowAt(harness), screen);

  const last = lines().length - VIEW_ROWS;
  harness.press(READER, "End");
  const end = harness.own(READER, LONG_READ);
  assert.equal(topOf(end), last);
  assert.ok(text(end, POSITION_ROW).includes("100%"));
  const before = copies(harness);
  harness.press(READER, "PageDown");
  assert.equal(copies(harness), before, "no further than the end, so nothing sent");
  assert.equal(nowAt(harness), last);
  harness.press(READER, "Home");
  assert.equal(nowAt(harness), 0);

  // Another reader still starts at the top, and the shared page never moved.
  harness.press(READER, "PageDown");
  harness.join(OTHER, LONG_READ);
  assert.equal(topOf(harness.shared(LONG_READ)), 0);
  assert.equal(nowAt(harness), screen);
});

test("the wheel scrolls three lines a notch, and small turns add up", () => {
  const harness = opened();
  harness.wheel(READER, 120, 10, 10);
  assert.equal(nowAt(harness), 3);
  for (let turn = 0; turn < 3; turn += 1) {
    harness.wheel(READER, 40, 10, 10);
  }
  assert.equal(nowAt(harness), 6, "three thirds of a notch are a notch");
  harness.wheel(READER, -480, 10, 10);
  assert.equal(nowAt(harness), 0, "never above the start");
});

test("the scroll bar shows where the reader is", () => {
  const total = lines().length;
  const [start, length] = thumb(0, total);
  assert.equal(start, 0);
  // 19 of 140 lines shown, of a 19-cell track: 2.6 cells, rounded.
  assert.equal(total, 140, "the figure below is for this length");
  assert.equal(length, 3);
  const [endStart, endLength] = thumb(total - VIEW_ROWS, total);
  assert.equal(endStart + endLength, TRACK_CELLS, "the block ends at the bottom");
  const [middle] = thumb(Math.floor((total - VIEW_ROWS) / 2), total);
  assert.ok(middle > 0 && middle + length < TRACK_CELLS, "halfway is between");

  const harness = opened();
  const atTop = harness.shared(LONG_READ);
  const bottom = VIEW_FIRST_ROW + VIEW_ROWS - 1;
  assert.equal(atTop[VIEW_FIRST_ROW]?.[BAR_COLUMN], FULL);
  assert.equal(atTop[VIEW_FIRST_ROW]?.[BAR_ATTRIBUTE_COLUMN], mosaicColour(Colour.White));
  assert.equal(atTop[bottom]?.[BAR_COLUMN], TRACK);
  assert.equal(atTop[bottom]?.[BAR_ATTRIBUTE_COLUMN], mosaicColour(Colour.Blue));
  harness.press(READER, "End");
  const atEnd = harness.own(READER, LONG_READ);
  assert.equal(atEnd[VIEW_FIRST_ROW]?.[BAR_COLUMN], TRACK);
  assert.equal(atEnd[bottom]?.[BAR_COLUMN], FULL);
  assert.equal(atEnd[bottom]?.[BAR_ATTRIBUTE_COLUMN], mosaicColour(Colour.White));
});

test("the bar is one clean block at every place", () => {
  // A cell has one foreground colour, so a cell with both block and track
  // drew a second, smaller white piece beside the block.
  const all = lines();
  for (let top = 0; top <= all.length - VIEW_ROWS; top += 1) {
    const screen = draw(all, top);
    const cells = Array.from({ length: VIEW_ROWS }, (_, row) => [screen.at(VIEW_FIRST_ROW + row, BAR_ATTRIBUTE_COLUMN), screen.at(VIEW_FIRST_ROW + row, BAR_COLUMN)]);
    for (const [colour, code] of cells) {
      const block = colour === mosaicColour(Colour.White) && code === FULL;
      const track = colour === mosaicColour(Colour.Blue) && code === TRACK;
      assert.ok(block || track, `line ${top}: ${colour} ${code}`);
    }
    const rows = cells.flatMap(([, code], row) => (code === FULL ? [row] : []));
    assert.ok(
      rows.every((row, index) => index === 0 || row === (rows[index - 1] ?? 0) + 1),
      `line ${top}: one run, ${rows.join(",")}`,
    );
    assert.equal(rows[0], thumb(top, all.length)[0]);
  }
});

test("a click on the bar moves a screen", () => {
  const harness = opened();
  const bottom = VIEW_FIRST_ROW + VIEW_ROWS - 1;
  harness.click(READER, LEFT, BAR_COLUMN, bottom);
  assert.equal(nowAt(harness), VIEW_ROWS - 1);
  harness.click(READER, LEFT, BAR_ATTRIBUTE_COLUMN, VIEW_FIRST_ROW);
  assert.equal(nowAt(harness), 0, "the colour cell counts as the bar");
  const before = copies(harness);
  harness.click(READER, LEFT, BAR_COLUMN, VIEW_FIRST_ROW);
  assert.equal(copies(harness), before, "a click on the block does nothing");
  harness.click(READER, LEFT, 10, bottom);
  assert.equal(copies(harness), before, "a click on the text does nothing");
});

test("a click on a prompt follows its link", () => {
  const harness = opened();
  const links = navigation(LONG_READ);
  const [start] = labelColumns(labels(links))[1] ?? [0];
  harness.click(READER, LEFT, start + 1, FASTEXT_ROW);
  assert.deepEqual(harness.gotos(READER), [links[1][1]], `sent to ${links[1][0]}`);
});

// Not in the Rust showcase: the page does not use digits, so they select pages.
test("digits typed on the long read select a page, and space still scrolls", () => {
  const harness = opened();
  harness.typeText(READER, "1");
  harness.typeText(READER, "0");
  harness.typeText(READER, "1");
  assert.deepEqual(harness.gotos(READER), [0x101]);
  harness.typeText(READER, " ");
  assert.equal(nowAt(harness), VIEW_ROWS - 1);
});
