// crates/teletext-showcase/src/audio/tests.rs, ported: the sound area
// driven as the server drives it, and the sound lines it writes read back.
// The Rust tests read the encoded records a receiver gets; here the
// program writes PCM and the server encodes it, so the tests read the
// lines' members and samples.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Key } from "../../../kits/typescript/src/kit.ts";
import { Harness, type Sent, shows } from "../../../kits/typescript/src/testing.ts";
import { audio } from "../src/audio.ts";
import { MENU, RADIO } from "../src/audio/pages.ts";
import { BLOCK_SAMPLES, CHANNEL, HANDLE, LEAD_MS, TICK_MS, sampleAt } from "../src/audio/radio.ts";
import { RATE } from "../src/audio/synth.ts";
import { Tracker } from "../src/audio/tracker.ts";

type Fields = Readonly<Record<string, unknown>>;

function area(): Harness {
  return new Harness(audio.applications([]));
}

/** The sound lines written from line `since` on, for `session` alone or for everyone with `page` open. */
function soundLines(harness: Harness, to: { readonly session: number } | { readonly page: number }, since = 0): Sent[] {
  return harness.sent.slice(since).filter((message) => {
    const audience = message["to"] as Fields | undefined;
    if (message["type"] !== "sound" || audience === undefined) {
      return false;
    }
    return "session" in to ? audience["session"] === to.session : audience["page"] === to.page.toString(16);
  });
}

/** A sound line's member's name and fields. */
function member(message: Sent): [string, Fields] {
  for (const name of ["object", "live", "blocks", "play", "stop", "gain"]) {
    const fields = message[name];
    if (fields !== undefined) {
      return [name, fields as Fields];
    }
  }
  throw new Error(`no member in ${JSON.stringify(message)}`);
}

/** The members' names. */
function names(messages: readonly Sent[]): string[] {
  return messages.map((message) => member(message)[0]);
}

/** A member's samples, from its base64 of 16-bit little-endian PCM. */
function samplesOf(fields: Fields): Int16Array {
  const bytes = Buffer.from(String(fields["pcm"]), "base64");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return Int16Array.from({ length: bytes.byteLength / Int16Array.BYTES_PER_ELEMENT }, (_, index) =>
    view.getInt16(index * Int16Array.BYTES_PER_ELEMENT, true),
  );
}

/** The `blocks` members among `messages`. */
function blocks(messages: readonly Sent[]): Fields[] {
  return messages.map(member).flatMap(([name, fields]) => (name === "blocks" ? [fields] : []));
}

/** Whole blocks due in the lead from a start. */
function blocksInLead(): number {
  return Math.floor(sampleAt(LEAD_MS) / BLOCK_SAMPLES);
}

// Opening the radio sends that session, and only it, the live object, a
// play bound to the page on channel 3, then the lead's blocks to the
// page's viewers.
test("a listener is sent the live object, a play, and blocks", () => {
  const harness = area();
  harness.join(2, MENU);
  harness.join(1, RADIO);
  const own = soundLines(harness, { session: 1 });
  assert.deepEqual(
    own.map(member),
    [
      ["live", { handle: HANDLE, generation: 1, rate: RATE }],
      ["play", { channel: CHANNEL, handle: HANDLE, generation: 1, gain: 85, fade_ms: 20, page: "501" }],
    ],
  );
  const stream = blocks(soundLines(harness, { page: RADIO }));
  assert.equal(stream.length, 1, "one message for the tick's blocks");
  const [first] = stream;
  assert.ok(first !== undefined);
  assert.deepEqual([first["handle"], first["generation"], first["rate"], first["first"]], [HANDLE, 1, RATE, 0]);
  assert.equal(samplesOf(first).length, blocksInLead() * BLOCK_SAMPLES);
  assert.deepEqual(soundLines(harness, { session: 2 }), [], "the menu's viewer hears nothing");
  assert.equal(harness.channel.nextWake(), TICK_MS, "the stream tops up on the tick");
  assert.deepEqual(harness.errors, []);
});

// Two listeners: the second joins the same generation with the blocks not
// yet heard, and from then on both get the same lines.
test("two listeners get the same blocks", () => {
  const harness = area();
  harness.join(1, RADIO);
  const [stream] = blocks(soundLines(harness, { page: RADIO }));
  assert.ok(stream !== undefined);
  const first = samplesOf(stream);
  harness.advance(100);
  const since = harness.sent.length;
  harness.join(2, RADIO);
  const joined = soundLines(harness, { session: 2 }, since);
  assert.deepEqual(names(joined), ["live", "play", "blocks"]);
  assert.deepEqual(joined.map(member).map(([, fields]) => fields["generation"]), [1, 1, 1]);
  assert.deepEqual(soundLines(harness, { session: 1 }, since), [], "the first listener is sent nothing again");
  assert.deepEqual(soundLines(harness, { page: RADIO }, since), [], "nor the page");
  const [caughtUp] = blocks(joined);
  assert.ok(caughtUp !== undefined);
  const caught = samplesOf(caughtUp);
  assert.ok(caught.length > 0 && caught.length < first.length, `${caught.length} of ${first.length}`);
  assert.equal(caught.length % BLOCK_SAMPLES, 0, "whole blocks");
  assert.equal(caughtUp["first"], (first.length - caught.length) / BLOCK_SAMPLES, "numbered as the stream numbered them");
  assert.deepEqual(caught, first.subarray(first.length - caught.length), "the newest blocks, the same samples");
  for (let tick = 1; tick <= 8; tick += 1) {
    const before = harness.sent.length;
    harness.runUntil(tick * TICK_MS);
    assert.equal(blocks(soundLines(harness, { page: RADIO }, before)).length, 1, `tick ${tick}: one stream for both`);
    assert.deepEqual([...soundLines(harness, { session: 1 }, before), ...soundLines(harness, { session: 2 }, before)], [], `tick ${tick}`);
  }
  assert.deepEqual(harness.errors, []);
});

// Leaving the page stops the radio for that listener; with nobody left
// nothing is made, and no wake is asked for.
test("leaving stops, and nobody listening makes nothing", () => {
  const harness = area();
  assert.equal(harness.channel.nextWake(), null, "off air at the start");
  harness.join(1, RADIO);
  harness.advance(100);
  harness.sharedPublications(RADIO);
  const since = harness.sent.length;
  harness.open(1, MENU);
  assert.deepEqual(soundLines(harness, { session: 1 }, since).map(member), [["stop", { channel: CHANNEL, fade_ms: 20 }]]);
  assert.equal(harness.sharedPublications(RADIO), 1, "the page goes off air");
  assert.ok(shows(harness.shared(RADIO), "OFF AIR"));
  const quiet = harness.sent.length;
  for (let now = TICK_MS; now <= 2_000; now += 100) {
    harness.runUntil(now);
  }
  assert.deepEqual(harness.sent.slice(quiet), [], "nothing made or sent");
  assert.equal(harness.channel.nextWake(), null);
});

// A listener whose connection ends is gone; the next listener starts a new
// generation.
test("a closed listener stops the stream", () => {
  const harness = area();
  harness.join(1, RADIO);
  harness.advance(50);
  harness.close(1);
  const since = harness.sent.length;
  harness.runUntil(TICK_MS);
  assert.deepEqual(harness.sent.slice(since), []);
  harness.runUntil(1_000);
  harness.join(2, RADIO);
  const [live] = soundLines(harness, { session: 2 });
  assert.ok(live !== undefined);
  assert.deepEqual(member(live), ["live", { handle: HANDLE, generation: 2, rate: RATE }]);
});

// The Rust test plays what a listener is sent through the engine's own
// receiver. Here the server encodes and the engine plays, so what is
// checked is what the program sends: the tracker's samples from the moment
// the page opened, in whole numbered blocks, none lost, none late, always
// a lead ahead of real time.
test("the stream reaches a listener whole, in order and ahead of time", () => {
  const harness = area();
  const openedAt = 1_000;
  harness.runUntil(openedAt);
  harness.join(1, RADIO);
  const sent: Int16Array[] = [];
  let read = 0;
  let next = 0;
  let total = 0;
  for (let now = openedAt; now <= openedAt + 2_000; now += TICK_MS) {
    harness.runUntil(now);
    for (const fields of blocks(soundLines(harness, { page: RADIO }, read))) {
      const samples = samplesOf(fields);
      assert.equal(fields["first"], next, `at ${now}: the next block`);
      assert.equal(samples.length % BLOCK_SAMPLES, 0, "whole blocks");
      next += samples.length / BLOCK_SAMPLES;
      total += samples.length;
      sent.push(samples);
    }
    read = harness.sent.length;
    const ahead = sampleAt(openedAt) + total - sampleAt(now);
    assert.ok(ahead > sampleAt(LEAD_MS) - BLOCK_SAMPLES, `at ${now}: ${ahead} samples ahead`);
  }
  assert.ok(total >= sampleAt(2_000), `${total} samples`);
  const stream = new Int16Array(total);
  let offset = 0;
  for (const samples of sent) {
    stream.set(samples, offset);
    offset += samples.length;
  }
  assert.deepEqual(stream, new Tracker().render(sampleAt(openedAt), total));
  assert.deepEqual(harness.errors, []);
});

// Not in the Rust showcase: the menu and the radio use neither digits nor
// the coloured keys, so they select pages (INPUT.md section 9.4).
test("digits and the coloured keys select pages from the menu and the radio", () => {
  const harness = area();
  harness.join(1, MENU);
  harness.typeText(1, "501");
  assert.deepEqual(harness.gotos(1), [RADIO], "the menu takes no digits");
  harness.open(1, RADIO);
  harness.typeText(1, "500");
  harness.press(1, Key.F1);
  assert.deepEqual(harness.gotos(1), [RADIO, MENU, MENU], "nor does the radio, whose red link is Sound");
  assert.deepEqual(harness.errors, []);
});
