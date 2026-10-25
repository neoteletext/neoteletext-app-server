import assert from "node:assert/strict";
import { test } from "node:test";

import { Harness, text } from "../../../kits/typescript/src/testing.ts";
import type { Area } from "../src/area.ts";
import { AREAS, applications, sections } from "../src/areas.ts";
import { INDEX } from "../src/common.ts";

const AREA: Area = {
  applications: () => [],
  sections: () => [
    { page: 0x101, name: "One", about: "the first" },
    { page: 0x200, name: "Two", about: "the second" },
    { page: 0x300, name: "Three", about: "the third" },
    { page: 0x400, name: "Four", about: "the fourth" },
    { page: 0x500, name: "Five", about: "the fifth" },
  ],
};

test("the index lists every area's sections and links the first four", () => {
  const harness = new Harness(applications([AREA]));
  const rows = harness.shared(INDEX);
  assert.ok(text(rows, 1).includes("NeoTeletext showcase"));
  assert.equal(text(rows, 4).trimEnd(), " \u0003101\u0007One");
  assert.equal(text(rows, 5).trimEnd(), "     \u0006the first");
  assert.ok(text(rows, 12).includes("500") && text(rows, 12).includes("Five"));
  assert.deepEqual(harness.sharedPages(INDEX)[0]?.links, [0x101, 0x200, 0x300, 0x400, null, INDEX]);
});

test("every area registered has sections, and every page is declared once", () => {
  const harness = new Harness(applications());
  const declared = (harness.sent[0]?.["pages"] as { page: string }[]).map((entry) => entry.page);
  assert.equal(new Set(declared).size, declared.length);
  assert.equal(declared[0], "100");
  for (const area of AREAS) {
    assert.ok(area.sections().length > 0);
  }
  assert.equal(sections().length, AREAS.flatMap((area) => area.sections()).length);
  assert.deepEqual(harness.errors, []);
});
