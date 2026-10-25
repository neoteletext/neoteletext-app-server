import assert from "node:assert/strict";
import { test } from "node:test";

import { ENGLISH, englishCodes, foldCharacter } from "../src/english.ts";

const codesOf = (text: string): number[] => [...text].map((character) => character.charCodeAt(0));

test("ASCII the English set shares is written as itself", () => {
  const shared = "Hello, World! 0123456789 $@ ()*+-./:;<=>?\"'%&";
  assert.deepEqual(englishCodes(shared), codesOf(shared));
});

test("the signs of Table 36's English column are written at their codes", () => {
  assert.deepEqual(englishCodes("£#½¼¾÷—‖"), [
    ENGLISH.pound,
    ENGLISH.hash,
    ENGLISH.half,
    ENGLISH.quarter,
    ENGLISH.threeQuarters,
    ENGLISH.divide,
    ENGLISH.dash,
    ENGLISH.doubleBar,
  ]);
  assert.deepEqual(englishCodes("←→↑"), [ENGLISH.leftArrow, ENGLISH.rightArrow, ENGLISH.upArrow]);
  assert.deepEqual([ENGLISH.pound, ENGLISH.hash, ENGLISH.dash, ENGLISH.half, ENGLISH.quarter, ENGLISH.threeQuarters, ENGLISH.divide], [
    0x23, 0x5f, 0x60, 0x5c, 0x7b, 0x7d, 0x7e,
  ]);
});

test("ASCII whose codes the English set uses for other signs becomes the nearest it has", () => {
  // At their ASCII codes these would show as arrows, fractions and #.
  assert.equal(String.fromCharCode(...englishCodes("[a]{b}\\|^_`~")), "(a)(b)//  '-");
  for (const code of englishCodes("[]{}\\|^_`~")) {
    assert.ok(![0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e].includes(code), `code ${code}`);
  }
});

test("curly quotes, dashes and accents fold to ASCII", () => {
  assert.equal(String.fromCharCode(...englishCodes("‘it’s’ “so” – café Ångström naïve…")), "'it's' \"so\" - cafe Angstrom naive...");
  assert.equal(String.fromCharCode(...englishCodes("Straße Æsir 3×4 20°")), "Strasse AEsir 3x4 20 deg");
  assert.equal(String.fromCharCode(...englishCodes("é")), "e", "a combining mark is dropped");
  assert.equal(String.fromCharCode(...englishCodes("Москва")), "Moskva", "Cyrillic is romanised");
});

test("what the set cannot show is a question mark, a control a space", () => {
  assert.equal(String.fromCharCode(...englishCodes("a\u{1F600}b中c")), "a?b?c");
  assert.equal(String.fromCharCode(...englishCodes("a\u0007b\u007fc\tz")), "a b c z");
  assert.equal(foldCharacter("\u{1F600}"), undefined);
  assert.equal(foldCharacter("é"), "e");
  assert.equal(foldCharacter("£"), "£");
});

test("every code written is a printable code of the G0 set", () => {
  const everything = Array.from({ length: 0x3000 }, (_, point) => String.fromCodePoint(point)).join("");
  for (const code of englishCodes(everything)) {
    assert.ok(code >= 0x20 && code <= 0x7e, `code ${code}`);
  }
});
