// crates/teletext-showcase/src/interactive/tests.rs, ported: the area
// driven as the server drives it, sessions that type, press keys and
// click, and the pages it publishes read back.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Colour, CursorFlag, Key, Mode, mosaicColour } from "../../../kits/typescript/src/kit.ts";
import { Harness, rowOf, shows, text } from "../../../kits/typescript/src/testing.ts";
import { interactive } from "../src/interactive.ts";
import { Problem, evaluate } from "../src/interactive/calculator.ts";
import { printable } from "../src/interactive/field.ts";
import { ENTRIES_KEPT, MESSAGE_LIMIT, POST_INTERVAL_MS } from "../src/interactive/guestbook.ts";
import { CALCULATOR, FORM, GUESTBOOK, MENU, PAINT, POLL, QUIZ, REPUBLISH_INTERVAL_MS, SIGN } from "../src/interactive/pages.ts";
import { FIRST_COLUMN, PALETTE, PALETTE_COLUMN, PALETTE_ROW, SOLID, SWATCH_CELLS, TOP } from "../src/interactive/paint.ts";
import { BAR_CELLS, CHANGE_INTERVAL_MS } from "../src/interactive/poll.ts";
import { QUESTIONS } from "../src/interactive/quiz.ts";

/** A mosaic cell with its top two rows of sixels set, as a poll bar's cells are (EN 300 706 Table 47). */
const BAR_CELL = 0x2f;
const LEFT = 0;
const RIGHT = 2;
const SPACE = 0x20;

function area(): Harness {
  return new Harness(interactive.applications());
}

test("the form echoes, masks and greets", () => {
  const harness = area();
  harness.join(1, FORM);
  const first = harness.cursor(1);
  assert.ok(first !== null);
  assert.equal(first.flags, CursorFlag.Echo);
  assert.equal(first.width, 16, "an empty field's room");

  harness.typeText(1, "Ada");
  const cursor = harness.cursor(1);
  assert.deepEqual([cursor?.row, cursor?.column, cursor?.width], [first.row, first.column + 3, 13], "the cursor after what was typed");
  assert.ok(text(harness.own(1, FORM), first.row).includes("Ada"));

  harness.press(1, Key.Backspace);
  harness.typeText(1, "a\u{1F600}[ ");
  assert.ok(text(harness.own(1, FORM), first.row).includes("Ada "), "what the page cannot show is dropped");
  assert.equal(harness.cursor(1)?.column, first.column + 4);

  harness.press(1, Key.Tab);
  harness.typeText(1, "secret");
  const masked = harness.cursor(1);
  assert.equal(masked?.flags, CursorFlag.Echo | CursorFlag.Masked);
  const page = harness.own(1, FORM);
  const secretRow = masked?.row ?? 0;
  assert.notEqual(secretRow, first.row, "Tab moved the cursor");
  assert.ok(text(page, secretRow).includes("******"), text(page, secretRow));
  assert.ok(!shows(page, "secret"), "the secret is never on the page");

  harness.press(1, Key.Enter);
  assert.ok(shows(harness.own(1, FORM), "Hello, Ada!"));
  assert.ok(shows(harness.own(1, FORM), "has 6 characters"));
});

test("a full field stops, and stops the echo", () => {
  const harness = area();
  harness.join(1, FORM);
  harness.typeText(1, "abcdefghijklmnopqrstuvwxyz");
  const cursor = harness.cursor(1);
  assert.deepEqual([cursor?.width, cursor?.flags], [0, 0], "no room, so no echo");
  assert.ok(cursor !== null && cursor.row !== 255, "the cursor stays at the end");
  const page = harness.own(1, FORM);
  assert.ok(shows(page, "abcdefghijklmnop"));
  assert.ok(!shows(page, "abcdefghijklmnopq"));
});

test("the guestbook is shared and bounded", () => {
  const harness = area();
  harness.join(1, GUESTBOOK);
  harness.typeText(1, "Hello from one");
  assert.ok(harness.gotos(1).includes(SIGN), "typing on the book goes to the signing page");
  harness.open(1, SIGN);
  assert.ok(shows(harness.own(1, SIGN), "Hello from one"), "with what was typed");
  harness.sharedPublications(GUESTBOOK);
  harness.press(1, Key.Enter);
  assert.equal(harness.gotos(1).at(-1), GUESTBOOK, "back to the book");
  assert.equal(harness.sharedPublications(GUESTBOOK), 1, "one publication, shared");
  assert.ok(shows(harness.shared(GUESTBOOK), "Hello from one"));

  harness.open(1, SIGN);
  harness.typeText(1, "Again");
  harness.press(1, Key.Enter);
  assert.ok(!shows(harness.shared(GUESTBOOK), "Again"), "too soon");
  assert.ok(shows(harness.own(1, SIGN), "Wait 5 more seconds"));

  harness.advance(POST_INTERVAL_MS);
  harness.press(1, Key.Enter);
  assert.ok(shows(harness.shared(GUESTBOOK), "Again"), "after the interval");

  const long = "x".repeat(MESSAGE_LIMIT + 10);
  harness.join(2, SIGN);
  harness.typeText(2, long);
  harness.press(2, Key.Enter);
  harness.advance(REPUBLISH_INTERVAL_MS);
  const book = harness.shared(GUESTBOOK);
  assert.ok(shows(book, "x".repeat(MESSAGE_LIMIT)));
  assert.ok(!shows(book, "x".repeat(MESSAGE_LIMIT + 1)));

  for (let session = 10; session < 30; session += 1) {
    harness.join(session, SIGN);
    harness.typeText(session, `Viewer ${session}`);
    harness.press(session, Key.Enter);
  }
  harness.advance(REPUBLISH_INTERVAL_MS);
  const full = harness.shared(GUESTBOOK);
  assert.ok((rowOf(full, "Viewer 29") ?? 99) < (rowOf(full, "Viewer 28") ?? 0), "newest first");
  let kept = 0;
  for (let session = 10; session < 30; session += 1) {
    kept += shows(full, `Viewer ${session}`) ? 1 : 0;
  }
  assert.equal(kept, ENTRIES_KEPT);
  assert.ok(!shows(full, "Hello from one"), "the oldest went");
});

test("a burst is coalesced", () => {
  const harness = area();
  for (let session = 1; session <= 20; session += 1) {
    harness.join(session, POLL);
  }
  harness.sharedPublications(POLL);
  for (let session = 1; session <= 20; session += 1) {
    harness.typeText(session, "1");
  }
  assert.equal(harness.sharedPublications(POLL), 1, "the first change at once, the rest wait");
  harness.advance(REPUBLISH_INTERVAL_MS);
  assert.equal(harness.sharedPublications(POLL), 1, "the rest in one");
  const poll = harness.shared(POLL);
  const news = rowOf(poll, "1 News") ?? 0;
  assert.ok(text(poll, news).includes(" 20  100%"), text(poll, news));
  harness.advance(REPUBLISH_INTERVAL_MS);
  assert.equal(harness.sharedPublications(POLL), 0, "nothing more to send");
});

test("votes count once per session", () => {
  const harness = area();
  for (let session = 1; session <= 3; session += 1) {
    harness.join(session, POLL);
  }
  harness.typeText(1, "2");
  harness.typeText(1, "2");
  harness.typeText(2, "2");
  harness.typeText(3, "4");
  harness.typeText(3, "9");
  harness.advance(REPUBLISH_INTERVAL_MS);
  const poll = harness.shared(POLL);
  assert.ok(shows(poll, "3 votes"));
  const weather = rowOf(poll, "2 Weather") ?? 0;
  assert.ok(text(poll, weather).includes("  2   66%"), text(poll, weather));

  harness.typeText(1, "3");
  harness.advance(REPUBLISH_INTERVAL_MS);
  assert.ok(text(harness.shared(POLL), weather).includes("  2   66%"), "a change within the interval is refused");

  harness.advance(CHANGE_INTERVAL_MS);
  harness.typeText(1, "3");
  harness.advance(REPUBLISH_INTERVAL_MS);
  const changed = harness.shared(POLL);
  assert.ok(text(changed, weather).includes("  1   33%"), text(changed, weather));
  assert.ok(shows(changed, "3 votes"));

  harness.close(3);
  harness.advance(REPUBLISH_INTERVAL_MS);
  assert.ok(shows(harness.shared(POLL), "2 votes"), "the ended session's vote went");
});

test("the results have bars", () => {
  const harness = area();
  harness.join(1, POLL);
  harness.join(2, POLL);
  harness.typeText(1, "1");
  harness.typeText(2, "3");
  harness.advance(REPUBLISH_INTERVAL_MS);
  const poll = harness.shared(POLL);
  const barCells = (label: string): number => (poll[(rowOf(poll, label) ?? 0) + 1] ?? []).filter((code) => code === BAR_CELL).length;
  assert.equal(barCells("1 News"), BAR_CELLS / 2);
  assert.equal(barCells("3 Sport"), BAR_CELLS / 2);
  assert.equal(barCells("2 Weather"), 0);
});

test("the quiz scores", () => {
  const harness = area();
  harness.join(1, QUIZ);
  QUESTIONS.forEach((question, number) => {
    // The first two answered wrong, the rest right.
    const answer = number < 2 ? (question.right + 1) % 4 : question.right;
    harness.typeText(1, `${answer + 1}`);
    if (number === 0) {
      assert.ok(shows(harness.own(1, QUIZ), "No: it was 2, 1974"));
    }
  });
  const page = harness.own(1, QUIZ);
  assert.ok(shows(page, "You scored 8 out of 10"), text(page, 8));
  harness.typeText(1, "1");
  assert.ok(shows(harness.own(1, QUIZ), "You scored 8"), "a digit after the end does nothing");
  harness.press(1, Key.Enter);
  assert.ok(shows(harness.own(1, QUIZ), "Question 1 of 10"), "Enter plays again");
});

test("sums are checked", () => {
  assert.deepEqual(evaluate("2+3*4"), { value: 14n });
  assert.deepEqual(evaluate("(2+3)*4"), { value: 20n });
  assert.deepEqual(evaluate(" -7 / 2 "), { value: -3n });
  assert.deepEqual(evaluate("--5"), { value: 5n });
  assert.deepEqual(evaluate("10-2-3"), { value: 5n });
  assert.deepEqual(evaluate("1/0"), { problem: Problem.DivisionByZero });
  assert.deepEqual(evaluate("5/(3-3)"), { problem: Problem.DivisionByZero });
  assert.deepEqual(evaluate("9223372036854775807+1"), { problem: Problem.Overflow });
  assert.deepEqual(evaluate("99999999999999999999"), { problem: Problem.Overflow });
  assert.deepEqual(evaluate("-9223372036854775807-1"), { value: -(2n ** 63n) });
  assert.deepEqual(evaluate("(-9223372036854775807-1)/-1"), { problem: Problem.Overflow });
  assert.deepEqual(evaluate("3000000000*4000000000"), { problem: Problem.Overflow });
  assert.deepEqual(evaluate("(2"), { problem: Problem.Unreadable });
  assert.deepEqual(evaluate("2)"), { problem: Problem.Unreadable });
  assert.deepEqual(evaluate("2 3"), { problem: Problem.Unreadable });
  assert.deepEqual(evaluate(""), { problem: Problem.Unreadable });
  assert.deepEqual(evaluate("(".repeat(1_000)), { problem: Problem.Unreadable }, "deep nesting is refused, not recursed");
  assert.deepEqual(evaluate("-".repeat(1_000)), { problem: Problem.Unreadable });
});

test("the calculator answers on its page", () => {
  const harness = area();
  harness.join(1, CALCULATOR);
  harness.typeText(1, "6x7");
  assert.ok(shows(harness.own(1, CALCULATOR), "6x7"));
  harness.press(1, Key.Enter);
  assert.ok(shows(harness.own(1, CALCULATOR), "= 42"));
  harness.typeText(1, "1/0");
  harness.press(1, Key.Enter);
  assert.ok(shows(harness.own(1, CALCULATOR), "Cannot divide by zero"));
  harness.typeText(1, "9223372036854775807*2");
  harness.press(1, Key.Enter);
  assert.ok(shows(harness.own(1, CALCULATOR), "Too big for 64 bits"));
});

test("the paint box paints", () => {
  const harness = area();
  harness.join(1, PAINT);
  assert.equal(harness.pageModes(1), Mode.MouseButtons);

  const [column, row] = [FIRST_COLUMN + 4, TOP + 2];
  harness.click(1, LEFT, column, row);
  assert.equal(harness.own(1, PAINT)[row]?.[column], SOLID);

  const red = PALETTE.indexOf(Colour.Red);
  harness.click(1, LEFT, PALETTE_COLUMN + red * SWATCH_CELLS + 1, PALETTE_ROW);
  harness.click(1, LEFT, column + 2, row);
  const page = harness.own(1, PAINT);
  assert.equal(page[row]?.[column + 2], SOLID);
  assert.equal(page[row]?.[column + 1], mosaicColour(Colour.Red), "the block's own colour");
  assert.notEqual(page[row]?.[column - 1], mosaicColour(Colour.Red), "the first block keeps its colour");

  harness.click(1, LEFT, column + 2, row);
  assert.equal(harness.own(1, PAINT)[row]?.[column + 2], SPACE, "clicked again in its colour: rubbed out");
  harness.click(1, RIGHT, column, row);
  assert.equal(harness.own(1, PAINT)[row]?.[column], SPACE, "the right button rubs out");
  harness.click(1, LEFT, column + 2, row);

  harness.typeText(1, "c");
  assert.equal(harness.own(1, PAINT)[row]?.[column + 2], SPACE, "cleared");
});

test("only printable characters pass", () => {
  for (const typed of ["a", "Z", "7", " ", "!", "#", "£", "@", "?"]) {
    assert.equal(printable(typed), typed, typed);
  }
  for (const typed of ["[", "\\", "]", "^", "_", "`", "{", "|", "}", "~", "\u007f", "\n", "é", "\u{1F600}"]) {
    assert.equal(printable(typed), undefined, JSON.stringify(typed));
  }
});

test("an ended session is forgotten", () => {
  const harness = area();
  harness.join(1, FORM);
  harness.typeText(1, "Ada");
  harness.close(1);
  harness.join(1, FORM);
  assert.ok(!shows(harness.own(1, FORM), "Ada"), "a new session with the same number starts afresh");
});

// Not in the Rust showcase: pages that use digits keep them, and the rest
// of the area selects pages by them (INPUT.md section 9.4).
test("digits type where the page uses them and select pages where it does not", () => {
  const harness = area();
  harness.join(1, MENU);
  harness.typeText(1, "204");
  assert.deepEqual(harness.gotos(1), [QUIZ], "the menu takes no digits");
  harness.open(1, PAINT);
  harness.typeText(1, "205");
  assert.deepEqual(harness.gotos(1), [QUIZ, CALCULATOR], "nor does the paint box");
  harness.open(1, CALCULATOR);
  harness.typeText(1, "206");
  assert.ok(shows(harness.own(1, CALCULATOR), "206"), "the calculator types them");
  assert.deepEqual(harness.gotos(1), [QUIZ, CALCULATOR]);
  harness.press(1, Key.F1);
  assert.deepEqual(harness.gotos(1), [QUIZ, CALCULATOR, MENU], "its red link, Input");
});
