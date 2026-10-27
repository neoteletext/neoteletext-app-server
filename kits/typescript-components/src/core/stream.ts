// Sound too long to send whole: a live sound (docs/applications.md section
// 3.4; the engine's audio.md section 8.3), played to one viewer as it is
// sent. The runtime sends its blocks ahead of real time, a tick at a time,
// as the showcase's radio does: blocks of BLOCK_SAMPLES, LEAD_MS ahead,
// never more than the receiver's ring at once. Where the samples come from
// is the source's: one that is still being decoded says how far it has
// got, and the stream waits for it.

import type { SampleRate } from "../kit.ts";

/** A live sound's samples, as far as they are known. */
export interface StreamSource {
  /** What it is, for the page that shows it playing. */
  readonly name: string;
  /** Its sample rate: one the protocol takes (16000 for speech). */
  readonly rate: SampleRate;
  /** The samples known so far, 16-bit mono. */
  samples(): Int16Array;
  /** Whether every sample is known. */
  readonly complete: boolean;
  /** Why it cannot play, if it cannot. */
  readonly failure: string | null;
  /** Its length in samples when known in advance, or null. */
  readonly expected: number | null;
}

/** A viewer's stream as a page shows it. */
export interface Listening {
  readonly name: string;
  /** Milliseconds heard, and in all (null while not known). */
  readonly heardMs: number;
  readonly lengthMs: number | null;
  readonly state: "waiting" | "playing" | "paused" | "ended" | "failed";
  readonly failure: string | null;
}

/** The block size the server encodes a live sound's blocks in: 512 bytes of IMA ADPCM, its 4-byte header holding the first sample, two samples a byte after (the engine's audio.md). */
export const BLOCK_SAMPLES = 1 + (512 - 4) * 2;
/** How far ahead of real time blocks are sent: more than the receiver's 400 ms buffer and a tick, well inside its ring (the showcase radio's measure). */
export const LEAD_MS = 800;
/** How often streams are topped up. */
export const TICK_MS = 250;
/** The most blocks one tick sends: the receiver's ring (the engine's LIVE_RING_BLOCKS). */
export const MOST_BLOCKS = 32;
/** Block numbers and generations are two bytes, wrapping. */
export const WRAP = 0x1_0000;
/** The handle every viewer's stream has: the last, which stored sounds never take. */
export const STREAM_HANDLE = 255;
/** The profile's channel streams play on: the last of four, clear of effects. */
export const STREAM_CHANNEL = 3;
/** Milliseconds in a second, for converting samples. */
export const SECOND_MS = 1_000;
