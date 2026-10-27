import assert from "node:assert/strict";
import { test } from "node:test";

import { Button, Held, type Input, Key, character, clicked, decimalDigit, pressed, readInput, released, typed } from "../src/input.ts";

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
  assert.deepEqual(readInput({ kind: "mouse", action: "press", pressed: true, button: 2, column: 39, row: 24 }), {
    kind: "mouse",
    action: "press",
    button: 2,
    column: 39,
    row: 24,
  });
  assert.deepEqual(readInput({ kind: "mouse", action: "release", pressed: false, button: 0, column: 0, row: 0 }), {
    kind: "mouse",
    action: "release",
    button: 0,
    column: 0,
    row: 0,
  });
  assert.deepEqual(readInput({ kind: "mouse", action: "move", buttons: Held.Left | Held.Right, column: 39, row: 12 }), {
    kind: "mouse",
    action: "move",
    buttons: 5,
    column: 39,
    row: 12,
  });
  assert.deepEqual(readInput({ kind: "wheel", dx: 0, dy: -120, column: 1, row: 2 }), { kind: "wheel", dx: 0, dy: -120, column: 1, row: 2 });
  assert.deepEqual(readInput({ kind: "hover", action: "enter", entered: true, column: 2, row: 20 }), { kind: "hover", action: "enter", column: 2, row: 20 });
  assert.deepEqual(readInput({ kind: "hover", action: "leave", entered: false, column: 2, row: 20 }), { kind: "hover", action: "leave", column: 2, row: 20 });
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
    { kind: "mouse", pressed: true, button: 0, column: 0, row: 0 },
    { kind: "mouse", action: "hold", button: 0, column: 0, row: 0 },
    { kind: "mouse", action: "press", column: 0, row: 0 },
    { kind: "mouse", action: "press", button: 3, column: 0, row: 0 },
    { kind: "mouse", action: "press", button: 0, column: -1, row: 0 },
    { kind: "mouse", action: "move", column: 0, row: 0 },
    { kind: "mouse", action: "move", buttons: 8, column: 0, row: 0 },
    { kind: "wheel", dx: 0.5, dy: 0, column: 0, row: 0 },
    { kind: "hover", action: "hold", column: 0, row: 1 },
    { kind: "hover", column: 0, row: 1 },
    { kind: "hover", action: "enter", row: 1 },
    { kind: "hover", action: "leave", column: 0, row: -1 },
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
  assert.equal(character({ kind: "key", action: "repeat", key: "a", text: "a" }), "a", "a repeat types again");
  assert.equal(character({ kind: "key", action: "press", key: "a" }), undefined, "a key that produced no text (Control and a letter) types nothing");
  assert.equal(character({ kind: "key", action: "release", key: "a", text: "a" }), undefined, "a release types nothing");
  assert.equal(character({ kind: "key", action: "press", key: "Numpad5", text: "5" }), "5", "a functional key's text is typed: the keypad");
  assert.equal(character({ kind: "key", action: "press", key: Key.Enter, text: "\r" }), undefined, "a control character is the key's, not text");
  assert.equal(character({ kind: "key", action: "press", key: Key.F1 }), undefined, "a functional key types nothing");
  assert.equal(character({ kind: "paste", text: "a" }), undefined);
});

test("typing comes from a key's press or repeat with its text, and from raw text, never a paste (INPUT.md section 6.3)", () => {
  assert.equal(typed({ kind: "key", action: "press", key: "e", text: "\u00e9" }), "\u00e9", "a dead key's composed character");
  assert.equal(typed({ kind: "key", action: "press", key: "e", text: "\u00b4e" }), "\u00b4e", "a dead key that did not compose: both characters");
  assert.equal(typed({ kind: "text", text: "\u4f60\u597d" }), "\u4f60\u597d", "an input method's committed text");
  assert.equal(typed({ kind: "key", action: "release", key: "e", text: "e" }), undefined);
  assert.equal(typed({ kind: "key", action: "press", key: "Dead" }), undefined, "the dead key itself");
  assert.equal(typed({ kind: "paste", text: "a" }), undefined);
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

test("a click is the main button pressed, and only that", () => {
  assert.ok(clicked({ kind: "mouse", action: "press", button: Button.Left, column: 0, row: 0 }));
  assert.ok(!clicked({ kind: "mouse", action: "release", button: Button.Left, column: 0, row: 0 }));
  assert.ok(!clicked({ kind: "mouse", action: "press", button: Button.Right, column: 0, row: 0 }));
  assert.ok(!clicked({ kind: "mouse", action: "move", buttons: Held.Left, column: 0, row: 0 }));
  assert.ok(!clicked({ kind: "key", action: "press", key: Key.Enter }));
});
