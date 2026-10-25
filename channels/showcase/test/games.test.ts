// crates/teletext-showcase/src/games/tests.rs, ported: each game, the
// sprites, the hall and the canvas on their own.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Colour, Screen } from "../../../kits/typescript/src/kit.ts";
import { Breakout } from "../src/games/breakout.ts";
import { Canvas } from "../src/games/canvas.ts";
import { Cave } from "../src/games/cave.ts";
import { Hall } from "../src/games/hall.ts";
import { ENEMY_HEIGHT, ENEMY_WIDTH, Invaders, enemy } from "../src/games/invaders.ts";
import { playPage } from "../src/games/look.ts";
import { Control, type Dice, Game, Keys } from "../src/games/pages.ts";
import { Snake } from "../src/games/snake.ts";
import { enemies, extract } from "../src/games/sprites.ts";
import type { Picture } from "../src/pictures.ts";
import { Effect } from "../src/sound.ts";

/** Dice that always throw the same: `value`, or the highest number below the bound. */
class Loaded implements Dice {
  readonly #value: number;

  constructor(value: number) {
    this.#value = value;
  }

  below(bound: number): number {
    return Math.min(this.#value, Math.max(0, bound - 1));
  }
}

/** The largest number the Rust's u32 dice could be loaded with. */
const U32_MAX = 0xffff_ffff;

// A shot kills an enemy: points, a laser and an explosion; the ship moves.
test("invaders shoot and score", () => {
  const drawn = enemies([], ENEMY_WIDTH, ENEMY_HEIGHT);
  const game = new Invaders();
  const sounds: Effect[] = [];
  // No bombs: the dice never fall under the bomb chance.
  const dice = new Loaded(U32_MAX);
  const fire = new Keys();
  fire.press(Control.Fire);
  for (let tick = 0; tick < 200 && game.score() === 0; tick += 1) {
    game.tick(fire, dice, sounds);
  }
  assert.ok(game.score() > 0, "a shot from the middle meets the wave");
  assert.ok(sounds.includes(Effect.Laser) && sounds.includes(Effect.Explosion));
  assert.equal(game.lives(), 3);
  const screen = playPage(Game.Invaders);
  game.draw(screen, drawn);
  assert.ok(screen.rowText(1).includes("SCORE 0000"));

  const start = game.playerX();
  const left = new Keys();
  left.press(Control.Left);
  game.tick(left, dice, sounds);
  assert.ok(game.playerX() < start);
});

// A wave shot down brings the next, of the next creatures, with a fanfare
// and a banner naming them.
test("invaders come in waves", () => {
  const drawn = enemies([], ENEMY_WIDTH, ENEMY_HEIGHT);
  const game = new Invaders();
  const sounds: Effect[] = [];
  const dice = new Loaded(U32_MAX);
  // Past the first wave's banner.
  for (let tick = 0; tick < 20; tick += 1) {
    game.tick(new Keys(), dice, sounds);
  }
  game.clearWave();
  game.tick(new Keys(), dice, sounds);
  assert.equal(game.wave(), 1);
  assert.ok(sounds.includes(Effect.Win));
  const screen = playPage(Game.Invaders);
  game.draw(screen, drawn);
  const second = enemy(drawn, 1, 0);
  assert.ok(second !== undefined);
  assert.notDeepEqual(second, enemy(drawn, 0, 0), "new creatures");
  const rowTexts = Array.from({ length: 25 }, (_, row) => screen.rowText(row));
  assert.ok(rowTexts.some((row) => row.includes("WAVE 2")));
  assert.ok(rowTexts.some((row) => row.includes(second.name)));
});

// Bombs cost lives, and the last one ends the game.
test("invaders' bombs end the game", () => {
  const game = new Invaders();
  const sounds: Effect[] = [];
  // Bombs at every chance, from the middle column, over the ship.
  const dice = new Loaded(2);
  for (let tick = 0; tick < 2_000 && !game.over(); tick += 1) {
    game.tick(new Keys(), dice, sounds);
  }
  assert.ok(game.over());
  assert.equal(game.lives(), 0);
  assert.ok(sounds.includes(Effect.Lose));
});

// The snake turns, never straight back, and dies at the wall.
test("the snake turns and dies at the wall", () => {
  const dice = new Loaded(0);
  const snake = new Snake(dice);
  const sounds: Effect[] = [];
  const [x, y] = snake.head();
  snake.turn(Control.Left);
  snake.turn(Control.Up);
  for (let tick = 0; tick < 2; tick += 1) {
    snake.tick(dice, sounds);
  }
  assert.deepEqual(snake.head(), [x, y - 1], "left was refused, up taken");
  for (let tick = 0; tick < 200; tick += 1) {
    snake.tick(dice, sounds);
  }
  assert.ok(snake.over());
  assert.ok(sounds.includes(Effect.Lose));
});

// The snake grows when it eats.
test("the snake grows", () => {
  const dice = new Loaded(0);
  const snake = new Snake(dice);
  const sounds: Effect[] = [];
  // The food is on the first free square, the grid's top-left, and the
  // snake steps every second tick: up to the top row, then left to it.
  const [x, y] = snake.head();
  snake.turn(Control.Up);
  for (let tick = 0; tick < y * 2; tick += 1) {
    snake.tick(dice, sounds);
  }
  snake.turn(Control.Left);
  for (let tick = 0; tick < x * 2; tick += 1) {
    snake.tick(dice, sounds);
  }
  assert.deepEqual(snake.head(), [0, 0]);
  assert.ok(snake.score() > 0 && sounds.includes(Effect.Coin));
  snake.turn(Control.Down);
  for (let tick = 0; tick < 6; tick += 1) {
    snake.tick(dice, sounds);
  }
  assert.ok(snake.length() > 4, `longer: ${snake.length()}`);
});

// Breakout: the ball launches, the wall loses bricks, the paddle moves.
test("breakout launches and breaks bricks", () => {
  const game = new Breakout();
  const sounds: Effect[] = [];
  const fire = new Keys();
  fire.press(Control.Fire);
  game.tick(fire, sounds);
  assert.ok(game.inFlight());
  const full = game.bricksLeft();
  for (let tick = 0; tick < 100; tick += 1) {
    game.tick(new Keys(), sounds);
  }
  assert.ok(game.bricksLeft() < full);
  assert.ok(game.score() > 0);
  const right = new Keys();
  right.press(Control.Right);
  const before = game.paddleX();
  game.tick(right, sounds);
  assert.ok(game.paddleX() > before);
});

// Breakout: a ball missed costs a life, and the last ends the game.
test("breakout ends when the balls are lost", () => {
  const game = new Breakout();
  const sounds: Effect[] = [];
  // The paddle at the far left, the ball launched rightwards and up, and
  // never met again.
  const left = new Keys();
  left.press(Control.Left);
  for (let tick = 0; tick < 3_000 && !game.over(); tick += 1) {
    game.tick(left, sounds);
  }
  assert.ok(game.over());
  assert.ok(sounds.includes(Effect.Lose));
});

// The cave flyer climbs, and a ship left alone meets the rock.
test("the cave flyer climbs and crashes", () => {
  // The cave keeps to the top, and narrows away from the ship.
  const dice = new Loaded(0);
  const cave = new Cave(dice);
  const sounds: Effect[] = [];
  const start = cave.shipY();
  const up = new Keys();
  up.press(Control.Up);
  cave.tick(up, dice, sounds);
  assert.ok(cave.shipY() < start);
  for (let tick = 0; tick < 5_000 && !cave.over(); tick += 1) {
    cave.tick(new Keys(), dice, sounds);
  }
  assert.ok(cave.over() && cave.score() > 0);
});

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
