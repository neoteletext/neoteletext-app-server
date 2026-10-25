import assert from "node:assert/strict";
import { test } from "node:test";

import { MOST_SAMPLES, pcmBase64, soundFields } from "../src/sound.ts";

test("samples are base64 of 16-bit little-endian PCM", () => {
  // 1, -2 and 0x1234: 01 00, FE FF, 34 12.
  assert.equal(pcmBase64(Int16Array.from([1, -2, 0x1234])), Buffer.from([0x01, 0x00, 0xfe, 0xff, 0x34, 0x12]).toString("base64"));
  assert.equal(pcmBase64(new Int16Array(0)), "");
});

test("each member is written as the protocol writes it", () => {
  assert.deepEqual(soundFields({ object: { handle: 20, generation: 1, rate: 8_000, samples: Int16Array.from([1]) } }), {
    object: { handle: 20, generation: 1, rate: 8_000, pcm: "AQA=" },
  });
  assert.deepEqual(soundFields({ live: { handle: 30, generation: 2, rate: 22_050 } }), { live: { handle: 30, generation: 2, rate: 22_050 } });
  assert.deepEqual(soundFields({ blocks: { handle: 30, generation: 2, rate: 22_050, first: 7, samples: Int16Array.from([0]) } }), {
    blocks: { handle: 30, generation: 2, rate: 22_050, first: 7, pcm: "AAA=" },
  });
  assert.deepEqual(soundFields({ play: { channel: 1, handle: 20, generation: 1, gain: 80, fadeMs: 5 } }), {
    play: { channel: 1, handle: 20, generation: 1, gain: 80, fade_ms: 5 },
  });
  assert.deepEqual(soundFields({ play: { channel: 3, handle: 9, generation: 0, loop: true, after: false, duck: 6, page: 0x501 } }), {
    play: { channel: 3, handle: 9, generation: 0, loop: true, after: false, duck: 6, page: "501" },
  });
  assert.deepEqual(soundFields({ stop: { channel: 2, fadeMs: 100 } }), { stop: { channel: 2, fade_ms: 100 } });
  assert.deepEqual(soundFields({ gain: { channel: 0, gain: 50 } }), { gain: { channel: 0, gain: 50 } });
});

test("what the server would refuse is refused here, where the mistake is", () => {
  const samples = new Int16Array(1);
  for (const bad of [
    () => soundFields({ object: { handle: 256, generation: 0, rate: 8_000, samples } }),
    () => soundFields({ object: { handle: 1, generation: 70_000, rate: 8_000, samples } }),
    () => soundFields({ object: { handle: 1, generation: 0, rate: 9_000 as 8_000, samples } }),
    () => soundFields({ object: { handle: 1, generation: 0, rate: 8_000, samples: new Int16Array(MOST_SAMPLES + 1) } }),
    () => soundFields({ play: { channel: 4, handle: 1, generation: 0 } }),
    () => soundFields({ play: { channel: 0, handle: 1, generation: 0, gain: 101 } }),
    () => soundFields({ play: { channel: 0, handle: 1, generation: 0, duck: 7 } }),
    () => soundFields({ play: { channel: 0, handle: 1, generation: 0, page: 0x900 } }),
    () => soundFields({ stop: { channel: -1 } }),
    () => soundFields({ gain: { channel: 0, gain: 0.5 } }),
  ]) {
    assert.throws(bad, RangeError);
  }
});
