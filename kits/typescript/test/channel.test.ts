import assert from "node:assert/strict";
import { test } from "node:test";

import { type Application, type Context, type PageEntry, Random } from "../src/channel.ts";
import { Button, Held, type Input, Key } from "../src/input.ts";
import { Colour, Screen } from "../src/screen.ts";
import { Harness, shows } from "../src/testing.ts";

/** A page with `text` on row 1 and Fastext links. */
function screen(text: string, links: readonly [number | null, number | null, number | null, number | null] = [null, null, null, null]): Screen {
  const drawn = new Screen();
  drawn.coloured(1, 0, Colour.White, text);
  drawn.fastext(links);
  return drawn;
}

/** An application that records what it is told. */
class Recorder implements Application {
  readonly heard: string[] = [];
  readonly #entries: readonly PageEntry[];

  constructor(entries: readonly PageEntry[]) {
    this.#entries = entries;
  }

  pages(): readonly PageEntry[] {
    return this.#entries;
  }

  start(context: Context): void {
    for (const { page, title } of this.#entries) {
      context.publish("channel", page, screen(title, [0x100, 0x101, null, null]));
    }
  }

  joined(session: number): void {
    this.heard.push(`joined ${session}`);
  }

  opened(session: number, page: number): void {
    this.heard.push(`opened ${session} ${page.toString(16)}`);
  }

  leftPage(session: number, page: number): void {
    this.heard.push(`left ${session} ${page.toString(16)}`);
  }

  input(session: number, page: number, input: Input): void {
    this.heard.push(`input ${session} ${page.toString(16)} ${JSON.stringify(input)}`);
  }

  ended(session: number): void {
    this.heard.push(`ended ${session}`);
  }
}

test("pages are declared in order, the first application to declare one serving it", () => {
  const first = new Recorder([
    { page: 0x100, title: "Index" },
    { page: 0x101, title: "News" },
  ]);
  const second = new Recorder([
    { page: 0x101, title: "Also news" },
    { page: 0x200, title: "Games", ownsDigits: true },
  ]);
  const harness = new Harness([first, second]);
  assert.deepEqual(harness.sent[0], {
    type: "pages",
    pages: [
      { page: "100", title: "Index" },
      { page: "101", title: "News" },
      { page: "200", title: "Games" },
    ],
  });
  harness.join(1, 0x101);
  assert.deepEqual(first.heard, ["joined 1", "opened 1 101"]);
  assert.deepEqual(second.heard, ["joined 1"], "every application hears a join, only the owner the page");
  harness.open(1, 0x200);
  assert.deepEqual(first.heard.slice(2), ["left 1 101"]);
  assert.deepEqual(second.heard.slice(1), ["opened 1 200"]);
});

test("digits and colour keys navigate on a page that does not use them, and reach one that does", () => {
  const index = new Recorder([
    { page: 0x100, title: "Index" },
    { page: 0x101, title: "News" },
  ]);
  const games = new Recorder([{ page: 0x200, title: "Games", ownsDigits: true }]);
  const harness = new Harness([index, games]);
  harness.join(1, 0x100);
  harness.typeText(1, "200");
  assert.deepEqual(harness.gotos(1), [0x200]);
  harness.press(1, Key.F2);
  assert.deepEqual(harness.gotos(1), [0x200, 0x101], "green follows the page's green link");
  assert.ok(index.heard.every((line) => !line.startsWith("input")), "navigation took them");
  harness.typeText(1, "x");
  assert.equal(index.heard.at(-1), `input 1 100 {"kind":"text","text":"x"}`);

  harness.open(1, 0x200);
  harness.typeText(1, "101");
  assert.equal(games.heard.at(-1), `input 1 200 {"kind":"text","text":"101"}`, "the page's own digits");
  assert.deepEqual(harness.gotos(1), [0x200, 0x101]);
});

test("colour keys follow the links of the copy the viewer sees", () => {
  const own: Application = {
    pages: () => [{ page: 0x100, title: "Index" }],
    start: (context) => context.publish("channel", 0x100, screen("shared", [0x101, null, null, null])),
    opened: (session, page, context) => context.publish(session, page, screen("own", [0x102, null, null, null])),
  };
  const harness = new Harness([own]);
  harness.join(1, 0x100);
  harness.join(2, 0x100);
  harness.press(1, Key.Red);
  assert.deepEqual(harness.gotos(1), [0x102]);
  assert.ok(shows(harness.own(1, 0x100), "own"));
  assert.ok(shows(harness.shared(0x100), "shared"));
  harness.close(1);
  harness.join(1, 0x100);
  assert.ok(shows(harness.own(1, 0x100), "own"));
});

test("a released copy leaves the viewer the channel's, its links with it", () => {
  const greeting: Application = {
    pages: () => [{ page: 0x100, title: "Index", section: "Front" }],
    start: (context) => context.publish("channel", 0x100, screen("shared", [0x101, null, null, null])),
    opened: (session, page, context) => context.publish(session, page, screen("hello", [0x102, null, null, null])),
    input: (session, page, _input, context) => {
      context.release(session, page);
      context.release(session, page);
    },
  };
  const harness = new Harness([greeting]);
  assert.deepEqual(harness.sent[0], { type: "pages", pages: [{ page: "100", title: "Index", section: "Front" }] }, "the section is declared");
  harness.join(1, 0x100);
  assert.ok(shows(harness.own(1, 0x100), "hello"));
  harness.typeText(1, "x");
  assert.equal(harness.sent.filter((message) => message["type"] === "release").length, 1, "released once: then it has no copy of its own");
  assert.equal(harness.ownPages(1, 0x100), undefined);
  assert.ok(shows(harness.seen(1, 0x100).rows, "shared"));
  harness.press(1, Key.Red);
  assert.deepEqual(harness.gotos(1), [0x101], "red follows the channel's link now");
});

test("a page retitled says so once, and the contents list it so", () => {
  const story: Application = {
    pages: () => [{ page: 0x201, title: "Story" }],
    start: (context) => {
      context.title(0x201, "Robot arm");
      context.title(0x201, "Robot arm");
    },
  };
  const harness = new Harness([story]);
  assert.equal(harness.sent.filter((message) => message["type"] === "title").length, 1);
  assert.equal(harness.title(0x201), "Robot arm");
  assert.equal(harness.title(0x202), undefined);
});

test("a click the page does not take follows the prompt or page number it is on", () => {
  const taken: string[] = [];
  const front: Application = {
    pages: () => [
      { page: 0x100, title: "Index" },
      { page: 0x200, title: "Two" },
    ],
    start: (context) => {
      const drawn = screen("See 200 here", [0x200, 0x101, null, null]);
      drawn.coloured(24, 0, Colour.Red, "Two");
      drawn.coloured(24, 10, Colour.Green, "News");
      context.publish("channel", 0x100, drawn);
      context.publishTti("channel", 0x200, "PN,20000\nOL,3,Back to 100\n");
    },
    // The page takes a click on row 1's first cell, and nothing else.
    input: (_session, _page, input) => {
      const mine = input.kind === "mouse" && input.row === 1 && input.column === 0;
      if (mine) {
        taken.push(input.action);
      }
      return mine;
    },
  };
  const harness = new Harness([front]);
  harness.join(1, 0x100);
  harness.clickLeft(1, 6, 1);
  harness.clickLeft(1, 12, 24);
  harness.clickLeft(1, 0, 1);
  harness.click(1, Button.Right, 6, 1);
  harness.move(1, Held.Left, 6, 1);
  assert.deepEqual(harness.gotos(1), [0x200, 0x101], "the number, then the green prompt; never the page's own click, a right click or a move");
  assert.deepEqual(taken, ["press", "release"]);
  harness.open(1, 0x200);
  harness.clickLeft(1, 9, 3);
  assert.deepEqual(harness.gotos(1).slice(2), [0x100], "a page published from TTI, its rows read back");
});

test("the server's figures reach every application, which may name the channel's pages", () => {
  const shown: string[] = [];
  const figures = (name: string): Application => ({
    pages: () => [{ page: name === "one" ? 0x100 : 0x150, title: name }],
    start: () => undefined,
    stats: (stats, context) => shown.push(`${name} ${String(stats.channel["viewers"])} ${context.titleOf(0x150) ?? "none"} ${context.titleOf(0x151) ?? "none"}`),
  });
  const harness = new Harness([figures("one"), figures("two")]);
  harness.stats({ server: {}, channel: { viewers: 4 } });
  assert.deepEqual(shown, ["one 4 two none", "two 4 two none"]);
});

test("each application wakes when it asked, the earliest asked for winning", () => {
  const woken: string[] = [];
  const clock = (name: string, first: number, every: number): Application => ({
    pages: () => [],
    start: (context) => {
      context.wakeAt(first + 50);
      context.wakeAt(first);
    },
    wake: (context) => {
      woken.push(`${name} ${context.now()}`);
      context.wakeIn(every);
    },
  });
  const harness = new Harness([clock("a", 100, 100), clock("b", 150, 1_000)]);
  assert.equal(harness.channel.nextWake(), 100);
  harness.advance(99);
  assert.deepEqual(woken, []);
  harness.advance(1);
  assert.deepEqual(woken, ["a 100"], "only the application due");
  harness.runUntil(450);
  assert.deepEqual(woken, ["a 100", "b 150", "a 200", "a 300", "a 400"]);
  assert.equal(harness.now, 450);
  assert.equal(harness.channel.nextWake(), 500);
});

test("the real time is the server's, from start", () => {
  let seen = 0;
  const harness = new Harness(
    [
      {
        pages: () => [],
        start: (context) => context.wakeIn(1_000),
        wake: (context) => {
          seen = context.epochMs();
        },
      },
    ],
    { epochMs: 1_792_886_400_000 },
  );
  harness.advance(1_000);
  assert.equal(seen, 1_792_886_401_000);
});

test("a session first named after the program started again joins then, and ends once", () => {
  const recorder = new Recorder([{ page: 0x100, title: "Index" }]);
  const harness = new Harness([recorder]);
  harness.channel.input(9, 0x100, { kind: "text", text: "x" });
  assert.deepEqual(recorder.heard.slice(0, 2), ["joined 9", `input 9 100 {"kind":"text","text":"x"}`]);
  harness.channel.close(9);
  harness.channel.close(9);
  harness.channel.close(10);
  assert.deepEqual(recorder.heard.slice(2), ["ended 9"]);
});

test("an application that fails is reported, and the channel goes on", () => {
  const harness = new Harness([
    {
      pages: () => [{ page: 0x100, title: "Index" }],
      start: (context) => context.publish("channel", 0x100, screen("up")),
      input: () => {
        throw new Error("broken");
      },
      opened: (_session, _page, context) => context.publish("channel", 0x101, screen("undeclared")),
    },
  ]);
  harness.join(1, 0x100);
  harness.typeText(1, "x");
  assert.equal(harness.errors.length, 2);
  assert.match(harness.errors[0] ?? "", /101 was not declared/);
  assert.match(harness.errors[1] ?? "", /broken/);
  assert.ok(shows(harness.shared(0x100), "up"));
  harness.channel.input(1, 0x150, { kind: "focus" });
  assert.match(harness.errors[2] ?? "", /no application serves/);
});

test("random numbers are the runtime's xorshift64*", () => {
  const random = new Random(7n);
  assert.deepEqual([random.next(), random.next(), random.next()], [3_522_932_607, 3_986_975_863, 383_753_578]);
  assert.equal(new Random(0n).next(), new Random(1n).next(), "a zero seed is made odd, as the runtime's is");
  assert.equal(random.below(0), 0);
  for (let draw = 0; draw < 1_000; draw += 1) {
    assert.ok(random.below(3) < 3);
  }
});

test("a page published from TTI keeps its Fastext links for the coloured keys", () => {
  const harness = new Harness([
    {
      pages: () => [{ page: 0x601, title: "Picture" }],
      start: (context) => context.publishTti("channel", 0x601, "PN,60101\nOL,1,x\nFL,602,8FF,8FF,600,8FF,100\nOL,26,@ABC\n"),
    },
  ]);
  harness.join(1, 0x601);
  harness.press(1, Key.Red);
  harness.press(1, Key.Blue);
  assert.deepEqual(harness.gotos(1), [0x602, 0x600]);
  assert.deepEqual(harness.errors, []);
});
