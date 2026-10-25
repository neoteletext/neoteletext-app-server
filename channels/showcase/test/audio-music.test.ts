// The radio's music, ported from crates/teletext-showcase/src/audio/
// tests.rs: the tracker's synthesis and the tunes it reads.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";

import { RATE } from "../src/audio/synth.ts";
import { MIX_PEAK, ROWS_PER_BAR, Tracker, melodyCell } from "../src/audio/tracker.ts";
import { PLAYLIST } from "../src/audio/tunes.ts";

/** Samples in a second. */
const SECOND = RATE;
/** A span of the stream the Rust test renders at each probe. */
const PROBE_SAMPLES = 4_000;

// The synthesis is the same every time, stays inside 16 bits, loops, and
// is loud enough to hear.
test("synthesis is deterministic and in range", () => {
  const [one, two] = [new Tracker(), new Tracker()];
  const cycle = one.cycle();
  assert.ok(cycle > SECOND * 60, `a playlist of minutes: ${cycle}`);
  let loudest = 0;
  for (let second = 0; second < Math.floor(cycle / SECOND); second += 7) {
    const first = second * SECOND;
    const block = one.render(first, PROBE_SAMPLES);
    assert.deepEqual(block, two.render(first, PROBE_SAMPLES), `second ${second}`);
    for (const sample of block) {
      assert.ok(Math.abs(sample) <= MIX_PEAK, `second ${second}: ${sample}`);
      loudest = Math.max(loudest, Math.abs(sample));
    }
  }
  assert.ok(loudest > MIX_PEAK / 3, `loud enough: ${loudest}`);
  assert.deepEqual(one.render(cycle + 12_345, 500), one.render(12_345, 500), "the playlist loops");
});

/** Seconds between the spans the Rust stream was hashed over. */
const HASHED_EVERY = 13;
/** The SHA-256 of one second of the Rust tracker's samples every 13 s of the playlist, from 0 to 143 s, as 16-bit little-endian PCM (crates/teletext-showcase, 25 October 2026). */
const RUST_HASH = "1c8ca5ddb6802e905a5432e678a15940333ae01b3f2416ab5d7e421fb2e6c56e";

// Not in the Rust: the integer synthesis is the Rust showcase's sample for
// sample, so any arithmetic that drifts from it fails here.
test("the samples are the Rust showcase's", () => {
  const tracker = new Tracker();
  const hash = createHash("sha256");
  for (let second = 0; second * SECOND < tracker.cycle(); second += HASHED_EVERY) {
    const samples = tracker.render(second * SECOND, SECOND);
    hash.update(new Uint8Array(samples.buffer, samples.byteOffset, samples.byteLength));
  }
  assert.equal(hash.digest("hex"), RUST_HASH);
});

/** The longest a tune's title may be: a page entry's title (docs/applications.md). */
const LONGEST_TITLE = 32;
/** The longest style line: row 5 from column 6. */
const LONGEST_STYLE_LINE = 34;

// Every tune reads cleanly: sixteen good tokens a bar, a chord a bar, drum
// bars of sixteen rows, titles that fit the page.
test("the tunes are well formed", () => {
  const tokens = (text: string): string[] => text.split(/\s+/u).filter((piece) => piece.length > 0);
  for (const tune of PLAYLIST) {
    assert.equal(tune.chords.length, tune.melody.length, tune.title);
    tune.melody.forEach((bar, index) => {
      const cells = tokens(bar);
      assert.equal(cells.length, ROWS_PER_BAR, `${tune.title} bar ${index + 1}`);
      assert.ok(
        cells.every((token) => melodyCell(token) !== undefined),
        `${tune.title} bar ${index + 1}: ${bar}`,
      );
    });
    assert.equal(tokens(tune.bass).length, ROWS_PER_BAR, tune.title);
    assert.ok(tokens(tune.bass).every((token) => ["R", "T", "F", "S", "O", "-", "."].includes(token)));
    for (const drum of [tune.kick, tune.snare, tune.hats]) {
      assert.equal([...drum].length, ROWS_PER_BAR, tune.title);
    }
    assert.ok(tune.fill.length === 0 || [...tune.fill].length === ROWS_PER_BAR);
    const line = `${tune.style}, ${tune.tempo} bpm`;
    assert.ok(tune.title.length <= LONGEST_TITLE && line.length <= LONGEST_STYLE_LINE, `${line} fits its row`);
  }
});
