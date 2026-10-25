// Page 501, NeoTeletext Radio (crates/teletext-showcase/src/audio/
// radio.rs): one synthesized music stream for every listener (decision
// 7). Each tick's blocks are made once and sent, one message, to every
// session with the page open; the server encodes them. A listener is a
// session with the page open: the stream starts for the first and stops
// for the last, so nothing is made while nobody listens.
//
// The stream is the engine's live sound (its REQ-AUD-014, `audio.md`
// section 8.3) on the profile's channel 3, which the showcase's effects
// leave to it. A listener is sent the live object and a play bound to this
// page, then the blocks not yet heard, so it joins the stream where the
// others are, in the same generation. Blocks go whole, ahead of real time
// by LEAD_MS; the page, shared, shows what is playing and the voices'
// levels, redrawn once a tick for everyone.
//
// The server encodes each `blocks` message in one call of the engine's
// encoder, which carries its step size from block to block within a call
// and starts each call at the smallest, costing 2 to 7 dB at every
// restart (TECH-DEBT.md). So a tick's blocks go in one message, as the
// Rust radio encodes them in one call. A listener who joins is sent the
// blocks it has yet to hear in one message of their own: the same samples
// the others were sent, encoded afresh, where the Rust sent it the others'
// very records.

import { page } from "../common.ts";
import { type Application, Colour, type Context, type PageEntry, SIXELS_ACROSS, SIXELS_DOWN, type Screen, Sixels } from "../kit.ts";
import { badge, columnBar, sixels, trackBar } from "./draw.ts";
import { BOARD, DRUMS, MENU, PIANO, RADIO } from "./pages.ts";
import { ENVELOPE_ONE, MILLISECONDS, RATE, scaledLevel } from "./synth.ts";
import { ARPEGGIO, BASS, HATS, KICK, LEAD, SNARE, Tracker, VOICES, noteName, share } from "./tracker.ts";
import { PLAYLIST } from "./tunes.ts";

/** The stream's handle in the audio profile: clear of the effects library's 1 to 28. */
export const HANDLE = 40;
/** The profile's channel the radio plays on: the last, which the showcase's effects (`sound.ts`, channels 0 to 2) leave to it. */
export const CHANNEL = 3;
/** How loud the radio plays, 0 to 100. */
const GAIN = 85;
/** Its fade in and out, in milliseconds. */
const FADE_MS = 20;
/** The block size the server encodes a live sound's blocks in: 512 bytes (docs/applications.md section 3.4, the engine's `IMA_BLOCK_LARGE`). */
export const BLOCK_BYTES = 512;
/** The bytes of an IMA ADPCM block's header, which holds its first sample and step index (the engine's `audio.md`). */
const BLOCK_HEADER_BYTES = 4;
/** Samples a byte of IMA ADPCM codes: two four-bit codes. */
const SAMPLES_PER_BYTE = 2;
/** The samples of a block: the header's one and two a byte after it, 1,017, 64 ms at 16 kHz. */
export const BLOCK_SAMPLES = 1 + (BLOCK_BYTES - BLOCK_HEADER_BYTES) * SAMPLES_PER_BYTE;
/**
 * How far ahead of real time blocks are sent, in milliseconds: more than
 * the receiver mixer's buffer of a live sound (400 ms, the engine's
 * `audio.md` section 8.3) plus a {@link TICK_MS} between sends, well inside
 * its ring of 32 blocks (about 2 s at this rate).
 */
export const LEAD_MS = 800;
/** How often the stream is topped up and the page redrawn: four times a second. */
export const TICK_MS = 250;
/** The most blocks one tick sends, a ring's worth (the engine's `LIVE_RING_BLOCKS`): after a stall the stream skips ahead instead. */
const MOST_BLOCKS = 32;
/** Block numbers are two bytes, wrapping. */
const BLOCK_NUMBERS = 0x1_0000;
/** Generations are two bytes, wrapping. */
const GENERATIONS = 0x1_0000;

/** The stream sample heard at `milliseconds` of the channel's clock: every listener hears the same moment of the playlist. */
export function sampleAt(milliseconds: number): number {
  return Math.floor((milliseconds * RATE) / MILLISECONDS);
}

/** A block sent: its number, the stream sample it starts at, and its samples. */
interface Block {
  readonly number: number;
  readonly start: number;
  readonly samples: Int16Array;
}

/** The live sound now streaming. */
interface Stream {
  /** The next block's number. */
  nextBlock: number;
  /** The stream sample the next block starts at. */
  sent: number;
  /** The blocks sent and not yet heard, for a listener who joins. */
  recent: Block[];
}

/** What the meters show of a tick. */
interface Window {
  readonly levels: readonly number[];
  /** The mean absolute sample. */
  readonly loudness: number;
}

/** The column the horizontal meters start at. */
const METER_COLUMN = 3;
/** The progress bar's cells. */
const PROGRESS_CELLS = 36;
/** Its sixels across. */
const PROGRESS_SIXELS = 72;
if (PROGRESS_SIXELS !== PROGRESS_CELLS * SIXELS_ACROSS) {
  throw new Error("the progress bar's sixels are its cells'");
}
/** The voices' bars: their labels and colours, in the tracker's order. */
const BARS: readonly (readonly [string, Colour])[] = (() => {
  const bars: [string, Colour][] = Array.from({ length: VOICES }, () => ["", Colour.White]);
  bars[BASS] = ["Bass", Colour.Red];
  bars[KICK] = ["Kick", Colour.Yellow];
  bars[LEAD] = ["Lead", Colour.Green];
  bars[ARPEGGIO] = ["Arp", Colour.Cyan];
  bars[SNARE] = ["Snare", Colour.Magenta];
  bars[HATS] = ["Hats", Colour.White];
  return bars;
})();
/** The first bar's first cell, after its colour. */
const BARS_COLUMN = 3;
/** A bar's width in cells. */
const BAR_CELLS = 5;
/** From one bar to the next: its cells and the next one's colour. */
const BAR_STEP = BAR_CELLS + 1;
/** The bars' top row. */
const BARS_ROW = 8;
/** Their height in rows: the bars and their labels fill rows 8 to 16. */
const BAR_ROWS = 8;
/** Probes of the voices' levels over a tick: one every 8 ms, short enough to catch a closed hi-hat (35 ms). */
const LEVEL_PROBES = 32;
/** Samples between the loudness's probes. */
const LOUDNESS_STRIDE = 8;
/** The oscilloscope's row. */
const SCOPE_ROW = 19;
/** Its width in cells. */
const SCOPE_CELLS = 36;
/** Its height in rows. */
const SCOPE_ROWS = 3;
/** Samples between its columns: 72 columns cover 13.5 ms. */
const SCOPE_STRIDE = 3;
/** The smallest peak the trace scales to, so near silence stays flat. */
const SCOPE_QUIET = 1_000;
/** Thousandths in one. */
const THOUSAND = 1_000;

/** Thousandths as an envelope level, for {@link share}. */
function scaleThousandths(thousandths: number): number {
  return Math.floor((Math.min(thousandths, THOUSAND) * ENVELOPE_ONE) / THOUSAND);
}

/** The meter's three colours, each with its cells: green to a loud mix, yellow above it, red at the top. */
const LEVEL_SEGMENTS: readonly (readonly [Colour, number])[] = [
  [Colour.Green, 18],
  [Colour.Yellow, 7],
  [Colour.Red, 5],
];
/** The level meter's row, under the bars' labels. */
const LEVEL_ROW = 17;
/** The meter's first colour cell. */
const LEVEL_COLUMN = 6;
/** Quarter octaves in an octave: a meter in decibels reads a quarter octave as 1.5 dB. */
const QUARTERS = 4;
/** The quietest loudness the meter shows, in quarter octaves: 2^9, a mean of 512, 36 dB under full scale. */
const LEVEL_FLOOR = 9 * QUARTERS;
/** The loudest, in quarter octaves: 2^13, which the loudest passages reach (a full mix's mean is 4 000 to 8 000). */
const LEVEL_TOP = 13 * QUARTERS;
/** The bits under a number's top one that give its quarter octave. */
const QUARTER_BITS = 2;
/** The mask of those bits. */
const QUARTER_MASK = 0b11;
/** Bits in a 32-bit number, for its base-2 logarithm. */
const WORD_BITS = 32;

/** Loudness as quarter octaves: four a doubling, as a meter in decibels is read. */
function quarterOctaves(loudness: number): number {
  if (loudness <= 0) {
    return 0;
  }
  const octave = WORD_BITS - 1 - Math.clz32(loudness);
  // The two bits under the top one.
  const quarter = octave >= QUARTER_BITS ? (loudness >>> (octave - QUARTER_BITS)) & QUARTER_MASK : 0;
  return octave * QUARTERS + quarter;
}

/** The level meter on `row`: loudness as lit sixels of the segments. */
function levelMeter(screen: Screen, row: number, loudness: number): void {
  const total = LEVEL_SEGMENTS.reduce((sum, [, cells]) => sum + cells * SIXELS_ACROSS, 0);
  const range = LEVEL_TOP - LEVEL_FLOOR;
  const above = Math.min(Math.max(0, quarterOctaves(loudness) - LEVEL_FLOOR), range);
  const lit = Math.floor((above * total) / range);
  let column = LEVEL_COLUMN + 1;
  let before = 0;
  for (const [colour, cells] of LEVEL_SEGMENTS) {
    trackBar(screen, row, column, cells, Math.max(0, lit - before), colour);
    before += cells * SIXELS_ACROSS;
    column += cells + 1;
  }
}

/** Bits in a byte. */
const BYTE_BITS = 8;

/** What the stream is, on row 23. */
function streamLine(screen: Screen): void {
  const bits = RATE * BLOCK_BYTES * BYTE_BITS;
  const kilobits = Math.floor(Math.floor(bits / BLOCK_SAMPLES) / THOUSAND);
  screen.coloured(23, 0, Colour.Blue, `Live IMA ADPCM, ${Math.floor(RATE / THOUSAND)} kHz, ${kilobits} kbit/s`);
}

/** The radio. */
export class Radio implements Application {
  readonly #tracker = new Tracker();
  readonly #listeners = new Set<number>();
  #stream: Stream | null = null;
  /** The live sound's generation: each start is a new one. */
  #generation = 0;

  pages(): readonly PageEntry[] {
    return [{ page: RADIO, title: "NeoTeletext Radio" }];
  }

  start(context: Context): void {
    this.#draw(context);
  }

  opened(session: number, page: number, context: Context): void {
    if (page !== RADIO) {
      return;
    }
    const now = context.now();
    this.#listeners.add(session);
    const starting = this.#stream === null;
    if (starting) {
      this.#start(now);
    }
    this.#join(session, now, context);
    if (starting) {
      this.#pump(now, context);
      Radio.#schedule(context);
    }
    this.#draw(context);
  }

  leftPage(session: number, page: number, context: Context): void {
    if (page === RADIO && this.#listeners.has(session)) {
      context.sound({ session }, { stop: { channel: CHANNEL, fadeMs: FADE_MS } });
      this.#leave(session, context);
    }
  }

  wake(context: Context): void {
    if (this.#stream === null) {
      return;
    }
    this.#pump(context.now(), context);
    this.#draw(context);
    Radio.#schedule(context);
  }

  ended(session: number, context: Context): void {
    this.#leave(session, context);
  }

  /** Starts a new generation of the stream at `now`. */
  #start(now: number): void {
    this.#generation = (this.#generation + 1) % GENERATIONS;
    this.#stream = { nextBlock: 0, sent: sampleAt(now), recent: [] };
  }

  /** `session` joins: the live object, the play, and the blocks it has yet to hear, so it joins at the first of them. */
  #join(session: number, now: number, context: Context): void {
    const stream = this.#stream;
    if (stream === null) {
      return;
    }
    const sound = { handle: HANDLE, generation: this.#generation, rate: RATE };
    context.sound({ session }, { live: sound });
    // Bound to the page: the receiver fades it out when the viewer
    // leaves, before the server hears (`audio.md` 7.3).
    context.sound({ session }, { play: { channel: CHANNEL, ...sound, gain: GAIN, fadeMs: FADE_MS, page: RADIO } });
    const heard = sampleAt(now);
    const unheard = stream.recent.filter((block) => block.start + BLOCK_SAMPLES > heard);
    const first = unheard[0];
    if (first !== undefined) {
      const samples = new Int16Array(unheard.length * BLOCK_SAMPLES);
      unheard.forEach((block, index) => samples.set(block.samples, index * BLOCK_SAMPLES));
      context.sound({ session }, { blocks: { ...sound, first: first.number, samples } });
    }
  }

  /** Makes the blocks due by `now` plus the lead, once, and sends them to every listener. */
  #pump(now: number, context: Context): void {
    const stream = this.#stream;
    if (stream === null) {
      return;
    }
    const heard = sampleAt(now);
    const due = sampleAt(now + LEAD_MS);
    // After a stall the blocks already past are skipped, not sent late.
    stream.sent = Math.max(stream.sent, heard);
    const count = Math.min(Math.floor(Math.max(0, due - stream.sent) / BLOCK_SAMPLES), MOST_BLOCKS);
    if (count > 0) {
      const first = stream.nextBlock;
      // One message for the tick's blocks: the server encodes it in one
      // call, keeping the encoder's step size from block to block.
      const samples = this.#tracker.render(stream.sent, count * BLOCK_SAMPLES);
      for (let index = 0; index < count; index += 1) {
        const offset = index * BLOCK_SAMPLES;
        stream.recent.push({ number: stream.nextBlock, start: stream.sent, samples: samples.subarray(offset, offset + BLOCK_SAMPLES) });
        stream.nextBlock = (stream.nextBlock + 1) % BLOCK_NUMBERS;
        stream.sent += BLOCK_SAMPLES;
      }
      context.sound({ page: RADIO }, { blocks: { handle: HANDLE, generation: this.#generation, rate: RATE, first, samples } });
    }
    while (stream.recent[0] !== undefined && stream.recent[0].start + BLOCK_SAMPLES <= heard) {
      stream.recent.shift();
    }
  }

  /** The next tick, on the tick's grid. */
  static #schedule(context: Context): void {
    const now = context.now();
    context.wakeAt(now + TICK_MS - (now % TICK_MS));
  }

  #leave(session: number, context: Context): void {
    if (this.#listeners.delete(session) && this.#listeners.size === 0) {
      this.#stream = null;
      this.#draw(context);
    }
  }

  /** The page, shared, as it is now. */
  #draw(context: Context): void {
    const screen = page("NeoTeletext Radio", Colour.Blue, [
      ["Sound", MENU],
      ["Piano", PIANO],
      ["Board", BOARD],
      ["Drums", DRUMS],
    ]);
    if (this.#stream === null) {
      Radio.#offAir(screen);
    } else {
      this.#onAir(screen, context.now());
    }
    context.publish("channel", RADIO, screen);
  }

  static #offAir(screen: Screen): void {
    badge(screen, 3, 0, Colour.Blue, Colour.White, " OFF AIR ");
    screen.coloured(3, 14, Colour.Cyan, "nobody is listening");
    screen.centred(5, Colour.White, "Open this page to tune in: the radio");
    screen.centred(6, Colour.White, "starts for its first listener, and");
    screen.centred(7, Colour.White, "every listener hears one stream.");
    screen.coloured(9, 1, Colour.Yellow, "On the playlist");
    let row = 11;
    for (const tune of PLAYLIST.filter((candidate) => !candidate.ident)) {
      screen.coloured(row, 2, Colour.White, tune.title);
      screen.coloured(row, 20, Colour.Cyan, tune.style);
      row += 2;
    }
    streamLine(screen);
  }

  #onAir(screen: Screen, now: number): void {
    const heard = sampleAt(now);
    badge(screen, 3, 0, Colour.Red, Colour.White, " ON AIR ");
    const listeners = this.#listeners.size;
    const noun = listeners === 1 ? "listener" : "listeners";
    screen.coloured(3, 13, Colour.Cyan, `${listeners} ${noun}, one stream`);
    const place = this.#tracker.place(heard);
    if (place !== undefined) {
      screen.coloured(4, 0, Colour.Yellow, "Now");
      screen.coloured(4, 5, Colour.White, place.tune.title);
      screen.coloured(5, 5, Colour.Cyan, `${place.tune.style}, ${place.tune.tempo} bpm`);
      const lead = place.lead === undefined ? "rest" : noteName(place.lead);
      const bar = String(place.bar + 1).padStart(2);
      screen.coloured(6, 0, Colour.Green, `Bar ${bar} of ${String(place.bars).padEnd(2)} Chord ${place.chord.padEnd(4)} Lead ${lead}`);
      const progress = share(scaleThousandths(place.thousandths), PROGRESS_SIXELS);
      trackBar(screen, 7, METER_COLUMN, PROGRESS_CELLS, progress, Colour.Yellow);
      screen.coloured(22, 0, Colour.Yellow, "Next");
      screen.coloured(22, 5, Colour.White, place.next.title);
    }
    const window = this.#window(heard);
    BARS.forEach(([name, colour], index) => {
      const column = BARS_COLUMN + index * BAR_STEP;
      const height = share(window.levels[index] ?? 0, BAR_ROWS * SIXELS_DOWN);
      columnBar(screen, BARS_ROW, column, [BAR_CELLS, BAR_ROWS], height, colour);
      screen.coloured(BARS_ROW + BAR_ROWS, column - 1, colour, name);
    });
    screen.coloured(LEVEL_ROW, 0, Colour.White, "Level");
    levelMeter(screen, LEVEL_ROW, window.loudness);
    this.#scope(screen, heard);
    streamLine(screen);
  }

  /** The voices' loudest levels, and the mix's loudness, over the tick just heard. */
  #window(heard: number): Window {
    const span = sampleAt(TICK_MS);
    const first = Math.max(0, heard - span);
    const levels = new Array<number>(VOICES).fill(0);
    for (let probe = 1; probe <= LEVEL_PROBES; probe += 1) {
      const at = first + Math.floor((span * probe) / LEVEL_PROBES);
      // A level counts less the longer ago it was: a peak meter's fall,
      // over a tick, so a drum hit shows and then dies away.
      const recency = Math.floor((ENVELOPE_ONE * probe) / LEVEL_PROBES);
      this.#tracker.voices(at).forEach((voice, index) => {
        levels[index] = Math.max(levels[index] ?? 0, scaledLevel(voice.level, recency));
      });
    }
    let total = 0;
    let count = 0;
    for (let at = first; at < heard; at += LOUDNESS_STRIDE) {
      total += Math.abs(this.#tracker.sample(at));
      count += 1;
    }
    return { levels, loudness: count === 0 ? 0 : Math.floor(total / count) };
  }

  /** The waveform just heard, as a trace in mosaics, scaled to its own peak so a quiet passage shows its shape too. */
  #scope(screen: Screen, heard: number): void {
    const [width, height] = sixels(SCOPE_CELLS, SCOPE_ROWS);
    const canvas = new Sixels(width, height);
    const first = Math.max(0, heard - width * SCOPE_STRIDE);
    const trace = Array.from({ length: width }, (_, x) => this.#tracker.sample(first + x * SCOPE_STRIDE));
    const peak = Math.max(SCOPE_QUIET, ...trace.map((sample) => Math.abs(sample)));
    const bottom = height - 1;
    let previous: number | null = null;
    trace.forEach((sample, x) => {
      // From -peak at the bottom to +peak at the top.
      const lifted = Math.min(Math.max(peak - sample, 0), peak * 2);
      const y = Math.floor((lifted * bottom) / (peak * 2));
      const [from, to] = previous === null ? [y, y] : [Math.min(previous, y), Math.max(previous, y)];
      for (let joined = from; joined <= to; joined += 1) {
        canvas.set(x, joined, true);
      }
      previous = y;
    });
    canvas.draw(screen, SCOPE_ROW, METER_COLUMN, Colour.Green);
  }
}
