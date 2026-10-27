// The podcast's clips as live streams (the library's core/stream.ts): each
// a story cut from an episode, or a What's That Sound round's segment, kept
// as Ogg Opus. The server takes PCM, so ffmpeg decodes a clip to 16 kHz
// mono as it is asked for, in the background; the stream sends what is
// decoded so far and waits for the rest. The latest few are kept decoded,
// since a viewer who plays one often plays it again.

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";

import type { StreamSource } from "../../../kits/typescript-components/src/index.ts";

/** The rate clips are decoded to: speech, and what the showcase's radio streams at. */
const RATE = 16_000;
/** Bytes in a 16-bit sample. */
const SAMPLE_BYTES = 2;
/** The clips peak at full scale (measured with ffmpeg's volumedetect, 27 October 2026: 0.0 dB); they are decoded this much quieter, so the server's IMA ADPCM, which overshoots on steep peaks, has room. */
const HEADROOM = "volume=-3dB";
/** Clips kept decoded: about two minutes each at 16 kHz is 4 MB. */
const KEPT = 12;
/** The longest a clip may be decoded to: the longest story is 33 minutes. */
const LONGEST_SAMPLES = RATE * 60 * 40;

/** A clip being decoded, or decoded. */
class DecodedClip implements StreamSource {
  readonly name: string;
  readonly rate = RATE;
  readonly expected: number | null;
  complete = false;
  failure: string | null = null;
  #buffer = new Int16Array(RATE * 60);
  #length = 0;
  #pending: Buffer = Buffer.alloc(0);

  constructor(name: string, path: string, seconds: number | null, ffmpeg: string) {
    this.name = name;
    this.expected = seconds === null ? null : Math.round(seconds * RATE);
    if (!existsSync(path)) {
      this.failure = "the clip is not on this server";
      this.complete = true;
      return;
    }
    const child = spawn(ffmpeg, ["-v", "error", "-nostdin", "-i", path, "-ac", "1", "-ar", String(RATE), "-af", HEADROOM, "-f", "s16le", "pipe:1"], { stdio: ["ignore", "pipe", "pipe"] });
    let errors = "";
    child.stdout.on("data", (chunk: Buffer) => this.#take(chunk));
    child.stderr.on("data", (chunk: Buffer) => {
      errors = `${errors}${chunk.toString()}`.slice(-200);
    });
    child.on("error", (error) => {
      this.failure = `the decoder did not start: ${error.message}`;
      this.complete = true;
    });
    child.on("close", (code) => {
      if (code !== 0 && this.failure === null) {
        this.failure = `the decoder failed: ${errors.trim() || `exit ${String(code)}`}`;
      }
      this.complete = true;
    });
  }

  #take(chunk: Buffer): void {
    const bytes = Buffer.concat([this.#pending, chunk]);
    const whole = bytes.length - (bytes.length % SAMPLE_BYTES);
    this.#pending = bytes.subarray(whole);
    const count = whole / SAMPLE_BYTES;
    if (this.#length + count > LONGEST_SAMPLES) {
      return;
    }
    if (this.#length + count > this.#buffer.length) {
      const grown = new Int16Array(Math.min(LONGEST_SAMPLES, Math.max(this.#buffer.length * 2, this.#length + count)));
      grown.set(this.#buffer.subarray(0, this.#length));
      this.#buffer = grown;
    }
    for (let index = 0; index < count; index += 1) {
      this.#buffer[this.#length + index] = bytes.readInt16LE(index * SAMPLE_BYTES);
    }
    this.#length += count;
  }

  samples(): Int16Array {
    return this.#buffer.subarray(0, this.#length);
  }
}

/** Clips by path, decoded on demand. */
export class Clips {
  readonly #ffmpeg: string;
  readonly #kept = new Map<string, DecodedClip>();

  constructor(ffmpeg = "ffmpeg") {
    this.#ffmpeg = ffmpeg;
  }

  /** The clip at `path`, named `name`, decoding if it is not kept. */
  source(name: string, path: string, seconds: number | null): StreamSource {
    const kept = this.#kept.get(path);
    if (kept !== undefined && kept.failure === null) {
      this.#kept.delete(path);
      this.#kept.set(path, kept);
      return kept;
    }
    const clip = new DecodedClip(name, path, seconds, this.#ffmpeg);
    this.#kept.set(path, clip);
    while (this.#kept.size > KEPT) {
      const oldest = this.#kept.keys().next().value;
      if (oldest === undefined) {
        break;
      }
      this.#kept.delete(oldest);
    }
    return clip;
  }
}
