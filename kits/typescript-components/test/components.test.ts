// Components whose behaviour pages rely on: dates written one way
// everywhere, a centred stack, a list whose choice by a click moves its
// band, so what shows beside it is the item chosen, controls that look
// disabled when they are, and a tree whose view keeps full as it shuts.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Harness, text } from "../../typescript/src/testing.ts";
import { Button, Centred, type KeyEvent, type Child, ChoiceList, Collapsible, Columns, Component, ComponentChannel, DATE_FORMATS, DateText, DISABLED, formatDate, Frame, h, type PageSpec, Pagination, Slider, TabView, Text, Tree } from "../src/index.ts";
import { Colour, COLUMNS, Key } from "../src/kit.ts";

const PAGE = 0x150;
const PROMPTS = [
  ["Index", 0x100],
  ["", null],
  ["", null],
  ["", null],
] as const;
/** The frame's first body row, where a page's first component is drawn. */
const BODY_ROW = 3;

function frame(title: string, ...body: Child[]): ReturnType<typeof h> {
  return h(Frame, { title, background: Colour.Blue, prompts: PROMPTS }, ...body);
}

function harness(specs: readonly PageSpec[]): Harness {
  return new Harness([new ComponentChannel(specs)]);
}

test("a date is written in its pattern's tokens, UTC, whatever form it comes in", () => {
  const moment = Date.UTC(2026, 9, 7, 14, 5, 9);
  assert.equal(formatDate(moment, DATE_FORMATS.short), "07 Oct 26");
  assert.equal(formatDate(moment, DATE_FORMATS.long), "07 Oct 2026");
  assert.equal(formatDate(moment, DATE_FORMATS.numeric), "07.10.26");
  assert.equal(formatDate(moment, DATE_FORMATS.iso), "2026-10-07");
  assert.equal(formatDate(moment, DATE_FORMATS.stamp), "Wed 07 Oct 14:05");
  assert.equal(formatDate(moment, DATE_FORMATS.month), "October 2026");
  assert.equal(formatDate(moment, DATE_FORMATS.seconds), "14:05:09");
  assert.equal(formatDate(moment, "dddd D M [at] H"), "Wednesday 7 10 at 14", "unpadded tokens, and text in brackets kept");
  assert.equal(formatDate(new Date(moment), "DD MMM YYYY"), "07 Oct 2026", "a Date");
  assert.equal(formatDate("2021-07-16", DATE_FORMATS.long), "16 Jul 2021", "a calendar day is that day, not midnight elsewhere");
  assert.equal(formatDate("2026-10-07 23:30:00", DATE_FORMATS.stamp), "Wed 07 Oct 23:30", "a time with no zone is UTC");
  assert.equal(formatDate("2026-10-07T23:30:00+10:00", DATE_FORMATS.stamp), "Wed 07 Oct 13:30", "a zone given is taken");
  assert.equal(formatDate("not a date", DATE_FORMATS.long), "");
  assert.equal(formatDate(Number.NaN, DATE_FORMATS.long, "undated"), "undated");
});

test("a date component shows its date in its colour, or its fallback", () => {
  const seen = harness([{ page: PAGE, title: "Test", root: () => frame("Dates", h(DateText, { value: "2021-07-16" }), h(DateText, { value: "", fallback: "no date" })) }]);
  seen.join(1, PAGE);
  assert.match(text(seen.seen(1, PAGE).rows, BODY_ROW), /^\x0716 Jul 2021 *$/u, "after its colour, white");
  assert.match(text(seen.seen(1, PAGE).rows, BODY_ROW + 1), /no date/u);
});

test("a centred stack puts each child in the middle of the width at its own size", () => {
  const seen = harness([{ page: PAGE, title: "Test", root: () => frame("Centred", h(Centred, {}, h(Text, { text: "ab" }), h(Text, { text: "abcdef" }))) }]);
  seen.join(1, PAGE);
  const rows = seen.seen(1, PAGE).rows;
  const short = text(rows, BODY_ROW);
  const long = text(rows, BODY_ROW + 1);
  const middle = (line: string, word: string): number => line.indexOf(word) + word.length / 2;
  assert.ok(Math.abs(middle(short, "ab") - middle(long, "abcdef")) <= 1, `${JSON.stringify(short)} over ${JSON.stringify(long)}`);
  assert.ok(short.indexOf("ab") > 10, "not at the left");
});

/** A list beside what its chosen item is, as the rounds page has. */
class Chooser extends Component<object, { readonly chosen: number; readonly pressed: string }> {
  override state = { chosen: 0, pressed: "none" };

  override render(): Child {
    const items = ["first", "second", "third"];
    return [
      h(ChoiceList, { items, shown: 3, onMove: (chosen: number) => this.setState({ chosen }), onChoose: () => {} }),
      h(Text, { text: `showing ${items[this.state.chosen] ?? ""}` }),
      h(Button, { label: "Act", onPress: () => this.setState({ pressed: items[this.state.chosen] ?? "" }) }),
      h(Text, { text: `acted on ${this.state.pressed}` }),
    ];
  }
}

test("a click on a list's item moves its band there, so what shows beside the list and its buttons act on that item", () => {
  const seen = harness([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Choose", h(Chooser, {})) }]);
  seen.join(1, PAGE);
  seen.clickLeft(1, 5, BODY_ROW + 2);
  assert.match(text(seen.own(1, PAGE), BODY_ROW + 3), /showing third/u);
  seen.clickLeft(1, 3, BODY_ROW + 4);
  assert.match(text(seen.own(1, PAGE), BODY_ROW + 5), /acted on third/u);
});

test("a list's rows hold components: each item drawn by what the list's user gives, in the row's look", () => {
  const items = [{ name: "Launch", day: "2021-07-16" }];
  const seen = harness([
    {
      page: PAGE,
      title: "Test",
      root: () =>
        frame(
          "Rows",
          h(ChoiceList<{ name: string; day: string }>, {
            items,
            shown: 1,
            render: (item, look) => h(Columns, { widths: [10, null] }, h(Text, { text: item.name, foreground: look.foreground }), h(DateText, { value: item.day, foreground: look.foreground })),
            onChoose: () => {},
          }),
        ),
    },
  ]);
  seen.join(1, PAGE);
  assert.match(text(seen.seen(1, PAGE).rows, BODY_ROW), /Launch +.16 Jul 2021/u);
});

test("a button fits its label with a cell each side, or its padding, or a width given, its label centred in it", () => {
  assert.equal(Button.widthOf("stop"), 10, "band, foreground, a space, four letters, a space, the cell after");
  assert.equal(Button.widthOf("stop", 2, 3), 13);
  const seen = harness([{ page: PAGE, title: "Test", root: () => frame("Buttons", h(Button, { label: "go", width: 12, onPress: () => {} }), h(Button, { label: "go", onPress: () => {} })) }]);
  seen.join(1, PAGE);
  const fixed = text(seen.seen(1, PAGE).rows, BODY_ROW);
  assert.equal(fixed.indexOf("go"), 6, `centred in 8 cells of room after its band: ${JSON.stringify(fixed)}`);
});

/** Plays three seconds on Enter, pauses on "p", resumes on "r", and shows what plays. */
class Player extends Component<object, { readonly shown: string }> {
  override state = { shown: "" };

  override focusable(): boolean {
    return true;
  }

  override onKey(event: KeyEvent): boolean {
    if (event.action === "release") {
      return false;
    }
    const samples = new Int16Array(48_000).fill(1);
    if (event.key === Key.Enter) {
      this.services.stream({ name: "three seconds", rate: 16_000, samples: () => samples, complete: true, failure: null, expected: samples.length }, event.session);
    } else if (event.key === "p") {
      this.services.pauseStream(event.session);
    } else if (event.key === "r") {
      this.services.resumeStream(event.session);
    }
    const listening = this.services.listening(event.session);
    this.setState({ shown: `${listening?.state ?? "none"} ${listening?.heardMs ?? 0}` });
    return true;
  }

  override render(): Child {
    return h(Text, { text: this.state.shown });
  }
}

test("a stream pauses where it was heard to, sending nothing, and resumes from there as a new stream", () => {
  const seen = harness([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Player", h(Player, {})) }]);
  seen.join(1, PAGE);
  seen.press(1, Key.Enter);
  seen.runUntil(1_000);
  seen.press(1, "p");
  assert.match(text(seen.own(1, PAGE), BODY_ROW), /paused 1000/u);
  assert.ok("stop" in (seen.sounds(1).at(-1) ?? {}), "the receiver stops it");
  const sent = seen.sounds(1).length;
  seen.runUntil(3_000);
  assert.equal(seen.sounds(1).length, sent, "nothing is sent while paused");
  seen.press(1, "r");
  const generations = seen.sounds(1).flatMap((message) => ("live" in message ? [(message["live"] as { generation: number }).generation] : []));
  assert.deepEqual(generations, [1, 2], "resumed as a new stream");
  seen.runUntil(3_500);
  seen.press(1, "x");
  assert.match(text(seen.own(1, PAGE), BODY_ROW), /playing 1500/u, "heard from where it paused");
});

test("a disabled control looks it: tabs, a heading, the page now and a slider's value in the disabled look, not their own", () => {
  const seen = harness([
    {
      page: PAGE,
      title: "Test",
      root: () =>
        frame(
          "Disabled",
          h(TabView, { tabs: ["One", "Two"], disabled: true }),
          h(Collapsible, { titles: ["A"], disabled: true }),
          h(Pagination, { count: 5, disabled: true }),
          h(Slider, { label: "Volume", min: 0, max: 10, disabled: true }),
        ),
    },
  ]);
  seen.join(1, PAGE);
  const rows = seen.seen(1, PAGE).rows;
  // The chosen tab's chip on blue, the other's name, after its colour's cell in its slot, in DISABLED.
  assert.deepEqual([rows[BODY_ROW]?.[0], rows[BODY_ROW]?.["One".length + 4 + 2]], [Colour.Blue, DISABLED]);
  assert.deepEqual([rows[BODY_ROW + 1]?.[0], rows[BODY_ROW + 1]?.[2]], [Colour.Black, DISABLED], "no band, its title dim");
  assert.equal(rows[BODY_ROW + 2]?.[2], Colour.Blue, "the page now on a dim chip, after the dim arrow's two cells");
  assert.equal(rows[BODY_ROW + 3]?.[COLUMNS - 6], DISABLED, "the value after the track");
});

test("a tree's view comes up when an item is shut, so none of it is left empty", () => {
  const leaves = (count: number): { label: string }[] => Array.from({ length: count }, (_, index) => ({ label: `leaf ${index + 1}` }));
  const seen = harness([{ page: PAGE, title: "Test", root: () => frame("Tree", h(Tree, { nodes: [{ label: "P", children: leaves(10) }, { label: "Q", children: leaves(3) }], open: ["P", "Q"], shown: 4 })) }]);
  seen.join(1, PAGE);
  seen.press(1, Key.End);
  for (let step = 0; step < 3; step += 1) {
    seen.press(1, Key.Up);
  }
  // Q, shut, is the last of the twelve rows left: the view shows the four that end with it.
  seen.press(1, Key.Enter);
  const rows = seen.seen(1, PAGE).rows;
  assert.match(text(rows, BODY_ROW + 3), /\+ Q/u);
  assert.match(text(rows, BODY_ROW), /leaf 8/u);
});
