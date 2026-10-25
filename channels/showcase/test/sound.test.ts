import assert from "node:assert/strict";
import { test } from "node:test";

import type { Application, Context } from "../../../kits/typescript/src/kit.ts";
import { Harness } from "../../../kits/typescript/src/testing.ts";
import { Effect, NOTES, PEAK, RATE, Sounds, handle, library, samples, samplesIn } from "../src/sound.ts";

test("every sound has its handle: the notes from 1, the effects from 20", () => {
  assert.deepEqual(
    library().map(handle),
    [20, 21, 22, 23, 24, 25, 26, 27, 28, ...Array.from({ length: NOTES }, (_, note) => 1 + note)],
  );
  assert.equal(handle({ note: 99 }), NOTES, "a note beyond the octave is its last");
});

test("each sound is as long as its parts, within the peak, and fades out", () => {
  const lengths: [Effect, number][] = [
    [Effect.Laser, samplesIn(160)],
    [Effect.Explosion, samplesIn(450)],
    [Effect.Hit, samplesIn(60)],
    [Effect.Coin, samplesIn(70) + samplesIn(140)],
    [Effect.Jump, samplesIn(150)],
    [Effect.Win, 4 * samplesIn(110)],
    [Effect.Lose, samplesIn(500)],
    [Effect.Click, samplesIn(12)],
    [Effect.Bell, samplesIn(500)],
    [{ note: 0 }, samplesIn(400)],
  ];
  for (const [effect, length] of lengths) {
    const sound = samples(effect);
    assert.equal(sound.length, length, JSON.stringify(effect));
    assert.ok(sound.every((sample) => Math.abs(sample) <= PEAK), JSON.stringify(effect));
    assert.ok(Math.abs(sound[sound.length - 1] ?? PEAK) < PEAK / 10, `${JSON.stringify(effect)} fades out`);
  }
  assert.equal(samplesIn(1_000), RATE);
  // A square wave starts at its peak, at full envelope.
  assert.equal(samples(Effect.Click)[0], PEAK);
  // A triangle starts at its trough.
  assert.equal(samples(Effect.Bell)[0], -PEAK);
});

test("the notes of the octave rise", () => {
  // The first sample where each note's triangle first rises past zero comes sooner for a higher note.
  const firstCrossing = (note: number): number => samples({ note }).findIndex((sample) => sample > 0);
  for (let note = 1; note < NOTES; note += 1) {
    assert.ok(firstCrossing(note) <= firstCrossing(note - 1), `note ${note}`);
  }
  assert.ok(firstCrossing(NOTES - 1) < firstCrossing(0));
});

/** An application that plays what it is told on each text input. */
function player(sounds: Sounds): Application {
  return {
    pages: () => [{ page: 0x100, title: "Sounds" }],
    start: () => {},
    input: (session, _page, input, context: Context) => {
      if (input.kind === "text") {
        sounds.play(session, Effect.Coin, context);
      }
    },
    ended: (session) => sounds.forget(session),
  };
}

test("a session is sent the library once, then a command for each sound, the channels rotating", () => {
  const harness = new Harness([player(new Sounds())]);
  harness.join(1, 0x100);
  harness.typeText(1, "a");
  harness.typeText(1, "b");
  harness.typeText(1, "c");
  harness.typeText(1, "d");
  const sounds = harness.sounds(1);
  const objects = sounds.filter((message) => "object" in message);
  assert.equal(objects.length, library().length, "the whole library, once");
  assert.ok(objects.every((message) => (message["object"] as Record<string, unknown>)["rate"] === RATE));
  const plays = sounds.filter((message) => "play" in message).map((message) => message["play"]);
  assert.deepEqual(plays, [
    { channel: 1, handle: 23, generation: 1, gain: 80, fade_ms: 5 },
    { channel: 2, handle: 23, generation: 1, gain: 80, fade_ms: 5 },
    { channel: 0, handle: 23, generation: 1, gain: 80, fade_ms: 5 },
    { channel: 1, handle: 23, generation: 1, gain: 80, fade_ms: 5 },
  ]);
  harness.close(1);
  harness.join(1, 0x100);
  harness.typeText(1, "e");
  assert.equal(harness.sounds(1).filter((message) => "object" in message).length, 2 * library().length, "a new session is sent it again");
});
