// The component library's core, driven as the server drives a channel (the
// kit's Harness): what is published and to whom, the lifecycle, input to
// the right component, and timers.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Harness, text } from "../../typescript/src/testing.ts";
import { type Canvas, type Child, Component, ComponentChannel, Frame, h, Headline, type KeyEvent, type PageSpec, Picture, ScrollingText, type Size, Ticker } from "../src/index.ts";
import { Colour, Key } from "../src/kit.ts";

const PAGE = 0x150;
const OTHER = 0x151;
const PROMPTS = [
  ["Index", 0x100],
  ["", null],
  ["", null],
  ["", null],
] as const;

function frame(title: string, ...body: Child[]): ReturnType<typeof h> {
  return h(Frame, { title, background: Colour.Blue, prompts: PROMPTS }, ...body);
}

interface CounterState {
  readonly count: number;
}

const events: string[] = [];

class Counter extends Component<{ readonly label: string }, CounterState> {
  override state: CounterState = { count: 0 };

  override componentDidMount(): void {
    events.push(`mount ${this.props.label}`);
  }

  override componentDidUpdate(_props: unknown, previous: CounterState): void {
    events.push(`update ${this.props.label} ${previous.count}->${this.state.count}`);
  }

  override componentWillUnmount(): void {
    events.push(`unmount ${this.props.label}`);
  }

  override focusable(): boolean {
    return true;
  }

  override onKey(event: KeyEvent): boolean {
    if (event.key === Key.Enter && event.action === "press") {
      this.setState((state) => ({ count: state.count + 1 }));
      return true;
    }
    return false;
  }

  override measure(space: Size): Size {
    return { width: space.width, height: 1 };
  }

  override paint(canvas: Canvas): void {
    canvas.coloured(0, 0, Colour.White, `${this.props.label} ${this.state.count}`);
  }
}

function channel(specs: readonly PageSpec[]): Harness {
  return new Harness([new ComponentChannel(specs)]);
}

test("a page every viewer shares is published once at the start, with its frame", () => {
  const harness = channel([{ page: PAGE, title: "Test", root: () => frame("Shared page", h(Headline, { title: "A headline", page: OTHER })) }]);
  const rows = harness.shared(PAGE);
  assert.match(text(rows, 1), /Shared page/u);
  assert.match(text(rows, 3), /151\x07A headline/u);
  assert.match(text(rows, 24), /Index/u);
  assert.equal(harness.sharedPublications(PAGE), 1);
});

test("a viewer's own copy is sent only once it differs from the shared one, and released when it matches again", () => {
  events.length = 0;
  const harness = channel([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Counter", h(Counter, { label: "count" })) }]);
  harness.join(7, PAGE);
  assert.equal(harness.ownPages(7, PAGE), undefined, "nothing differs yet, so nothing of its own is sent");
  harness.press(7, Key.Enter);
  assert.match(text(harness.own(7, PAGE), 3), /count 1/u);
  assert.match(text(harness.shared(PAGE), 3), /count 0/u, "the shared copy is unchanged");
  harness.open(7, OTHER);
  assert.ok(harness.sent.some((message) => message["type"] === "release" && message["session"] === 7), "leaving releases its own copy");
  // Each copy's counter is given the keys at its first draw and renders again for it (the mount forces it: what a component renders may say whether it has the keys), so each mount has one update with nothing changed.
  assert.deepEqual(events, ["mount count", "update count 0->0", "mount count", "update count 0->0", "update count 0->1", "unmount count"]);
});

test("keys go to the focused component; pointer clicks to the component drawn in the cell", () => {
  const harness = channel([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Links", h(Headline, { title: "Go to 151", page: OTHER })) }]);
  harness.join(3, PAGE);
  harness.clickLeft(3, 10, 3);
  assert.deepEqual(harness.gotos(3), [OTHER]);
});

test("scrolling text moves with the keys and its scroll bar can be dragged", () => {
  const lines = Array.from({ length: 50 }, (_, index) => ({ foreground: Colour.White, text: `Line ${index + 1}` }));
  const harness = channel([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Scroll", h(ScrollingText, { lines, shown: 10 })) }]);
  harness.join(4, PAGE);
  harness.press(4, Key.PageDown);
  assert.match(text(harness.own(4, PAGE), 3), /Line 10/u, "a screen less one line");
  // The block is at the top of the track; grip it and drag it to the bottom.
  harness.press(4, Key.Home);
  harness.mouse(4, "press", 0, 39, 3);
  harness.move(4, 1, 39, 12);
  harness.mouse(4, "release", 0, 39, 12);
  assert.match(text(harness.own(4, PAGE), 12), /Line 50/u);
});

test("a timer runs while its component is on the page and stops when it leaves", () => {
  const harness = channel([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Ticker", h(Ticker, { items: ["one", "two", "three"], everyMs: 1_000 })) }]);
  harness.join(5, PAGE);
  harness.advance(1_000);
  // The shared copy ticks in step with the viewer's, so the viewer is sent nothing of its own.
  assert.match(text(harness.seen(5, PAGE).rows, 3), /two/u);
  assert.equal(harness.ownPages(5, PAGE), undefined);
  harness.advance(1_000);
  assert.match(text(harness.seen(5, PAGE).rows, 3), /three/u);
  harness.open(5, OTHER);
  const before = harness.sent.length;
  harness.advance(5_000);
  assert.equal(harness.sent.slice(before).filter((message) => message["session"] === 5).length, 0, "nothing more for the viewer who left");
});

test("a picture's TTI with CR LF line ends draws no Double Height: a CR kept in a row would hide the row below", () => {
  const tti = "PN,10000\r\nOL,2,\u001bWABC\r\nOL,3,\u001bWDEF\r\n";
  const harness = channel([{ page: PAGE, title: "Test", root: () => frame("QR", h(Picture, { tti, firstRow: 2 })) }]);
  const rows = harness.shared(PAGE);
  // Row 1 is the title band, double height on purpose; the picture is under it.
  assert.ok(!rows.slice(3).some((codes) => codes.includes(0x0d)), "no code 0x0D under the band");
  assert.match(text(rows, 3), /ABC/u);
  assert.match(text(rows, 4), /DEF/u);
});
