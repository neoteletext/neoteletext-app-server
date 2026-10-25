// What the adventures' tests share: a page's rows as a receiver shows them,
// controls as spaces, as the Rust tests read them, and the check that a
// picture is whole (crates/teletext-showcase/src/adventure/tests.rs).

import assert from "node:assert/strict";

import type { Rows } from "../../../kits/typescript/src/testing.ts";
import { type Art, known } from "../src/adventure/art.ts";

/** The last code of the controls, which a receiver shows as spaces (EN 300 706 Table 26). */
const LAST_CONTROL = 0x1f;
/** What a control shows as. */
const SPACE = " ";

/** The text of a row as a receiver shows it: controls as spaces. */
export function row(rows: Rows, at: number): string {
  return (rows[at] ?? []).map((code) => (code <= LAST_CONTROL ? SPACE : String.fromCharCode(code))).join("");
}

/** Every row's text, one a line. */
export function all(rows: Rows): string {
  return rows.map((_, at) => row(rows, at)).join("\n");
}

/** Checks `picture` is a whole grid of known sixels of `size`, its backgrounds one a cell row. */
export function checkWhole(picture: Art, size: readonly [number, number]): void {
  assert.deepEqual(picture.size(), size);
  assert.ok(
    picture.sixels.every((line) => [...line].length === size[0]),
    "every row as wide",
  );
  assert.ok(
    picture.sixels.every((line) => [...line].every(known)),
    "known letters only",
  );
  assert.equal([...picture.backgrounds].length, picture.cells()[1]);
  assert.ok([...picture.backgrounds].every(known));
}
