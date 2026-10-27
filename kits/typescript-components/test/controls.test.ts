// Controls and pointer areas (core/controls.ts, INPUT.md sections 7.3 and
// 7.4): the triplets as the spec's examples give them, packed as the
// engine's TTI reader takes them; what a page keeps of what its components
// declare; the kit's components declaring exactly the cells a click acts
// on, on the first sub-page alone, and only where a click reaches them;
// a move with no button held reaching the component whose pointer
// area it is in; and a hover reaching the component that declared the
// control.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Harness, text } from "../../typescript/src/testing.ts";
import {
  At,
  BAR_CELLS,
  Button,
  Calendar,
  type CalendarDate,
  type Canvas,
  type Child,
  CheckboxGroup,
  ChoiceList,
  Component,
  ComponentChannel,
  controlLines,
  controlTriplet,
  controlTriplets,
  DataTable,
  DATE_FORMATS,
  Dialog,
  enhancementLines,
  formatDate,
  Frame,
  Gap,
  h,
  type HoverEvent,
  keptControls,
  keptDeclarations,
  Layers,
  MAX_CONTROLS,
  MAX_POINTER_AREAS,
  Modal,
  PageLink,
  type PageSpec,
  PaintCanvas,
  pointerAreaTriplet,
  type PointerEvent,
  readControls,
  readDeclarations,
  Select,
  setActivePosition,
  type Size,
  Slider,
  Switch,
  TERMINATION,
  Text,
  WHOLE_PAGE_POINTER_AREA,
} from "../src/index.ts";
import { Colour, COLUMNS, Key } from "../src/kit.ts";

const PAGE = 0x150;
const PROMPTS = [
  ["Index", 0x100],
  ["", null],
  ["", null],
  ["", null],
] as const;
/** The frame's first body row. */
const BODY_ROW = 3;

function frame(title: string, ...body: Child[]): ReturnType<typeof h> {
  return h(Frame, { title, background: Colour.Blue, prompts: PROMPTS }, ...body);
}

/** The TTI of the latest copy of `page` published, shared or a viewer's. */
function latestTti(harness: Harness, page: number): string {
  const published = harness.publications().findLast((message) => Number.parseInt(String(message["page"]), 16) === page);
  assert.ok(published !== undefined, `page ${page.toString(16)} was published`);
  return String(published["tti"]);
}

/** The controls of the first sub-page of the latest copy of `page`. */
function controlsOf(harness: Harness, page: number): ReturnType<typeof readControls>[number] {
  return readControls(latestTti(harness, page))[0] ?? [];
}

/** The pointer areas of the first sub-page of the latest copy of `page`. */
function areasOf(harness: Harness, page: number): ReturnType<typeof readDeclarations>[number]["pointer"] {
  return readDeclarations(latestTti(harness, page))[0]?.pointer ?? { areas: [], wholePage: false };
}

/** Boxes in reading order, top to bottom and left to right. */
function inOrder<T extends { row: number; column: number }>(boxes: readonly T[]): T[] {
  return [...boxes].sort((a, b) => a.row - b.row || a.column - b.column);
}

/** One viewer's page, every component its own. */
function viewer(...body: Child[]): Harness {
  const seen = harness([{ page: PAGE, title: "Test", perViewer: true, root: () => frame("Test", ...body) }]);
  seen.join(1, PAGE);
  return seen;
}

function harness(specs: readonly PageSpec[]): Harness {
  return new Harness([new ComponentChannel(specs)]);
}

test("the triplets are INPUT.md section 7.3's: four buttons on row 20, nine cells each, and the Termination Marker", () => {
  assert.equal(setActivePosition(20), 0x0013c);
  assert.equal(controlTriplet(1, 9, false), 0x04941);
  assert.equal(controlTriplet(11, 9, false), 0x0494b);
  assert.equal(controlTriplet(21, 9, false), 0x04955);
  assert.equal(controlTriplet(31, 9, false), 0x0495f);
  assert.equal(controlTriplet(2, 21, true), 0x2a942, "D6 joins it to the run above");
  assert.equal(TERMINATION, 0x03fff);
});

test("an X/26 packet is an OL,26 line: its designation code, then thirteen triplets of three characters, six bits each, the least significant first", () => {
  const [line, ...more] = enhancementLines([setActivePosition(20), TERMINATION]);
  assert.deepEqual(more, []);
  assert.ok(line !== undefined);
  const body = line.slice("OL,26,".length);
  assert.ok(line.startsWith("OL,26,"));
  assert.equal(body.length, 1 + 13 * 3);
  assert.equal(body[0], "@", "designation code 0");
  // 0x0013C: 0x3C, 0x04, 0x00, each with bit 6 set.
  assert.equal(body.slice(1, 4), "|D@");
  // 0x03FFF: 0x3F, 0x3F, 0x03; the rest of the packet is Termination Markers too.
  assert.equal(body.slice(4), "\x7f\x7fC".repeat(12));
  assert.deepEqual(
    enhancementLines(new Array<number>(14).fill(TERMINATION)).map((each) => each.slice(0, 7)),
    ["OL,26,@", "OL,26,A"],
    "fourteen triplets take two packets, designation codes 0 and 1",
  );
});

test("a page keeps the controls declared in rows 1 to 23 and the page's columns, the first of two that overlap, and 32 at most", () => {
  const kept = keptControls([
    { row: 0, column: 0, width: 5, height: 2 },
    { row: 5, column: 38, width: 5, height: 1 },
    { row: 5, column: 0, width: 4, height: 1 },
    { row: 5, column: 3, width: 4, height: 1 },
    { row: 24, column: 0, width: 4, height: 1 },
    { row: 6, column: 0, width: 0, height: 1 },
  ]);
  assert.deepEqual(kept, [
    { row: 1, column: 0, width: 5, height: 1 },
    { row: 5, column: 38, width: 2, height: 1 },
    { row: 5, column: 0, width: 4, height: 1 },
  ]);
  const many = Array.from({ length: 40 }, (_, index) => ({ row: 1 + (index % 20), column: Math.floor(index / 20) * 10, width: 9, height: 1 }));
  assert.equal(keptControls(many).length, MAX_CONTROLS);
});

test("each row's runs follow its Set Active Position triplet, left to right; a taller control's rows below are joined; the data ends with the Termination Marker", () => {
  const lines = controlLines([
    { row: 4, column: 20, width: 10, height: 2 },
    { row: 4, column: 2, width: 6, height: 1 },
  ]);
  assert.equal(lines.length, 1);
  assert.deepEqual(readControls(`PN,15000\n${lines.join("\n")}\n`), [
    [
      { row: 4, column: 2, width: 6, height: 1 },
      { row: 4, column: 20, width: 10, height: 2 },
    ],
  ]);
  assert.deepEqual(controlLines([]), [], "no controls, no packet");
});

test("a button's band is a control: from its New Background cell to the cell before its Black Background; a disabled one declares none", () => {
  const width = Button.widthOf("press");
  const seen = harness([{ page: PAGE, title: "Test", root: () => frame("Buttons", h(Button, { label: "press", onPress: () => {} }), h(Button, { label: "off", disabled: true, onPress: () => {} })) }]);
  seen.join(1, PAGE);
  assert.deepEqual(controlsOf(seen, PAGE), [{ row: BODY_ROW, column: 1, width: width - 2, height: 1 }]);
  assert.doesNotMatch(latestTti(seen, PAGE), /^IM,/m, "the page asks for no input: every event is sent (INPUT.md section 6.3)");
});

test("a list's rows are controls, a taller item's rows joined, and its page stays one copy every viewer shares", () => {
  const components = new ComponentChannel([
    { page: PAGE, title: "Test", root: () => frame("Rows", h(ChoiceList, { items: ["first", "second"], shown: 4, itemRows: 2, shared: true, onChoose: () => {} })) },
  ]);
  assert.equal(components.perViewer(PAGE), false);
  const seen = new Harness([components]);
  seen.join(1, PAGE);
  // Its rows beside the scroll bar: the band from its New Background to the cell before the Black Background in the rows' last cell.
  const width = COLUMNS - BAR_CELLS - 2;
  assert.deepEqual(controlsOf(seen, PAGE), [
    { row: BODY_ROW, column: 1, width, height: 2 },
    { row: BODY_ROW + 2, column: 1, width, height: 2 },
  ]);
});

test("a disabled list declares no controls", () => {
  const seen = harness([{ page: PAGE, title: "Test", root: () => frame("Rows", h(ChoiceList, { items: ["first", "second"], shown: 2, disabled: true, onChoose: () => {} })) }]);
  seen.join(1, PAGE);
  assert.deepEqual(controlsOf(seen, PAGE), []);
  assert.ok(!latestTti(seen, PAGE).includes("OL,26,"), "no packet X/26");
});

test("a dialog over a page has the controls: the buttons under it cannot be clicked, so they declare none", () => {
  const seen = harness([
    {
      page: PAGE,
      title: "Test",
      root: () => frame("Modal", h(Modal, { dialog: h(Dialog, { title: "Sure?", buttons: ["Yes", "No"] }), row: 0 }, h(Button, { label: "under", onPress: () => {} }))),
    },
  ]);
  seen.join(1, PAGE);
  // Each answer's band, its New Background, its ink's cell and its label, on the dialog's buttons' row: none on the button under it, on the body's first row.
  const [yes, no, ...more] = controlsOf(seen, PAGE);
  assert.deepEqual(more, []);
  assert.ok(yes !== undefined && no !== undefined && yes.row > BODY_ROW);
  assert.deepEqual([yes.width, no.width, no.row], ["Yes".length + 2, "No".length + 2, yes.row]);
});

test("a pointer area's triplet is the control's shape in column mode 01010, and one of width 0, not joined, is the whole page", () => {
  const MODE = 0b01010;
  assert.equal(pointerAreaTriplet(1, 38, false), 1 | (MODE << 6) | (38 << 11));
  assert.equal(pointerAreaTriplet(1, 38, false), 0x13281);
  assert.equal(pointerAreaTriplet(1, 38, true), 1 | (MODE << 6) | ((0x40 | 38) << 11), "D6 joins it to the area above");
  assert.equal(pointerAreaTriplet(1, 38, true), 0x33281);
  assert.equal(WHOLE_PAGE_POINTER_AREA, 0 | (MODE << 6) | 0);
  assert.equal(WHOLE_PAGE_POINTER_AREA, 0x00280);
});

test("the whole page's area comes first; each row's controls and areas follow its Set Active Position left to right, joined below; read back as the engine reads them", () => {
  const controls = [{ row: 4, column: 20, width: 10, height: 1 }];
  const pointer = {
    areas: [
      { row: 4, column: 2, width: 6, height: 2 },
      { row: 4, column: 2, width: 6, height: 1 },
    ],
    wholePage: true,
  };
  assert.deepEqual(controlTriplets(controls, pointer), [WHOLE_PAGE_POINTER_AREA, setActivePosition(4), 0x03282, 0x03282, 0x05154, setActivePosition(5), 0x23282, TERMINATION]);
  assert.deepEqual(controlTriplet(20, 10, false), 0x05154);
  assert.deepEqual(readDeclarations(`PN,15000\n${controlLines(controls, pointer).join("\n")}\n`), [{ controls, pointer }], "areas may overlap each other and the controls");
  assert.deepEqual(controlLines([], { areas: [], wholePage: false }), [], "nothing, no packet");
});

test("a page keeps eight pointer areas, overlapping as they may, cut to rows 1 to 23 and its columns; the controls take the local data first", () => {
  const areas = Array.from({ length: 10 }, (_, index) => ({ row: 5, column: index, width: 4, height: 1 }));
  assert.equal(keptDeclarations([], { areas, wholePage: false }).pointer.areas.length, MAX_POINTER_AREAS);
  assert.deepEqual(
    keptDeclarations([], {
      areas: [
        { row: 0, column: 0, width: 5, height: 2 },
        { row: 6, column: 38, width: 5, height: 1 },
        { row: 7, column: 0, width: 0, height: 1 },
        { row: 24, column: 0, width: 5, height: 1 },
      ],
      wholePage: false,
    }).pointer.areas,
    [
      { row: 1, column: 0, width: 5, height: 1 },
      { row: 6, column: 38, width: 2, height: 1 },
    ],
  );
  // 32 controls six rows tall take 192 triplets, their rows' Set Active Positions 6 and the Termination Marker 1: 199 of 208.
  const tall = Array.from({ length: MAX_CONTROLS }, (_, index) => ({ row: 1, column: index, width: 1, height: 6 }));
  const held = keptDeclarations(tall, {
    areas: [
      { row: 1, column: 0, width: 40, height: 5 },
      { row: 1, column: 0, width: 40, height: 3 },
      { row: 1, column: 0, width: 40, height: 1 },
    ],
    wholePage: true,
  });
  assert.equal(held.controls.length, MAX_CONTROLS);
  assert.equal(held.pointer.wholePage, true, "the whole page's triplet: 200");
  assert.deepEqual(
    held.pointer.areas.map((area) => area.height),
    [5, 3],
    "205, then 208; the third has no room",
  );
  assert.equal(enhancementLines(controlTriplets(held.controls, held.pointer)).length, 16, "every packet X/26 a page has");
});

/** A component that declares a pointer area, or the whole page, and keeps what reaches it. */
class Watcher extends Component<{ readonly whole?: boolean; readonly seen: string[] }> {
  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(4, space.height) };
  }

  override paint(canvas: Canvas): void {
    if (this.props.whole === true) {
      canvas.wholePagePointerArea();
    } else {
      canvas.pointerArea(1, 2, 5, 2);
    }
  }

  override onPointer(event: PointerEvent): boolean {
    this.props.seen.push(`${event.action} ${event.buttons} ${event.local.row},${event.local.column}`);
    return true;
  }
}

test("a pointer area is in its component's own cells, joined for its height; a move with no button held in it reaches the component, and nowhere else does", () => {
  const seen: string[] = [];
  const page = viewer(h(Watcher, { seen }));
  assert.deepEqual(areasOf(page, PAGE), { areas: [{ row: BODY_ROW + 1, column: 2, width: 5, height: 2 }], wholePage: false });
  assert.deepEqual(controlsOf(page, PAGE), []);
  page.move(1, 0, 3, BODY_ROW + 1);
  page.move(1, 0, 10, BODY_ROW + 1);
  page.move(1, 0, 3, BODY_ROW + 3);
  assert.deepEqual(seen, ["move 0 1,3"], "the area's cell; not the box's cells beside it or below it");
});

test("a whole-page pointer area is one triplet, and takes every move on the page wherever its component is drawn", () => {
  const seen: string[] = [];
  const page = viewer(h(Watcher, { whole: true, seen }), h(Text, { text: "below" }));
  assert.deepEqual(areasOf(page, PAGE), { areas: [], wholePage: true });
  page.move(1, 0, 30, 20);
  assert.deepEqual(seen, [`move 0 ${20 - BODY_ROW},30`], "in its box's coordinates, outside it");
});

test("only the first sub-page declares controls: a click is found on its cells alone, so a hand on another would point at the first's component", () => {
  class Two extends Component {
    override render(): Child {
      return [h(Frame, { key: "one", title: "One", background: Colour.Blue, prompts: PROMPTS }, h(Button, { label: "one", onPress: () => {} })), h(Frame, { key: "two", title: "Two", background: Colour.Blue, prompts: PROMPTS }, h(Button, { label: "two", onPress: () => {} }))];
    }
  }
  const seen = harness([{ page: PAGE, title: "Test", root: () => h(Two, {}) }]);
  seen.join(1, PAGE);
  const [first, second, ...more] = readControls(latestTti(seen, PAGE));
  assert.deepEqual(more, []);
  assert.deepEqual(first, [{ row: BODY_ROW, column: 1, width: Button.widthOf("one") - 2, height: 1 }]);
  assert.deepEqual(second, []);
});

test("a click acts only on a control's cells: past a button's band, on a switch's label, past a box's text, past a link's label, nothing", () => {
  const pressed: string[] = [];
  const ticks: (readonly number[])[] = [];
  const page = viewer(
    h(Button, { label: "press", onPress: () => pressed.push("press") }),
    h(Switch, { label: "Sound", onChange: (on: boolean) => pressed.push(`switch ${String(on)}`) }),
    h(CheckboxGroup, { items: ["a", "b"], onChange: (value: readonly number[]) => ticks.push(value) }),
    h(PageLink, { page: 0x151, label: "Next" }),
  );
  const [button, chip, boxA, boxB, link, ...more] = inOrder(controlsOf(page, PAGE));
  assert.deepEqual(more, []);
  assert.deepEqual(button, { row: BODY_ROW, column: 1, width: Button.widthOf("press") - 2, height: 1 });
  assert.deepEqual(chip, { row: BODY_ROW + 1, column: 25, width: " OFF".length + 2, height: 1 }, "the chip's band, after the label's 24 cells");
  assert.deepEqual([boxA?.row, boxB?.row, link?.row], [BODY_ROW + 2, BODY_ROW + 3, BODY_ROW + 4]);
  page.clickLeft(1, 20, BODY_ROW);
  page.clickLeft(1, 5, BODY_ROW + 1);
  page.clickLeft(1, 30, BODY_ROW + 2);
  page.clickLeft(1, 20, BODY_ROW + 4);
  assert.deepEqual([pressed, ticks, page.gotos(1)], [[], [], []]);
  page.clickLeft(1, 5, BODY_ROW);
  page.clickLeft(1, 27, BODY_ROW + 1);
  page.clickLeft(1, 3, BODY_ROW + 2);
  page.clickLeft(1, 3, BODY_ROW + 4);
  assert.deepEqual([pressed, ticks, page.gotos(1)], [["press", "switch true"], [[0]], [0x151]]);
});

test("a dialog's button answers only on its band, not on the dialog's background after it", () => {
  const answers: (number | null)[] = [];
  const page = viewer(h(Dialog, { title: "Sure?", buttons: ["Yes", "No"], onAnswer: (button: number | null) => answers.push(button) }));
  const [yes] = controlsOf(page, PAGE);
  assert.ok(yes !== undefined);
  page.clickLeft(1, yes.column + yes.width, yes.row);
  assert.deepEqual(answers, []);
  page.clickLeft(1, yes.column, yes.row);
  assert.deepEqual(answers, [0]);
});

test("a layer placed with At covers only what it places: the button under it keeps its control and its click", () => {
  const pressed: string[] = [];
  const page = viewer(h(Layers, {}, h(Button, { label: "under", onPress: () => pressed.push("under") }), h(At, { row: 5, column: 0 }, h(Text, { text: "over" }))));
  assert.deepEqual(controlsOf(page, PAGE), [{ row: BODY_ROW, column: 1, width: Button.widthOf("under") - 2, height: 1 }]);
  page.clickLeft(1, 3, BODY_ROW);
  assert.deepEqual(pressed, ["under"]);
});

test("a modal keeps every click from the page under it, beside the dialog too, declares nothing of it, and keeps the keys in the dialog", () => {
  const pressed: string[] = [];
  const answers: (number | null)[] = [];
  const seen: string[] = [];
  const page = viewer(
    h(
      Modal,
      { dialog: h(Dialog, { title: "Sure?", buttons: ["OK"], onAnswer: (button: number | null) => answers.push(button) }), row: 0 },
      h(Watcher, { seen }),
      h(Gap, { rows: 8 }),
      h(Button, { label: "under", onPress: () => pressed.push("under") }),
    ),
  );
  const under = BODY_ROW + 4 + 8;
  const [ok, ...more] = controlsOf(page, PAGE);
  assert.deepEqual(more, [], "the dialog's button alone");
  assert.ok(ok !== undefined && ok.row < under);
  assert.deepEqual(areasOf(page, PAGE), { areas: [], wholePage: false }, "the pointer area under it is not declared");
  page.clickLeft(1, 3, under);
  page.press(1, Key.Enter);
  assert.deepEqual([pressed, answers], [[], [0]]);
});

test("a calendar's controls are its arrows and the chosen week's days; a click on any day chooses it and the controls follow its week", () => {
  const chosen: CalendarDate[] = [];
  const page = viewer(h(Calendar, { year: 2026, month: 9, selectable: true, onChoose: (date: CalendarDate) => chosen.push(date) }));
  // The heading, "← October 2026 →", centred in the seven days' 28 cells after its colour's cell; 1 October 2026 is a Thursday.
  const heading = `← ${formatDate(Date.UTC(2026, 9, 1), DATE_FORMATS.month)} →`;
  const at = Math.floor((28 - heading.length) / 2) + 1;
  const arrows = [
    { row: BODY_ROW, column: at, width: 2, height: 1 },
    { row: BODY_ROW, column: at + heading.length - 2, width: 2, height: 1 },
  ];
  const week = (row: number, from: number): { row: number; column: number; width: number; height: number }[] => Array.from({ length: 7 - from }, (_, day) => ({ row, column: 4 * (from + day) + 1, width: 3, height: 1 }));
  assert.deepEqual(inOrder(controlsOf(page, PAGE)), [...arrows, ...week(BODY_ROW + 2, 3)]);
  // 15 October is the third week's Thursday.
  page.clickLeft(1, 4 * 3 + 2, BODY_ROW + 4);
  assert.deepEqual(chosen, [{ year: 2026, month: 9, day: 15 }]);
  assert.deepEqual(inOrder(controlsOf(page, PAGE)), [...arrows, ...week(BODY_ROW + 4, 0)]);
  page.clickLeft(1, at, BODY_ROW);
  assert.match(text(page.seen(1, PAGE).rows, BODY_ROW), /September 2026/u, "the left arrow turns back a month");
});

test("a slider's track is its control; a scroll bar that scrolls is one, the whole bar; disabled, neither, and the wheel turns nothing", () => {
  const items = Array.from({ length: 10 }, (_, index) => `item ${index + 1}`);
  const page = viewer(h(Slider, { label: "Volume", min: 0, max: 10 }), h(ChoiceList, { items, shown: 4, onChoose: () => {} }));
  const track = COLUMNS - 12 - 1 - 6;
  const rows = Array.from({ length: 4 }, (_, row) => ({ row: BODY_ROW + 1 + row, column: 1, width: COLUMNS - BAR_CELLS - 2, height: 1 }));
  assert.deepEqual(inOrder(controlsOf(page, PAGE)), inOrder([{ row: BODY_ROW, column: 13, width: track, height: 1 }, ...rows, { row: BODY_ROW + 1, column: COLUMNS - BAR_CELLS, width: BAR_CELLS, height: 4 }]));
  const off = viewer(h(Slider, { label: "Volume", min: 0, max: 10, disabled: true }), h(ChoiceList, { items, shown: 4, disabled: true, onChoose: () => {} }));
  assert.deepEqual(controlsOf(off, PAGE), []);
  off.wheel(1, 120, 5, BODY_ROW + 1);
  off.clickLeft(1, 39, BODY_ROW + 4);
  assert.match(text(off.seen(1, PAGE).rows, BODY_ROW + 1), /item 1 /u);
});

test("a disabled select does not open to a click, and a disabled table does not sort", () => {
  const page = viewer(h(Select, { label: "Pick", items: ["one", "two"], disabled: true }), h(DataTable, { columns: [{ title: "Name", width: 10 }], rows: [["b"], ["a"]], shown: 2, sortable: true, disabled: true }));
  assert.deepEqual(controlsOf(page, PAGE), []);
  page.clickLeft(1, 16, BODY_ROW);
  page.clickLeft(1, 4, BODY_ROW + 1);
  const rows = page.seen(1, PAGE).rows;
  assert.match(text(rows, BODY_ROW + 1), /Name/u, "the table's heading straight under the select: no list opened");
  assert.match(text(rows, BODY_ROW + 2), /b/u, "unsorted");
});

test("a paint canvas with a brush is a pointer area over its surface, its mark on the cell under the pointer; without, it declares none; disabled, it paints nothing", () => {
  const EMPTY_CELL = 0x20;
  const MARK = 0x2c;
  const page = viewer(h(PaintCanvas, { columns: 10, rows: 4, brush: true }));
  assert.deepEqual(areasOf(page, PAGE), { areas: [{ row: BODY_ROW, column: 1, width: 9, height: 4 }], wholePage: false });
  assert.deepEqual(controlsOf(page, PAGE), [], "the surface is no control");
  page.move(1, 0, 5, BODY_ROW + 1);
  assert.equal(page.seen(1, PAGE).rows[BODY_ROW + 1]?.[5], MARK, "the middle row of an empty cell");
  page.move(1, 0, 6, BODY_ROW + 1);
  assert.deepEqual([page.seen(1, PAGE).rows[BODY_ROW + 1]?.[5], page.seen(1, PAGE).rows[BODY_ROW + 1]?.[6]], [EMPTY_CELL, MARK]);
  const plain = viewer(h(PaintCanvas, { columns: 10, rows: 4 }));
  assert.deepEqual(areasOf(plain, PAGE), { areas: [], wholePage: false });
  const off = viewer(h(PaintCanvas, { columns: 10, rows: 4, disabled: true }));
  off.clickLeft(1, 5, BODY_ROW + 1);
  assert.equal(off.seen(1, PAGE).rows[BODY_ROW + 1]?.[5], EMPTY_CELL);
});

test("a list's row is pressed only on its band: a click under a select's label, on the row's indent, takes nothing", () => {
  const changes: number[] = [];
  const page = viewer(h(Select, { label: "Pick", items: ["one", "two", "three"], onChange: (value: number) => changes.push(value) }));
  page.press(1, Key.Enter);
  const rows = controlsOf(page, PAGE).filter((control) => control.row > BODY_ROW);
  assert.deepEqual(rows[0], { row: BODY_ROW + 1, column: 13, width: COLUMNS - 13, height: 1 }, "after the label's 12 cells, from New Background to the row's end");
  page.clickLeft(1, 3, BODY_ROW + 2);
  assert.deepEqual(changes, []);
  page.clickLeft(1, 15, BODY_ROW + 2);
  assert.deepEqual(changes, [1]);
});

/** Hears every hover that comes up from the components under it. */
class HoverWatcher extends Component<{ readonly seen: string[] }> {
  override onHover(event: HoverEvent): boolean {
    this.props.seen.push(`${event.entered ? "enter" : "leave"} ${event.row},${event.column}`);
    return false;
  }
}

test("a hover goes to the component whose control begins at its cell, and up; the hint row says what the hovered control does while the pointer is on it", () => {
  const seen: string[] = [];
  const page = viewer(h(HoverWatcher, { seen }, h(Button, { label: "one", hint: "one is pressed", onPress: () => {} }), h(Button, { label: "two", hint: "two is pressed", onPress: () => {} })));
  const [one, two, ...more] = inOrder(controlsOf(page, PAGE));
  assert.ok(one !== undefined && two !== undefined && more.length === 0, "two controls");
  assert.match(text(page.seen(1, PAGE).rows, 23), /one is pressed/u, "the first button has the keys");
  page.hover(1, "enter", two.column, two.row);
  assert.match(text(page.seen(1, PAGE).rows, 23), /two is pressed/u, "the hovered button says what it does");
  // Moving from one control to the next is a leave, then an enter (INPUT.md section 5.4).
  page.hover(1, "leave", two.column, two.row);
  page.hover(1, "enter", one.column, one.row);
  page.hover(1, "leave", one.column, one.row);
  assert.match(text(page.seen(1, PAGE).rows, 23), /one is pressed/u, "off every control, the focused one's hint again");
  assert.deepEqual(seen, [`enter ${two.row},${two.column}`, `leave ${two.row},${two.column}`, `enter ${one.row},${one.column}`, `leave ${one.row},${one.column}`]);
});

test("a hover where no control begins is dropped, as is a leave with no enter; an enter with a leave missed leaves the control before first", () => {
  const seen: string[] = [];
  const page = viewer(h(HoverWatcher, { seen }, h(Button, { label: "one", onPress: () => {} }), h(Button, { label: "two", onPress: () => {} })));
  const [one, two] = inOrder(controlsOf(page, PAGE));
  assert.ok(one !== undefined && two !== undefined);
  page.hover(1, "enter", one.column + 1, one.row);
  page.hover(1, "leave", one.column, one.row);
  assert.deepEqual(seen, [], "inside a control is not its first cell, and nothing was entered to leave");
  page.hover(1, "enter", one.column, one.row);
  page.hover(1, "enter", one.column, one.row);
  page.hover(1, "enter", two.column, two.row);
  assert.deepEqual(seen, [`enter ${one.row},${one.column}`, `leave ${one.row},${one.column}`, `enter ${two.row},${two.column}`], "an enter again says nothing new");
});
