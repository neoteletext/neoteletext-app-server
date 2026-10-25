import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { test } from "node:test";

import { App, LONGEST_LINE, LineSplitter, type ServerEvents, readMessage, serveLines } from "../src/app.ts";
import { Colour, Screen } from "../src/screen.ts";

test("each of the server's messages is read whole", () => {
  assert.deepEqual(readMessage('{"type":"start","protocol":1,"epoch_ms":1792886400000}'), { type: "start", protocol: 1, epochMs: 1_792_886_400_000 });
  assert.deepEqual(readMessage('{"type":"join","session":7}'), { type: "join", session: 7 });
  assert.deepEqual(readMessage('{"type":"open","session":7,"page":"1A0"}'), { type: "open", session: 7, page: 0x1a0 });
  assert.deepEqual(readMessage('{"type":"leave","session":7,"page":"100"}'), { type: "leave", session: 7, page: 0x100 });
  assert.deepEqual(readMessage('{"type":"input","session":7,"page":"8FF","event":{"kind":"text","text":"hi"}}'), {
    type: "input",
    session: 7,
    page: 0x8ff,
    input: { kind: "text", text: "hi" },
  });
  assert.deepEqual(readMessage('{"type":"close","session":0}'), { type: "close", session: 0 });
});

test("a line that is not a whole, valid message is refused with the reason", () => {
  for (const bad of [
    "",
    "not json",
    "[]",
    '"start"',
    '{"type":"start","protocol":1}',
    '{"type":"join"}',
    '{"type":"join","session":-1}',
    '{"type":"join","session":1.5}',
    '{"type":"join","session":"7"}',
    '{"type":"join","session":9007199254740993}',
    '{"type":"open","session":1,"page":"900"}',
    '{"type":"open","session":1,"page":"0FF"}',
    '{"type":"open","session":1,"page":"1a0"}',
    '{"type":"open","session":1,"page":256}',
    '{"type":"input","session":1,"page":"100","event":{"kind":"key"}}',
    '{"type":"tick","session":1}',
    "{}",
  ]) {
    assert.equal(typeof readMessage(bad), "string", bad);
  }
});

function written(): { app: App; lines: Record<string, unknown>[] } {
  const lines: Record<string, unknown>[] = [];
  return { app: new App((line) => lines.push(JSON.parse(line) as Record<string, unknown>)), lines };
}

test("the first line declares the pages, once, as the server reads them", () => {
  const { app, lines } = written();
  app.declare([
    { page: 0x100, title: "Index" },
    { page: 0x1a0, title: "Hexadecimal" },
  ]);
  assert.deepEqual(lines, [{ type: "pages", pages: [{ page: "100", title: "Index" }, { page: "1A0", title: "Hexadecimal" }] }]);
  assert.throws(() => app.declare([]), /once/);
  assert.ok(app.serves(0x1a0) && !app.serves(0x101));
});

test("pages the server would refuse are refused at the declaration", () => {
  for (const pages of [
    [{ page: 0x900, title: "" }],
    [{ page: 0x0ff, title: "" }],
    [
      { page: 0x100, title: "" },
      { page: 0x100, title: "" },
    ],
    [{ page: 0x100, title: "x".repeat(33) }],
    [{ page: 0x100, title: "tab\there" }],
    Array.from({ length: 801 }, (_, index) => ({ page: 0x100 + index, title: "" })),
  ]) {
    assert.throws(() => written().app.declare(pages), RangeError);
  }
  const titled = written();
  titled.app.declare([{ page: 0x100, title: "é".repeat(32) }]);
  assert.equal(titled.lines.length, 1, "32 characters, however many bytes");
});

test("publish writes TTI for the declared page and owner", () => {
  const { app, lines } = written();
  app.declare([{ page: 0x101, title: "News" }]);
  const screen = new Screen();
  screen.coloured(1, 0, Colour.Red, "Hi");
  app.publish("channel", 0x101, screen);
  app.publish(7, 0x101, [
    { screen, subpage: 1 },
    { screen, subpage: 2 },
  ]);
  const [, shared, own] = lines;
  assert.equal(shared?.["owner"], "channel");
  assert.equal(shared?.["page"], "101");
  assert.ok(String(shared?.["tti"]).startsWith("PN,10100\nSC,0000\nOL,0,\nOL,1,\u001bAHi\n"));
  assert.equal(own?.["owner"], 7);
  assert.match(String(own?.["tti"]), /^PN,10101\n[^]*PN,10102\n/u);
  assert.throws(() => app.publish("channel", 0x102, screen), /not declared/);
  assert.throws(() => app.publish(-1, 0x101, screen), RangeError);
  assert.throws(() => app.publish("channel", 0x101, []), RangeError);
  assert.throws(() => app.publish("channel", 0x101, Array.from({ length: 65 }, (_, subpage) => ({ screen, subpage }))), RangeError);
  assert.throws(() => app.publish("channel", 0x101, [{ screen, subpage: 100 }]), RangeError);
});

test("an escaped control code survives the JSON line", () => {
  const raw: string[] = [];
  const direct = new App((line) => raw.push(line));
  direct.declare([{ page: 0x100, title: "" }]);
  const screen = new Screen();
  screen.coloured(1, 0, Colour.Green, "x");
  direct.publish("channel", 0x100, screen);
  assert.ok(raw[1]?.includes("\\u001bBx"), "ESC is written as \\u001b, so the line has no raw control character");
});

test("goto, sound and log write their lines", () => {
  const { app, lines } = written();
  app.goto(7, 0x300);
  app.sound({ session: 7 }, { play: { channel: 1, handle: 20, generation: 1 } });
  app.sound({ page: 0x501 }, { stop: { channel: 3 } });
  app.log("fetched 7 posts");
  assert.deepEqual(lines, [
    { type: "goto", session: 7, page: "300" },
    { type: "sound", to: { session: 7 }, play: { channel: 1, handle: 20, generation: 1 } },
    { type: "sound", to: { page: "501" }, stop: { channel: 3 } },
    { type: "log", text: "fetched 7 posts" },
  ]);
  assert.throws(() => app.goto(7, 0x900), RangeError);
  assert.throws(() => app.sound({ page: 0x50 }, { stop: { channel: 0 } }), RangeError);
});

test("lines are split across chunks, a return before the newline dropped", () => {
  const lines: string[] = [];
  const faults: string[] = [];
  const splitter = new LineSplitter(
    (line) => lines.push(line),
    (why) => faults.push(why),
  );
  splitter.push(Buffer.from('{"a":1}\n{"b"'));
  splitter.push(Buffer.from(':2}\r\n\n'));
  splitter.push(Buffer.from([0xe2, 0x82]));
  splitter.push(Buffer.from([0xac, 0x0a]));
  splitter.push(Buffer.from("last"));
  splitter.end();
  assert.deepEqual(lines, ['{"a":1}', '{"b":2}', "€", "last"]);
  assert.deepEqual(faults, []);
});

test("a line too long, or not UTF-8, is dropped whole and reported", () => {
  const lines: string[] = [];
  const faults: string[] = [];
  const splitter = new LineSplitter(
    (line) => lines.push(line),
    (why) => faults.push(why),
  );
  const half = Buffer.alloc(LONGEST_LINE / 2 + 1, 0x61);
  splitter.push(half);
  splitter.push(half);
  splitter.push(Buffer.from("tail\nnext\n"));
  splitter.push(Buffer.from([0xff, 0x0a]));
  assert.deepEqual(lines, ["next"]);
  assert.equal(faults.length, 2);
  assert.match(faults[0] ?? "", /over/);
  assert.match(faults[1] ?? "", /UTF-8/);
});

test("the line loop hands on each valid line, reports each bad one, and ends with its input", async () => {
  const input = new PassThrough();
  const errors: string[] = [];
  const heard: string[] = [];
  const events: ServerEvents = {
    start: (epochMs) => heard.push(`start ${epochMs}`),
    join: (session) => heard.push(`join ${session}`),
    open: (session, page) => heard.push(`open ${session} ${page.toString(16)}`),
    leave: (session, page) => heard.push(`leave ${session} ${page.toString(16)}`),
    input: (session, page, event) => heard.push(`input ${session} ${page.toString(16)} ${event.kind}`),
    close: (session) => heard.push(`close ${session}`),
  };
  const served = serveLines(events, { input, output: () => assert.fail("nothing is written"), errors: (text) => errors.push(text) });
  input.write('{"type":"start","protocol":2,"epoch_ms":5}\n{"type":"join","session":1}\nnonsense\n');
  input.write('{"type":"open","session":1,"page":"100"}\n{"type":"input","session":1,"page":"100","event":{"kind":"focus"}}\n');
  input.end('{"type":"leave","session":1,"page":"100"}\n{"type":"close","session":1}');
  await served;
  assert.deepEqual(heard, ["start 5", "join 1", "open 1 100", "input 1 100 focus", "leave 1 100", "close 1"]);
  assert.equal(errors.length, 2);
  assert.match(errors[0] ?? "", /protocol 2/);
  assert.match(errors[1] ?? "", /not JSON/);
});
