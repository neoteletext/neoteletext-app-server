// crates/teletext-showcase/src/games/tests.rs, ported: the sprites, the
// hall and the canvas on their own.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Colour, Screen } from "../../../kits/typescript/src/kit.ts";
import { Canvas } from "../src/games/canvas.ts";
import { Hall } from "../src/games/hall.ts";
import { Game } from "../src/games/pages.ts";
import { enemies, extract } from "../src/games/sprites.ts";
import type { Picture } from "../src/pictures.ts";


/** ESC, which starts an escaped control code in an OL line. */
const ESCAPE = "\u001b";
/** What a control code is written as after ESC: the code plus this. */
const ESCAPE_OFFSET = 0x40;
/** The last control code (EN 300 706 Table 26). */
const LAST_CONTROL = 0x1f;

/** A TTI page of `rows`, each (row, codes), codes under 0x20 in the TTI escape form. */
function tti(rows: readonly (readonly [number, readonly number[]])[]): string {
  const lines = ["PN,10000", "SC,0000", "PS,8000"];
  for (const [row, codes] of rows) {
    const line = codes.map((code) => (code <= LAST_CONTROL ? ESCAPE + String.fromCharCode(code + ESCAPE_OFFSET) : String.fromCharCode(code))).join("");
    lines.push(`OL,${row},${line}`);
  }
  return lines.map((line) => `${line}\n`).join("");
}

// A sprite is read back from its picture: the mosaics in their colour,
// cut to the lit sixels, its label under it left out, and scaled.
test("a sprite is extracted from a picture", () => {
  // A red block four cells wide and two rows tall, and a white label two
  // rows of cells under it.
  const block = [0x11, 0x7f, 0x7f, 0x7f, 0x7f];
  const label = [0x17, 0x23, 0x2c, 0x23, 0x2c, 0x23, 0x2c];
  const picture = tti([
    [5, block],
    [6, block],
    [9, label],
  ]);
  const sprite = extract(picture, 8, 6);
  assert.ok(sprite !== undefined);
  assert.deepEqual([sprite.width, sprite.height, sprite.colour], [8, 6, Colour.Red]);
  assert.equal(sprite.count(), 48, "every sixel of the block, none of the label");

  const small = extract(picture, 4, 4);
  assert.deepEqual([small?.width, small?.height, small?.count()], [4, 3, 12], "scaled, keeping its shape");

  // Hold Mosaics repeats the last mosaic in an attribute's cell.
  const held = [0x1e, 0x12, 0x7f, 0x13, 0x7f];
  const holding = extract(
    tti([
      [5, held],
      [6, held],
    ]),
    16,
    12,
  );
  assert.deepEqual([holding?.width, holding?.count()], [6, 36], "the held block fills the attribute's cell");
  // In alphanumerics, the same codes are letters, not sixels.
  const letters = [0x02, 0x7f, 0x7f];
  assert.equal(
    extract(
      tti([
        [5, letters],
        [6, letters],
      ]),
      8,
      6,
    ),
    undefined,
  );

  assert.equal(extract("not a page", 8, 6), undefined);
  assert.equal(extract(tti([[5, [0x11, 0x20, 0x20]]]), 8, 6), undefined, "nothing lit");
});

// Pictures of the sci-fi group become the enemies, named from their
// captions, in name order; without them there are drawn ones.
test("the enemies come from the sprite pictures", () => {
  const block = [0x16, 0x7f, 0x7f, 0x7f];
  const picture = (name: string, caption: string, group: string): Picture => ({
    name,
    caption,
    group,
    levels: [
      {
        level: "l15",
        tti: tti([
          [5, block],
          [6, block],
        ]),
      },
    ],
  });
  const pictures = [picture("s02", "02_void_drifter (sheet)", "s"), picture("s01", "01_razor_crawler (sheet)", "s"), picture("a01", "Some art", "a")];
  const sprites = enemies(pictures, 10, 6);
  assert.deepEqual(
    sprites.map((sprite) => sprite.name),
    ["Razor crawler", "Void drifter"],
  );
  assert.equal(sprites[0]?.colour, Colour.Cyan);
  assert.notEqual(enemies([], 10, 6).length, 0, "drawn ones");
});

// The hall keeps the best, best first; a tie goes under the score it
// ties.
test("the hall keeps the best", () => {
  const hall = new Hall();
  assert.ok(!hall.record(Game.Snake, 1, 0), "nothing for nothing");
  assert.ok(hall.record(Game.Snake, 1, 50));
  assert.ok(hall.record(Game.Invaders, 2, 70));
  assert.ok(hall.record(Game.Cave, 3, 50));
  assert.deepEqual(
    hall.entries().map((entry) => entry.player),
    [2, 1, 3],
  );
  for (let score = 100; score < 112; score += 1) {
    hall.record(Game.Breakout, 4, score);
  }
  assert.ok(!hall.record(Game.Snake, 5, 1), "the table is full of better");
  assert.equal(hall.best(Game.Breakout)?.[0], 111);
  assert.equal(hall.best(Game.Snake), undefined, "pushed off the table");
  assert.ok(hall.screen().rowText(5).includes("Player 0004"));
});

// The canvas puts a colour change in the empty cell before a sprite, and
// none where the colour stays.
test("the canvas places colours", () => {
  const canvas = new Canvas(20, 3);
  canvas.rectangle(2, 0, 4, 3, Colour.Red);
  canvas.rectangle(10, 0, 2, 3, Colour.Red);
  canvas.rectangle(14, 0, 2, 3, Colour.Green);
  const screen = new Screen();
  canvas.draw(screen, 5, 1);
  // Cells 1 to 10 hold sixel columns 0 to 19: the red block in cells 2 and
  // 3, its colour in cell 1; the second red block in cell 6 with no new
  // colour; green in cell 8, its colour in cell 7.
  assert.deepEqual(screen.row(5)?.slice(0, 12), [0x20, 0x11, 0x7f, 0x7f, 0x20, 0x20, 0x7f, 0x12, 0x7f, 0x20, 0x20, 0x20]);
});
