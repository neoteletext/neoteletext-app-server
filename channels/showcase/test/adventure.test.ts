// crates/teletext-showcase/src/adventure/tests.rs (the area's pages at the
// start), ported, and the menu's page selection: the adventures as one
// area, driven as the server drives it.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Key, pageText } from "../../../kits/typescript/src/kit.ts";
import { Harness } from "../../../kits/typescript/src/testing.ts";
import { adventure } from "../src/adventure.ts";
import { CROWN, MENU, STATION } from "../src/adventure/pages.ts";
import { all } from "./adventure-checks.ts";

/** A viewer's session. */
const VIEWER = 1;

function area(): Harness {
  return new Harness(adventure.applications([]));
}

// Every page is published once, shared, at the start: the menu, and each
// game's one page as it is before the viewer plays (decision 7).
test("every page is published once and shared", () => {
  const harness = area();
  const counts = new Map<string, number>();
  const [declaration, ...started] = harness.sent;
  assert.equal(declaration?.["type"], "pages");
  for (const message of started) {
    assert.equal(message["type"], "publish");
    assert.equal(message["owner"], "channel", "nothing for any viewer at the start");
    const page = String(message["page"]);
    counts.set(page, (counts.get(page) ?? 0) + 1);
  }
  assert.deepEqual([...counts.keys()].sort(), [MENU, STATION, CROWN].map(pageText));
  assert.ok([...counts.values()].every((count) => count === 1), "each page once");
});

test("the menu shows both games and the note", () => {
  const menu = all(area().shared(MENU));
  for (const shown of ["401", "Station Zero", "fourteen rooms, one way home", "402", "The Lost Crown", "eight endings, coloured keys", "Station Zero takes the keyboard"]) {
    assert.ok(menu.includes(shown), shown);
  }
});

// Not in the Rust showcase: the menu leaves digits and the coloured keys to
// page selection (INPUT.md section 9.4).
test("the menu selects pages by digits and its links", () => {
  const harness = area();
  harness.join(VIEWER, MENU);
  harness.typeText(VIEWER, "402");
  assert.deepEqual(harness.gotos(VIEWER), [CROWN], "the menu takes no digits");
  harness.press(VIEWER, Key.F1);
  assert.deepEqual(harness.gotos(VIEWER), [CROWN, STATION], "its red link, Station");
  assert.deepEqual(harness.errors, []);
});
