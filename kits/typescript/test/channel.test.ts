import assert from "node:assert/strict";
import { test } from "node:test";

import { type Application, type Context, type PageEntry, Random } from "../src/channel.ts";
import { type Input, Key } from "../src/input.ts";
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
