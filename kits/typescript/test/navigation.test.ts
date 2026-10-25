import assert from "node:assert/strict";
import { test } from "node:test";

import { type Input, Key } from "../src/input.ts";
import { FASTEXT_ROW, Navigation, clickedLink, numberAt, promptAt } from "../src/navigation.ts";
import { Colour, type FastextLinks, Screen } from "../src/screen.ts";

const PAGES = new Set([0x100, 0x101, 0x110, 0x203, 0x402]);
const LINKS: FastextLinks = [0x101, 0x110, null, 0x100];
const typed = (text: string): Input => ({ kind: "text", text });
const key = (name: string, action: "press" | "repeat" | "release" = "press"): Input => ({ kind: "key", action, key: name });

test("three digits typed go to that page", () => {
  const navigation = new Navigation((page) => PAGES.has(page));
  assert.deepEqual(navigation.take(1, typed("1"), {}, LINKS), { taken: true, goto: null });
  assert.deepEqual(navigation.take(1, typed("1"), {}, LINKS), { taken: true, goto: null });
  assert.deepEqual(navigation.take(1, typed("0"), {}, LINKS), { taken: true, goto: 0x110 });
  assert.deepEqual(navigation.take(1, typed("402"), {}, LINKS), { taken: true, goto: 0x402 }, "three at once");
});

test("digit keys count as typed digits, in any script", () => {
  const navigation = new Navigation((page) => PAGES.has(page));
  navigation.take(1, key("2"), {}, LINKS);
  assert.deepEqual(navigation.take(1, key("0", "release"), {}, LINKS), { taken: true, goto: null }, "a release is taken, and counts nothing");
  navigation.take(1, key("٠"), {}, LINKS);
  assert.deepEqual(navigation.take(1, { kind: "key", action: "press", key: "3", text: "3" }, {}, LINKS), { taken: true, goto: 0x203 });
  assert.deepEqual(navigation.take(1, typed("١٠١"), {}, LINKS), { taken: true, goto: 0x101 }, "Arabic-Indic");
});

test("a number the channel does not have goes nowhere, and a first digit must be a magazine", () => {
  const navigation = new Navigation((page) => PAGES.has(page));
  assert.deepEqual(navigation.take(1, typed("199"), {}, LINKS), { taken: true, goto: null });
  // 9 and 0 are no magazine (EN 300 706 clause 3.1), so they start nothing.
  assert.deepEqual(navigation.take(1, typed("90100"), {}, LINKS), { taken: true, goto: 0x100 });
});

test("each session types its own number, and opening a page starts it again", () => {
  const navigation = new Navigation((page) => PAGES.has(page));
  navigation.take(1, typed("11"), {}, LINKS);
  navigation.take(2, typed("1"), {}, LINKS);
  assert.deepEqual(navigation.take(2, typed("00"), {}, LINKS), { taken: true, goto: 0x100 });
  navigation.reset(1);
  assert.deepEqual(navigation.take(1, typed("0"), {}, LINKS), { taken: true, goto: null }, "0 cannot start a number");
  navigation.take(1, typed("11"), {}, LINKS);
  navigation.forget(1);
  assert.deepEqual(navigation.take(1, typed("110"), {}, LINKS), { taken: true, goto: 0x110 });
});

test("the coloured keys and F1 to F4 follow the page's own links", () => {
  const navigation = new Navigation((page) => PAGES.has(page));
  assert.deepEqual(navigation.take(1, key(Key.F1), {}, LINKS), { taken: true, goto: 0x101 });
  assert.deepEqual(navigation.take(1, key(Key.Green), {}, LINKS), { taken: true, goto: 0x110 });
  assert.deepEqual(navigation.take(1, key(Key.F3), {}, LINKS), { taken: true, goto: null }, "yellow has no link here");
  assert.deepEqual(navigation.take(1, key(Key.Blue), {}, LINKS), { taken: true, goto: 0x100 }, "blue follows the cyan link");
  assert.deepEqual(navigation.take(1, key(Key.Red, "repeat"), {}, LINKS), { taken: true, goto: null }, "only a press follows");
  assert.deepEqual(navigation.take(1, key(Key.F4, "release"), {}, LINKS), { taken: true, goto: null });
});

test("a page that uses digits or colour keys itself keeps them", () => {
  const navigation = new Navigation((page) => PAGES.has(page));
  assert.deepEqual(navigation.take(1, typed("101"), { ownsDigits: true }, LINKS), { taken: false });
  assert.deepEqual(navigation.take(1, key("1"), { ownsDigits: true }, LINKS), { taken: false });
  assert.deepEqual(navigation.take(1, key(Key.F1), { ownsDigits: true }, LINKS), { taken: true, goto: 0x101 }, "colour keys still navigate");
  assert.deepEqual(navigation.take(1, key(Key.F1), { ownsColourKeys: true }, LINKS), { taken: false });
  assert.deepEqual(navigation.take(1, typed("101"), { ownsColourKeys: true }, LINKS), { taken: true, goto: 0x101 }, "digits still navigate");
});

test("everything else is the page's", () => {
  const navigation = new Navigation((page) => PAGES.has(page));
  for (const input of [
    typed("c"),
    typed("1a"),
    typed(""),
    key("a"),
    key(Key.PageDown),
    key(Key.Enter),
    { kind: "paste", text: "123" },
    { kind: "mouse", action: "press", button: 0, column: 1, row: 24 },
    { kind: "mouse", action: "move", buttons: 1, column: 1, row: 24 },
    { kind: "focus" },
  ] satisfies Input[]) {
    assert.deepEqual(navigation.take(1, input, {}, LINKS), { taken: false }, JSON.stringify(input));
  }
});

/** Row 24 with a red prompt at column 0 and a green one at column 10, and page numbers on row 5. */
function clickable(): { rows: number[][]; links: FastextLinks } {
  const screen = new Screen();
  screen.coloured(5, 0, Colour.White, "See 200 or 9999, 1A0 or 099");
  screen.coloured(FASTEXT_ROW, 0, Colour.Red, "Red");
  screen.coloured(FASTEXT_ROW, 10, Colour.Green, "Green");
  screen.coloured(FASTEXT_ROW, 20, Colour.White, "plain");
  return { rows: Array.from({ length: 25 }, (_, row) => [...(screen.row(row) ?? [])]), links: [0x200, 0x100, null, null] };
}

test("a click on a prompt follows its link, from its colour code to the next", () => {
  const view = clickable();
  const row = view.rows[FASTEXT_ROW] ?? [];
  assert.deepEqual([0, 3, 9, 10, 15, 19, 21].map((column) => promptAt(row, column)), [0, 0, 0, 1, 1, 1, null], "white is no key's");
  assert.equal(clickedLink(view, 3, FASTEXT_ROW), 0x200);
  assert.equal(clickedLink(view, 12, FASTEXT_ROW), 0x100);
  assert.equal(clickedLink(view, 25, FASTEXT_ROW), null);
  assert.equal(promptAt([0x20, 0x20], 1), null, "no colour code before it");
});

test("a click on three digits standing alone goes to that page", () => {
  const view = clickable();
  const row = view.rows[5] ?? [];
  // "See 200": the digits at columns 5 to 7, after the colour code.
  assert.deepEqual([5, 6, 7].map((column) => numberAt(row, column)), [0x200, 0x200, 0x200]);
  assert.equal(numberAt(row, 4), null, "a space");
  assert.equal(numberAt(row, 13), null, "four digits are no page number");
  assert.equal(numberAt(row, 18), null, "1A0's digits are not three digits in a row");
  assert.equal(numberAt(row, 26), null, "099 is no page: magazines are 1 to 8");
  assert.equal(clickedLink(view, 6, 5), 0x200);
  assert.equal(clickedLink(view, 40, 5), null, "off the row");
  assert.equal(clickedLink(view, 0, 25), null, "off the page");
});
