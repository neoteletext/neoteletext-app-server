// crates/teletext-showcase/src/adventure/tests.rs (Station Zero's tests
// and the text's) and the test of station/game.rs, ported: page 401
// driven as the server drives it, the commands typed into it and what they
// change, and a game played to its end.

import assert from "node:assert/strict";
import { test } from "node:test";

import { COLUMNS, CursorFlag, Key, englishCodes } from "../../../kits/typescript/src/kit.ts";
import { Harness, type Rows } from "../../../kits/typescript/src/testing.ts";
import { CROWN, STATION } from "../src/adventure/pages.ts";
import {
  COMMAND_COLUMN,
  COMMAND_ROW,
  DESCRIPTION_ROWS,
  EXITS_ROW,
  ITEMS_ROW,
  NARROW_WIDTH,
  REPLY_ROWS,
  SAID_ROW,
  STATUS_ROW,
  Station,
  TEXT_WIDTH,
} from "../src/adventure/station.ts";
import { Game, MAX_COMMAND, parse } from "../src/adventure/station/game.ts";
import { ALL as STATION_PICTURES, ROWS as STATION_PICTURE_ROWS } from "../src/adventure/station/pictures.ts";
import { DIRECTIONS, Fixture, Item, ROOMS, Room, about, exit, fixtureNoun, itemNoun } from "../src/adventure/station/world.ts";
import { words, wrap } from "../src/adventure/text.ts";
import { Effect, handle } from "../src/sound.ts";
import { all, checkWhole, row } from "./adventure-checks.ts";

/** A viewer's session. */
const VIEWER = 1;
/** Another viewer's. */
const OTHER = 2;

function station(): Harness {
  return new Harness([new Station()]);
}

/** Types `command` and presses Enter; answers page 401 as the viewer has it last. */
function command(harness: Harness, session: number, typed: string): Rows {
  harness.typeText(session, typed);
  harness.key(session, Key.Enter, "press");
  return harness.own(session, STATION);
}

// Opening the game makes it the viewer's own page, with the cursor on the
// command line; typing echoes there and moves the cursor.
test("the game is the viewer's own with a cursor after the typing", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  assert.ok(harness.ownPages(VIEWER, STATION) !== undefined, "the viewer's own page");

  harness.typeText(VIEWER, "loo");
  const page = harness.own(VIEWER, STATION);
  assert.ok(row(page, COMMAND_ROW).includes("> loo"), `the typing is on the command line: ${row(page, COMMAND_ROW)}`);
  const cursor = harness.cursor(VIEWER);
  assert.deepEqual([cursor?.row, cursor?.column], [COMMAND_ROW, COMMAND_COLUMN + 3], "the cursor after the typing");
  assert.equal(cursor?.flags, CursorFlag.Echo);

  harness.key(VIEWER, Key.Backspace, "press");
  const shorter = harness.own(VIEWER, STATION);
  assert.ok(row(shorter, COMMAND_ROW).includes("> lo "), `Backspace takes a character: ${row(shorter, COMMAND_ROW)}`);
  harness.key(VIEWER, Key.Escape, "press");
  const cleared = harness.own(VIEWER, STATION);
  assert.ok(!row(cleared, COMMAND_ROW).includes("lo"), `Escape clears the line: ${row(cleared, COMMAND_ROW)}`);
});

// Typing on the keyboard comes as each key's press with its text
// (INPUT.md section 6.3), and its releases type nothing.
test("the game takes typing from the keys", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  harness.typeKeys(VIEWER, "loo");
  const page = harness.own(VIEWER, STATION);
  assert.ok(row(page, COMMAND_ROW).includes("> loo "), `the keys typed on the command line: ${row(page, COMMAND_ROW)}`);
});

// Commands move the player and change what the page shows; each viewer's
// game is their own.
test("commands move the player and each viewer has their own game", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  const first = harness.own(VIEWER, STATION);
  assert.ok(row(first, 1).includes("Cryo Bay"), all(first));
  assert.ok(row(first, EXITS_ROW).includes("Exits: south, east"), row(first, EXITS_ROW));

  const page = command(harness, VIEWER, "south");
  assert.ok(row(page, 1).includes("Crew Corridor"), all(page));
  assert.ok(row(page, SAID_ROW).includes("> south"), `the command is repeated: ${row(page, SAID_ROW)}`);
  assert.ok(row(page, STATUS_ROW).includes("Moves 1"), row(page, STATUS_ROW));

  harness.join(OTHER, STATION);
  const other = harness.own(OTHER, STATION);
  assert.ok(row(other, 1).includes("Cryo Bay"), `the other viewer starts afresh: ${all(other)}`);

  const refused = command(harness, VIEWER, "w");
  assert.ok(all(refused).includes("You cannot go that way."), all(refused));
  assert.ok(row(refused, 1).includes("Crew Corridor"));
});

// Taking and dropping move items between the room and the inventory.
test("taking and dropping change the room and the inventory", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  let page = command(harness, VIEWER, "e");
  assert.ok(row(page, 1).includes("Medical Bay"));
  assert.ok(row(page, ITEMS_ROW).includes("You see: radiation suit"), row(page, ITEMS_ROW));
  page = command(harness, VIEWER, "take the suit");
  assert.ok(all(page).includes("Taken: radiation suit."), all(page));
  assert.ok(!row(page, ITEMS_ROW).includes("radiation suit"), `no longer here: ${row(page, ITEMS_ROW)}`);
  page = command(harness, VIEWER, "i");
  assert.ok(all(page).includes("You carry: radiation suit."), all(page));
  page = command(harness, VIEWER, "drop suit");
  assert.ok(row(page, ITEMS_ROW).includes("You see: radiation suit"), row(page, ITEMS_ROW));
});

/** The solution, command by command, from waking to the docking bay. */
const SOLUTION = [
  "s",
  "e",
  "open locker",
  "take keycard",
  "w",
  "s",
  "use keycard",
  "e",
  "read datapad",
  "e",
  "take crowbar",
  "w",
  "w",
  "w",
  "use crowbar on machine",
  "take fuse",
  "e",
  "n",
  "n",
  "e",
  "take suit",
  "w",
  "s",
  "s",
  "s",
  "e",
  "use fuse",
  "w",
  "n",
  "up",
  "e",
  "press the beacon",
  "w",
  "use console",
  "d",
  "s",
  "s",
  "s",
] as const;

// The game can be won, by the solution typed into the channel, and the
// puzzles hold until solved.
test("the game can be won", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  // The puzzles refuse before they are solved.
  assert.ok(row(command(harness, VIEWER, "s"), 1).includes("Crew Corridor"));
  assert.ok(row(command(harness, VIEWER, "s"), 1).includes("Central Hub"));
  assert.ok(all(command(harness, VIEWER, "e")).includes("sealed"), "the laboratory is locked");
  assert.ok(all(command(harness, VIEWER, "u")).includes("no power"), "the lift is dead");
  assert.ok(row(command(harness, VIEWER, "restart"), 1).includes("Cryo Bay"));

  let page: Rows = [];
  for (const typed of SOLUTION) {
    page = command(harness, VIEWER, typed);
  }
  assert.ok(row(page, 1).includes("Docking Bay"), all(page));
  page = command(harness, VIEWER, "board shuttle");
  assert.ok(row(page, 1).includes("Homeward"), all(page));
  const moves = SOLUTION.length + 1;
  assert.ok(all(page).includes(`Mission complete in ${moves} moves`), all(page));
  assert.ok(all(page).includes("rescue"), `the beacon was sent: ${all(page)}`);
  page = command(harness, VIEWER, "n");
  assert.ok(all(page).includes("RESTART"), `the game is over: ${all(page)}`);
});

// A session's game is forgotten when it ends.
test("an ended session is forgotten", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  assert.ok(row(command(harness, VIEWER, "s"), 1).includes("Crew Corridor"));
  harness.close(VIEWER);
  harness.join(VIEWER, STATION);
  const page = harness.own(VIEWER, STATION);
  assert.ok(row(page, 1).includes("Cryo Bay"), `a new game: ${all(page)}`);
});

/** Whether all of `marked` fits `rows` rows, the first `narrow` of them beside a picture. */
function fits(marked: string, narrow: number, rows: number): boolean {
  const width = (line: number): number => (line < narrow ? NARROW_WIDTH : TEXT_WIDTH);
  const placed = wrap(marked, width, rows).reduce((sum, line) => sum + line.length, 0);
  return placed === words(marked).length;
}

// Every description, reply and examination fits its place on the page
// along the whole solution, and every thing can be examined and used.
test("the game's text fits the page", () => {
  const check = (game: Game): void => {
    assert.ok(fits(game.description(), STATION_PICTURE_ROWS, DESCRIPTION_ROWS), game.description());
    assert.ok(fits(game.reply(), 0, REPLY_ROWS), game.reply());
    assert.ok(fits(game.ending(), STATION_PICTURE_ROWS, DESCRIPTION_ROWS), game.ending());
  };
  const game = new Game();
  check(game);
  const things = ["keycard", "crowbar", "fuse", "suit", "datapad", "mug", "pod", "locker", "machine", "door", "panel", "console", "radio", "shuttle"];
  for (const typed of SOLUTION) {
    game.command(typed);
    check(game);
    for (const thing of things) {
      for (const verb of ["examine", "use", "open", "take", "drop"]) {
        const copy = game.clone();
        copy.command(`${verb} ${thing}`);
        check(copy);
      }
    }
    for (const other of ["help", "hint", "inventory", "look", "take all", "xyzzy", "dance", "go", "wait"]) {
      const copy = game.clone();
      copy.command(other);
      check(copy);
    }
  }
  for (const room of ROOMS) {
    assert.ok([...about(room).name].length <= 21, `${room}'s name fits the band`);
  }
});

// Every room is reachable from the cryo bay, and every way back exists.
test("the station map joins up", () => {
  const seen: Room[] = [];
  const queue: Room[] = [Room.CryoBay];
  for (let room = queue.pop(); room !== undefined; room = queue.pop()) {
    if (seen.includes(room)) {
      continue;
    }
    seen.push(room);
    for (const direction of DIRECTIONS) {
      const to = exit(room, direction);
      if (to !== undefined) {
        const from = room;
        assert.ok(
          DIRECTIONS.some((back) => exit(to, back) === from),
          `${to} leads back to ${from}`,
        );
        queue.push(to);
      }
    }
  }
  assert.equal(seen.length, ROOMS.length);
});

// Every picture is a whole grid of known sixels, its backgrounds one a cell
// row, at the size its page leaves it.
test("every picture of the station is whole", () => {
  for (const picture of STATION_PICTURES) {
    checkWhole(picture, [28, 18]);
  }
});

// Marked-up text wraps at word boundaries, highlights the braced words,
// and cuts a word longer than its line.
test("text wraps and highlights", () => {
  const lines = wrap("one {two three} four", () => 10, 5);
  const shown = lines.map((line) => line.map((word) => [word.text, word.highlight]));
  assert.deepEqual(shown, [
    [
      ["one", false],
      ["two", true],
    ],
    [
      ["three", true],
      ["four", false],
    ],
  ]);
  const cut = wrap("abcdefghij", () => 4, 5).map((line) => line.map((word) => word.text).join(""));
  assert.deepEqual(cut, ["abcd", "efgh", "ij"]);
  assert.equal(wrap("a b c", () => 1, 2).length, 2, "no more lines than asked");
});

test("commands parse with their synonyms", () => {
  assert.deepEqual(parse("  "), { kind: "empty" });
  assert.deepEqual(parse("n"), { kind: "go", direction: "north" });
  assert.deepEqual(parse("Go UP."), { kind: "go", direction: "up" });
  assert.deepEqual(parse("pick up the keycard"), { kind: "take", noun: itemNoun(Item.Keycard), named: true });
  assert.deepEqual(parse("take all"), { kind: "takeAll" });
  assert.deepEqual(parse("use crowbar on machine"), { kind: "use", nouns: [itemNoun(Item.Crowbar), fixtureNoun(Fixture.Machine)], named: true });
  assert.deepEqual(parse("look at the pod"), { kind: "examine", noun: fixtureNoun(Fixture.Pod), named: true });
  assert.deepEqual(parse("take"), { kind: "take", noun: undefined, named: false });
  assert.deepEqual(parse("take banana"), { kind: "take", noun: undefined, named: true });
  assert.deepEqual(parse("dance"), { kind: "unknown" });
});

// Not in the Rust tests: each command's sound, sent to the viewer alone.
test("a command plays its sound", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  command(harness, VIEWER, "s");
  command(harness, VIEWER, "w");
  command(harness, VIEWER, "look");
  const plays = harness.sounds(VIEWER).flatMap((message) => ("play" in message ? [(message["play"] as Record<string, unknown>)["handle"]] : []));
  assert.deepEqual(plays, [handle(Effect.Click), handle(Effect.Hit)], "a move clicks, a refusal hits, a look is quiet");
  assert.deepEqual(harness.sounds(OTHER), []);
});

// A line break in a paste ends the command, as Enter does; what follows it
// is dropped (crates/teletext-showcase/src/adventure/station.rs, type_into).
test("a line break in a paste ends the command", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  harness.paste(VIEWER, "e\nignored");
  const page = harness.own(VIEWER, STATION);
  assert.ok(row(page, 1).includes("Medical Bay"), all(page));
  assert.ok(row(page, SAID_ROW).includes("> e "), row(page, SAID_ROW));
  assert.ok(!all(page).includes("ignored"), "nothing after the break is kept");
  assert.equal(harness.cursor(VIEWER)?.column, COMMAND_COLUMN, "the command line is empty again");
});

// The command line keeps MAX_COMMAND characters, and the cursor stays after
// them on the row.
test("the command line holds the longest command", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  harness.typeText(VIEWER, "x".repeat(MAX_COMMAND + 10));
  assert.ok(row(harness.own(VIEWER, STATION), COMMAND_ROW).includes(`> ${"x".repeat(MAX_COMMAND)} `));
  const cursor = harness.cursor(VIEWER);
  assert.deepEqual([cursor?.column, cursor?.width], [COMMAND_COLUMN + MAX_COMMAND, COLUMNS - COMMAND_COLUMN - MAX_COMMAND]);
});

// Not in the Rust showcase, which wrote a character outside ASCII as a
// space: the row writes typing in the English set, where a letter may take
// more cells than one (a romanised Cyrillic letter) or none (a combining
// mark), so the cursor goes after the codes the row shows, never past the
// last column.
test("the cursor follows the typing as the row shows it", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  const typed = "\u0449a\u0301";
  harness.typeText(VIEWER, typed);
  assert.equal(harness.cursor(VIEWER)?.column, COMMAND_COLUMN + englishCodes(typed).length);
  harness.typeText(VIEWER, "\u0449".repeat(MAX_COMMAND));
  const cursor = harness.cursor(VIEWER);
  assert.deepEqual([cursor?.column, cursor?.width], [COLUMNS - 1, 1], "held at the last column");
});

// Not in the Rust showcase: the game types digits, and its coloured keys
// follow its links (INPUT.md section 9.4).
test("the game types digits and its coloured keys follow its links", () => {
  const harness = station();
  harness.join(VIEWER, STATION);
  harness.typeText(VIEWER, "402");
  assert.ok(row(harness.own(VIEWER, STATION), COMMAND_ROW).includes("> 402"), "the game types them");
  assert.deepEqual(harness.gotos(VIEWER), []);
  harness.press(VIEWER, Key.Green);
  assert.deepEqual(harness.gotos(VIEWER), [CROWN], "its green link, Crown");
  assert.deepEqual(harness.errors, []);
});
