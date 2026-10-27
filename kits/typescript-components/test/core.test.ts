// The library's core as the components rely on it: Tab moves the keys in
// the order components are drawn; a control's events go up the tree; what
// components need of their page is found before anything runs; a click
// says which region it fell in; a timer that runs only while viewed waits
// for a viewer; the bus reaches only the audience an event is for.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Harness, text } from "../../typescript/src/testing.ts";
import { type Canvas, Checkbox, type Child, Component, ComponentChannel, type ComponentEvent, Frame, h, Menu, type PageSpec, type PointerEvent, Rows, Switch, TextField, Ticker, ToastShelf } from "../src/index.ts";
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

function channel(specs: readonly PageSpec[]): { harness: Harness; components: ComponentChannel } {
  const components = new ComponentChannel(specs);
  return { harness: new Harness([components]), components };
}

/** Hears every event from under it. */
class Listener extends Component {
  static heard: string[] = [];

  override onComponentEvent(event: ComponentEvent): boolean {
    Listener.heard.push(`${event.type}:${JSON.stringify("value" in event ? event.value : null)}:${event.origin}`);
    return true;
  }
}

test("Tab moves the keys to the next control in the order they are drawn, and row 23 says what that control's keys do", () => {
  const { harness } = channel([{ page: PAGE, title: "Test", root: () => frame("Form", h(Checkbox, { label: "One" }), h(Switch, { label: "Two" })) }]);
  harness.join(1, PAGE);
  assert.match(text(harness.seen(1, PAGE).rows, 23), /ticks/u, "the first control has the keys");
  harness.press(1, Key.Tab);
  assert.match(text(harness.seen(1, PAGE).rows, 23), /turns it over/u, "Tab gave them to the switch");
  harness.press(1, Key.Enter);
  assert.match(text(harness.own(1, PAGE), 4), /ON/u);
  assert.doesNotMatch(text(harness.own(1, PAGE), 3), /x/u, "the check box was not ticked");
});

test("a control's change goes up the tree with its value and who did it", () => {
  Listener.heard = [];
  const { harness } = channel([{ page: PAGE, title: "Test", root: () => frame("Events", h(Listener, {}, h(Checkbox, { label: "Tick" }))) }]);
  harness.join(2, PAGE);
  harness.press(2, " ");
  assert.deepEqual(Listener.heard, ["change:true:viewer"]);
});

test("what components need is found before the program starts: digits for a numbered menu, a copy for each viewer for a control", () => {
  const { components } = channel([
    { page: PAGE, title: "Menu", root: () => frame("Menu", h(Menu, { shared: true, numbered: true, items: [{ label: "One" }], onChoose: () => {} })) },
    { page: OTHER, title: "Box", root: () => frame("Box", h(Rows, {}, h(Checkbox, { label: "Tick" }))) },
  ]);
  const entries = new Map(components.pages().map((entry) => [entry.page, entry]));
  assert.equal(entries.get(PAGE)?.ownsDigits, true);
  assert.equal(components.perViewer(PAGE), false, "a shared menu keeps one copy");
  assert.equal(components.perViewer(OTHER), true, "a check box is each viewer's own");
});

/** Marks two regions and says which was clicked. */
class Regions extends Component<object, { readonly clicked: string }> {
  override state = { clicked: "none" };

  override measure(): { width: number; height: number } {
    return { width: 40, height: 1 };
  }

  override paint(canvas: Canvas): void {
    canvas.coloured(0, 0, Colour.White, `left    right   ${this.state.clicked}`);
    canvas.region("left", 0, 0, 8);
    canvas.region("right", 0, 8, 8);
  }

  override onClick(event: PointerEvent): boolean {
    this.setState({ clicked: event.region ?? "nowhere" });
    return true;
  }
}

test("a click carries the region the component marked round it", () => {
  const { harness } = channel([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Regions", h(Regions, {})) }]);
  harness.join(3, PAGE);
  harness.clickLeft(3, 10, 3);
  assert.match(text(harness.own(3, PAGE), 3), /right +$/u);
  harness.clickLeft(3, 30, 3);
  assert.match(text(harness.own(3, PAGE), 3), /nowhere +$/u);
});

test("a timer that runs only while viewed waits while nobody watches, and catches up when someone comes", () => {
  const { harness } = channel([{ page: PAGE, title: "Test", root: () => frame("Ticker", h(Ticker, { items: ["one", "two", "three"], everyMs: 1_000 })) }]);
  assert.ok(harness.sharedPublications(PAGE) >= 1, "published at the start");
  harness.advance(5_000);
  assert.equal(harness.sharedPublications(PAGE), 0, "nothing published while nobody watched");
  harness.join(4, PAGE);
  assert.match(text(harness.seen(4, PAGE).rows, 3), /two/u, "one tick on arrival, not five");
});

test("an event on the bus reaches only its audience: a toast for one viewer shows on their copy alone", () => {
  /** Sends a toast to the viewer who presses Enter. */
  class Sender extends Component {
    override focusable(): boolean {
      return true;
    }

    override onKey(): boolean {
      this.services.bus.emit("toast", { kind: "info", text: "For you" }, { session: 6 });
      return true;
    }
  }
  const { harness } = channel([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Toasts", h(Sender, {}), h(ToastShelf, {})) }]);
  harness.join(6, PAGE);
  harness.join(7, PAGE);
  harness.press(7, Key.Enter);
  assert.match(text(harness.own(6, PAGE), 3), /For you/u);
  assert.equal(harness.ownPages(7, PAGE), undefined, "viewer 7 sees the page as everyone does");
});

test("a field keeps the digits: a page with one owns them", () => {
  const { components } = channel([{ page: PAGE, title: "Test", root: () => frame("Field", h(TextField, { longest: 10 })) }]);
  assert.equal(components.pages()[0]?.ownsDigits, true);
});

test("a field takes typing from each key's press with its text, and raw text from a phone (INPUT.md section 6.3)", () => {
  const { harness } = channel([{ page: PAGE, title: "Test", root: () => frame("Field", h(TextField, { longest: 10 })) }]);
  harness.join(1, PAGE);
  harness.typeKeys(1, "ab");
  harness.typeText(1, "cd");
  harness.key(1, "e", "release");
  assert.match(text(harness.seen(1, PAGE).rows, 3), /abcd/u);
  assert.doesNotMatch(text(harness.seen(1, PAGE).rows, 3), /abcde/u, "a release types nothing");
});

/** Shows what it was opened with. */
class Shows extends Component<{ readonly what: string }> {
  override measure(): { width: number; height: number } {
    return { width: 40, height: 1 };
  }

  override paint(canvas: Canvas): void {
    canvas.coloured(0, 0, Colour.White, `showing ${this.props.what}`);
  }
}

/** Sends the viewer who presses Enter to the other page, with what it is to show. */
class Sends extends Component {
  override focusable(): boolean {
    return true;
  }

  override onKey(event: { readonly session: number }): boolean {
    this.services.goto(event.session, OTHER, { what: "article 42" });
    return true;
  }
}

test("a viewer sent to a page with what it is to show sees it there: the page's root is made with it", () => {
  const { harness } = channel([
    { page: PAGE, title: "From", root: () => frame("From", h(Sends, {})) },
    { page: OTHER, title: "To", perViewer: true, root: (params) => frame("To", h(Shows, { what: String(params?.["what"] ?? "nothing") })) },
  ]);
  harness.join(8, PAGE);
  harness.press(8, Key.Enter);
  harness.open(8, OTHER);
  assert.match(text(harness.seen(8, OTHER).rows, 3), /showing article 42/u);
});

/** Starts a three-second stream for the viewer who presses Enter. */
class Plays extends Component {
  override focusable(): boolean {
    return true;
  }

  override onKey(event: { readonly session: number }): boolean {
    const samples = new Int16Array(48_000).fill(1);
    this.services.stream({ name: "three seconds", rate: 16_000, samples: () => samples, complete: true, failure: null, expected: samples.length }, event.session);
    return true;
  }
}

test("a stream is sent as a live sound, its blocks ahead of real time a tick at a time, and stopped when the viewer leaves its page", () => {
  const { harness } = channel([
    { page: PAGE, title: "Player", perViewer: true, root: () => frame("Player", h(Plays, {})) },
    { page: OTHER, title: "Other", root: () => frame("Other") },
  ]);
  harness.join(9, PAGE);
  harness.press(9, Key.Enter);
  const kinds = (): string[] => harness.sounds(9).map((message) => Object.keys(message).find((key) => ["live", "play", "blocks", "stop"].includes(key)) ?? "");
  assert.deepEqual(kinds().slice(0, 3), ["live", "play", "blocks"], "the live object, its play bound to the page, the first blocks");
  const firstBlocks = harness.sounds(9).filter((message) => "blocks" in message).length;
  harness.advance(1_000);
  assert.ok(harness.sounds(9).filter((message) => "blocks" in message).length > firstBlocks, "more blocks as time goes");
  harness.open(9, OTHER);
  assert.equal(kinds().at(-1), "stop");
});
