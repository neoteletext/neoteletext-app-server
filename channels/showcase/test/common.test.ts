import assert from "node:assert/strict";
import { test } from "node:test";

import { Colour, textColour } from "../../../kits/typescript/src/kit.ts";
import { FASTEXT_ROW, INDEX, labelAt, labelColumns, page } from "../src/common.ts";

test("a page has the service's header, a double-height title band and its Fastext", () => {
  const screen = page("Weather", Colour.Blue, [
    ["News", 0x101],
    ["Clock", 0x105],
    ["", null],
    ["Index", INDEX],
  ]);
  assert.equal(screen.rowText(0).slice(8, 19), "NeoTeletext");
  assert.deepEqual(screen.row(1)?.slice(0, 4), [textColour(Colour.Blue), 0x1d, 0x0d, textColour(Colour.White)]);
  assert.equal(screen.rowText(1).slice(4, 11), "Weather");
  // Each label after its colour, one cell apart; the empty yellow one takes no room.
  assert.equal(screen.rowText(FASTEXT_ROW).trimEnd(), "\u0001News \u0002Clock \u0006Index");
  assert.deepEqual(screen.fastextLinks(), [0x101, 0x105, null, INDEX]);
  assert.equal(screen.index(), INDEX);
});

test("labels follow one another, each after its colour, so none overwrites the last", () => {
  assert.deepEqual(labelColumns(["News", "Weather", "", "Index"]), [[0, 5], [6, 14], null, [15, 21]]);
  assert.equal(labelAt(["News", "Weather", "", "Index"], 0), 0, "the colour cell is the label's");
  assert.equal(labelAt(["News", "Weather", "", "Index"], 4), 0);
  assert.equal(labelAt(["News", "Weather", "", "Index"], 5), undefined, "the gap between labels");
  assert.equal(labelAt(["News", "Weather", "", "Index"], 20), 3);
  assert.equal(labelAt(["News", "Weather", "", "Index"], 21), undefined);
});
