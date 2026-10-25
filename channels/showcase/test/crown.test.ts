// crates/teletext-showcase/src/adventure/tests.rs (The Lost Crown's tests),
// ported: the story's joins, and the story played on its one page 402 as
// the server drives it.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Button, Key, pageText } from "../../../kits/typescript/src/kit.ts";
import { Harness, type Rows, readTti } from "../../../kits/typescript/src/testing.ts";
import { CHOICE_ROW, Crown, ENDING_CHOICE_ROW, TEXT_ROWS, TEXT_WIDTH, node } from "../src/adventure/crown.ts";
import { ALL as CROWN_PICTURES } from "../src/adventure/crown/pictures.ts";
import { CROSSROADS, Ending, type Id, NODES, type Node } from "../src/adventure/crown/story.ts";
import { CROWN, MENU, STATION } from "../src/adventure/pages.ts";
import { Station } from "../src/adventure/station.ts";
import { words, wrap } from "../src/adventure/text.ts";
import { FASTEXT_ROW, labelColumns } from "../src/common.ts";
import { all, checkWhole, row } from "./adventure-checks.ts";

/** A viewer's session. */
const VIEWER = 1;
/** Another viewer's. */
const OTHER = 2;
/** The page 402 is published as, in the protocol's hexadecimal. */
const CROWN_TEXT = pageText(CROWN);

function crown(): Harness {
  return new Harness([new Crown()]);
}

// Every choice of the story leads to a node or out to the menu, every node
// can be reached from the first, every ending leads back to the start, and
// every node's id is its place.
test("the story joins up", () => {
  assert.ok(NODES.length >= 20 && NODES.length <= 30, `${NODES.length} nodes`);
  NODES.forEach((each, place) => assert.equal(each.id, place, `${each.title}'s id is its place`));
  assert.equal(NODES[0]?.id, CROSSROADS, "the story starts at its first node");
  const reached = new Set<Id>();
  const queue: Id[] = [CROSSROADS];
  for (let id = queue.pop(); id !== undefined; id = queue.pop()) {
    if (reached.has(id)) {
      continue;
    }
    reached.add(id);
    const at = node(id);
    assert.ok(at !== undefined);
    assert.ok(at.choices.length > 0 && at.choices.length <= 4, at.title);
    for (const choice of at.choices) {
      if ("node" in choice.to) {
        assert.ok(node(choice.to.node) !== undefined, `${at.title} leads to ${choice.to.node}`);
        queue.push(choice.to.node);
      } else {
        assert.equal(choice.to.page, MENU, `${at.title} leaves only for the menu`);
      }
      assert.ok([...choice.label].length <= 9, `${choice.label} fits its Fastext slot`);
      assert.ok([...choice.text].length <= 36, `${choice.text} fits its row`);
    }
    if (at.ending !== Ending.Not) {
      assert.ok(
        at.choices.some((choice) => "node" in choice.to && choice.to.node === CROSSROADS),
        `${at.title} leads back to the start`,
      );
    }
  }
  assert.deepEqual(
    [...reached].sort((a, b) => a - b),
    NODES.map((each) => each.id),
    "every node is reached",
  );
  const endings = (ending: Ending): number => NODES.filter((each) => each.ending === ending).length;
  assert.ok(endings(Ending.Won) >= 2 && endings(Ending.Lost) >= 3, "several endings");
});

/** The node the choice `choice` of `id` leads to. */
function after(id: Id, choice: number): Node {
  const to = NODES[id]?.choices[choice]?.to;
  assert.ok(to !== undefined && "node" in to, `choice ${choice} of ${id} leads to a node`);
  const next = node(to.node);
  assert.ok(next !== undefined);
  return next;
}

/** What `act` sent of page 402: every publication its, and the last one's rows; undefined if it published nothing. */
function crownPage(harness: Harness, act: () => void): Rows | undefined {
  const from = harness.sent.length;
  act();
  const published = harness.sent.slice(from).filter((message) => message["type"] === "publish");
  assert.ok(
    published.every((message) => message["page"] === CROWN_TEXT),
    "the story never leaves its page",
  );
  const last = published.at(-1);
  return last === undefined ? undefined : readTti(String(last["tti"])).at(-1)?.rows;
}

/** As {@link crownPage}, for an act that publishes. */
function crownPageShown(harness: Harness, act: () => void): string {
  const rows = crownPage(harness, act);
  assert.ok(rows !== undefined, "the page was published");
  return all(rows);
}

/** A press of the left button on a cell, without its release. */
function click(harness: Harness, column: number, at: number): void {
  harness.mouse(VIEWER, "press", Button.Left, column, at);
}

// The whole story is one page: before choosing a viewer sees the first
// node, shared, with the choices' prompts but no links; each way of
// choosing (function key, remote colour key, letter, a click on a choice
// or its prompt) draws the next node on the same page, for that viewer
// alone.
test("the story is played on its one page", () => {
  const harness = crown();
  harness.join(VIEWER, CROWN);
  const first = harness.seen(VIEWER, CROWN);
  const start = NODES[0];
  assert.ok(start !== undefined);
  assert.ok(all(first.rows).includes(start.title));
  assert.deepEqual(first.links.slice(0, 4), [null, null, null, null], "the choices are the page's, not links");
  assert.ok(row(first.rows, FASTEXT_ROW).includes(start.choices[0]?.label ?? "?"), "row 24 prompts the choices");

  // Green: the second choice.
  let at = after(CROSSROADS, 1);
  assert.ok(crownPageShown(harness, () => harness.key(VIEWER, Key.F2, "press")).includes(at.title), `F2 chose ${at.title}`);

  // The other viewer is still at the crossroads, on their own page.
  harness.join(OTHER, CROWN);
  assert.ok(all(harness.seen(OTHER, CROWN).rows).includes(start.title));

  // A remote's red key, then the letter of red or a click.
  let next = after(at.id, 0);
  assert.ok(crownPageShown(harness, () => harness.key(VIEWER, Key.Red, "press")).includes(next.title), `the remote's red chose ${next.title}`);
  at = next;
  next = after(at.id, 0);
  assert.ok(crownPageShown(harness, () => harness.typeText(VIEWER, "r")).includes(next.title), `r chose ${next.title}`);
  at = next;
  const choiceRow = at.ending === Ending.Not ? CHOICE_ROW : ENDING_CHOICE_ROW;
  next = after(at.id, 0);
  assert.ok(crownPageShown(harness, () => click(harness, 10, choiceRow)).includes(next.title), `a click on the first choice chose ${next.title}`);
  at = next;
  const span = labelColumns([at.choices[0]?.label ?? "", "", "", ""])[0];
  assert.ok(span !== null && span !== undefined && span[1] > span[0]);
  next = after(at.id, 0);
  assert.ok(
    crownPageShown(harness, () => click(harness, span[1] - 1, FASTEXT_ROW)).includes(next.title),
    `a click on the red prompt chose ${next.title}`,
  );
});

/** The function keys of the choices, in order. */
const FUNCTION_KEYS = [Key.F1, Key.F2, Key.F3, Key.F4] as const;

// At an ending, "Menu" sends the viewer to the menu and begins the story
// again for their next visit; "Again" begins it on the page.
test("an ending leads back to the start or the menu", () => {
  const harness = crown();
  harness.join(VIEWER, CROWN);
  // Always the first choice, to an ending.
  let at = NODES[0];
  assert.ok(at !== undefined);
  let steps = 0;
  while (at.ending === Ending.Not) {
    const index = at.choices.findIndex((choice) => "node" in choice.to);
    at = after(at.id, index);
    const title = at.title;
    assert.ok(crownPageShown(harness, () => harness.key(VIEWER, FUNCTION_KEYS[index] ?? Key.F1, "press")).includes(title));
    steps += 1;
    assert.ok(steps < NODES.length, "the first choices reach an ending");
  }
  assert.ok(at.choices.length < 4);
  assert.equal(
    crownPage(harness, () => harness.key(VIEWER, Key.F4, "press")),
    undefined,
    "a key with no choice behind it changes nothing",
  );
  const again = at.choices.findIndex((choice) => "node" in choice.to && choice.to.node === CROSSROADS);
  const menu = at.choices.findIndex((choice) => "page" in choice.to && choice.to.page === MENU);
  assert.ok(again !== -1 && menu !== -1);
  const shown = crownPageShown(harness, () => harness.key(VIEWER, FUNCTION_KEYS[menu] ?? Key.F1, "press"));
  assert.deepEqual(harness.gotos(VIEWER), [MENU], "sent to the menu");
  assert.ok(shown.includes(NODES[0]?.title ?? "?"), "the story begins again");
  assert.notEqual(again, menu);
});

// Every node's text fits the rows it is given.
test("the story's text fits the page", () => {
  for (const each of NODES) {
    const placed = wrap(each.text, () => TEXT_WIDTH, TEXT_ROWS).reduce((sum, line) => sum + line.length, 0);
    assert.equal(placed, words(each.text).length, `${each.title} does not fit: ${each.text}`);
    assert.ok([...each.title].length <= 36, `${each.title}'s title`);
  }
});

// Every picture is a whole grid of known sixels, its backgrounds one a cell
// row, at the size its page leaves it.
test("every picture of the story is whole", () => {
  for (const picture of CROWN_PICTURES) {
    checkWhole(picture, [40, 18]);
  }
});

// Not in the Rust showcase: the coloured keys choose, and go nowhere, and
// digits select pages, which the story never used (INPUT.md section 9.4).
test("the coloured keys choose and digits select pages", () => {
  const harness = new Harness([new Crown(), new Station()]);
  harness.join(VIEWER, CROWN);
  harness.press(VIEWER, Key.F1);
  assert.ok(all(harness.own(VIEWER, CROWN)).includes(after(CROSSROADS, 0).title), "F1 chooses");
  assert.deepEqual(harness.gotos(VIEWER), [], "and goes nowhere");
  harness.typeText(VIEWER, "401");
  assert.deepEqual(harness.gotos(VIEWER), [STATION], "the story takes no digits");
  assert.deepEqual(harness.errors, []);
});
