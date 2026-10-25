import assert from "node:assert/strict";
import { test } from "node:test";

import { type Input, Key, character, decimalDigit, pressed, readInput, released } from "../src/input.ts";

test("every kind of event the server writes is read", () => {
  assert.deepEqual(readInput({ kind: "text", text: "hi" }), { kind: "text", text: "hi" });
  assert.deepEqual(readInput({ kind: "paste", text: "data" }), { kind: "paste", text: "data" });
  assert.deepEqual(readInput({ kind: "key", action: "press", key: "ArrowUp" }), { kind: "key", action: "press", key: "ArrowUp" });
  assert.deepEqual(readInput({ kind: "key", action: "repeat", key: "a", text: "A", base: "a" }), {
    kind: "key",
    action: "repeat",
    key: "a",
    text: "A",
    base: "a",
  });
  assert.deepEqual(readInput({ kind: "mouse", pressed: true, button: 2, column: 39, row: 24 }), {
    kind: "mouse",
    pressed: true,
    button: 2,
    column: 39,
    row: 24,
  });
  assert.deepEqual(readInput({ kind: "wheel", dx: 0, dy: -120, column: 1, row: 2 }), { kind: "wheel", dx: 0, dy: -120, column: 1, row: 2 });
  assert.deepEqual(readInput({ kind: "focus" }), { kind: "focus" });
  assert.deepEqual(readInput({ kind: "blur" }), { kind: "blur" });
});

test("an event that is not whole is refused, with the reason", () => {
  for (const bad of [
    null,
    [],
    "text",
    { kind: "text" },
    { kind: "text", text: 7 },
    { kind: "key", action: "hold", key: "a" },
    { kind: "key", action: "press", key: "" },
    { kind: "key", action: "press", key: "a", text: 1 },
    { kind: "key", action: "press", key: "x".repeat(65) },
    { kind: "mouse", pressed: "yes", button: 0, column: 0, row: 0 },
    { kind: "mouse", pressed: true, button: 3, column: 0, row: 0 },
    { kind: "mouse", pressed: true, button: 0, column: -1, row: 0 },
    { kind: "wheel", dx: 0.5, dy: 0, column: 0, row: 0 },
    { kind: "telepathy" },
  ]) {
    assert.equal(typeof readInput(bad), "string", JSON.stringify(bad));
  }
});

test("presses, releases and characters are as the runtime's Input reads them", () => {
  const press: Input = { kind: "key", action: "press", key: Key.Enter };
  const repeat: Input = { kind: "key", action: "repeat", key: Key.Enter };
  const release: Input = { kind: "key", action: "release", key: Key.Enter };
  assert.ok(pressed(press, Key.Enter) && pressed(repeat, Key.Enter) && !pressed(release, Key.Enter));
  assert.ok(released(release, Key.Enter) && !released(press, Key.Enter));
  assert.equal(character({ kind: "text", text: "é" }), "é");
  assert.equal(character({ kind: "text", text: "\u{1F600}" }), "\u{1F600}", "one character, two UTF-16 units");
  assert.equal(character({ kind: "text", text: "ab" }), undefined);
  assert.equal(character({ kind: "key", action: "press", key: "a", text: "A" }), "A", "the text the press produced");
  assert.equal(character({ kind: "key", action: "press", key: "a" }), "a");
  assert.equal(character({ kind: "key", action: "release", key: "a" }), undefined);
  assert.equal(character({ kind: "key", action: "press", key: Key.F1 }), undefined, "a functional key types nothing");
  assert.equal(character({ kind: "paste", text: "a" }), undefined);
});

test("the decimal digits of every script are 0 to 9 (INPUT.md section 5.7 rule 8)", () => {
  assert.equal(decimalDigit("7"), 7);
  assert.equal(decimalDigit("0"), 0);
  assert.equal(decimalDigit("٣"), 3, "Arabic-Indic");
  assert.equal(decimalDigit("۵"), 5, "extended Arabic-Indic");
  assert.equal(decimalDigit("३"), 3, "Devanagari");
  assert.equal(decimalDigit("๙"), 9, "Thai");
  assert.equal(decimalDigit("９"), 9, "full width");
  assert.equal(decimalDigit("\u{1D7D7}"), 9, "the first of the touching mathematical runs");
  assert.equal(decimalDigit("\u{1D7FF}"), 9, "the last of them");
  assert.equal(decimalDigit("\u{116E3}"), 9, "the second of two touching runs");
  for (const notADigit of ["a", "/", ":", "½", "Ⅳ", "²", "ٟ", "\u0000"]) {
    assert.equal(decimalDigit(notADigit), undefined, notADigit);
  }
});
