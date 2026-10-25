// Page 504, the drum machine (crates/teletext-showcase/src/audio/
// drums.rs): a step sequencer of sixteen steps over six of the library's
// sounds, each viewer's own pattern, played in a loop to that viewer
// alone. Every viewer's steps fall on one clock, so the channel needs one
// wake for all of them.

import { Colour, type Screen, mosaicColour } from "../kit.ts";
import { Effect } from "../sound.ts";
import { badge } from "./draw.ts";

/** Steps in the loop: a bar of sixteenth notes. */
export const STEPS = 16;
/** The tracks: their labels, sounds and colours, top to bottom. */
export const TRACKS: readonly (readonly [string, Effect, Colour])[] = [
  ["Bell", Effect.Bell, Colour.Magenta],
  ["C'", { note: 12 }, Colour.Cyan],
  ["G", { note: 7 }, Colour.Cyan],
  ["E", { note: 4 }, Colour.Cyan],
  ["Hit", Effect.Hit, Colour.Yellow],
  ["Tick", Effect.Click, Colour.White],
];
/** The tempo in beats a minute. */
const BEATS_PER_MINUTE = 120;
/** Milliseconds in a minute. */
const MINUTE_MS = 60_000;
/** The steps a beat. */
const BEAT = 4;
/** The tempo: 120 beats a minute of four steps, so a step is 125 ms. */
export const STEP_MS = MINUTE_MS / BEATS_PER_MINUTE / BEAT;
/**
 * A pattern to start from, a step a bit, so the machine plays something
 * at once: a rising arpeggio over a backbeat and running ticks.
 */
const START = [0x0001, 0x4100, 0x0840, 0x8008, 0x1010, 0x5555] as const;
if (START.length !== TRACKS.length) {
  throw new Error("a starting pattern for each track");
}

/** One viewer's machine. */
export class Machine {
  /** Each track's steps, a bit a step. */
  steps: number[] = [...START];
  /** The cursor: track and step. */
  cursor: [track: number, step: number] = [0, 0];
  /** Whether it plays. */
  playing = false;

  /** Whether `track` sounds at `step`. */
  on(track: number, step: number): boolean {
    const bits = this.steps[track];
    return bits !== undefined && step < STEPS && (bits & (1 << step)) !== 0;
  }

  /** Turns the step under the cursor on or off. */
  toggle(): void {
    const [track, step] = this.cursor;
    const bits = this.steps[track];
    if (bits !== undefined && step < STEPS) {
      this.steps[track] = bits ^ (1 << step);
    }
  }

  /** Moves the cursor, staying on the grid. */
  go(down: number, across: number): void {
    const [track, step] = this.cursor;
    this.cursor = [Math.min(Math.max(0, track + down), TRACKS.length - 1), Math.min(Math.max(0, step + across), STEPS - 1)];
  }

  /** Clears every step. */
  clear(): void {
    this.steps = new Array<number>(TRACKS.length).fill(0);
  }
}

/** The step the clock is at, at `now`. */
export function stepAt(now: number): number {
  return Math.floor(now / STEP_MS) % STEPS;
}

/** The grid's top row. */
const GRID_ROW = 5;
/** The first step's colour cell. */
const GRID_COLUMN = 6;
/** Columns a step takes: its colour and its mosaic. */
const STEP_CELLS = 2;
/** A step that is on: every sixel (EN 300 706 Table 47). */
const ON = 0x7f;
/** A step that is off: the middle sixel row, a dash. */
const OFF = 0x2c;

/** The cell of `step`'s mosaic. */
function stepColumn(step: number): number {
  return GRID_COLUMN + step * STEP_CELLS + 1;
}

/** The colour of a step's cell: the cursor, the playhead, on, a beat's first, or off. */
function stepColour(machine: Machine, track: number, step: number, playhead: number | null): Colour {
  if (machine.cursor[0] === track && machine.cursor[1] === step) {
    return Colour.Yellow;
  }
  if (playhead === step) {
    return Colour.White;
  }
  if (machine.on(track, step)) {
    return Colour.Green;
  }
  return step % BEAT === 0 ? Colour.Cyan : Colour.Blue;
}

/** Draws the machine; `playhead` is the step playing, when it plays. */
export function draw(screen: Screen, machine: Machine, playhead: number | null): void {
  screen.coloured(3, 1, Colour.Cyan, "Sixteen steps, six sounds, your own");
  for (let beat = 0; beat < STEPS / BEAT; beat += 1) {
    screen.coloured(GRID_ROW - 1, stepColumn(beat * BEAT) - 1, Colour.Yellow, `${beat + 1}`);
  }
  TRACKS.forEach(([label, , colour], track) => {
    const row = GRID_ROW + track;
    screen.coloured(row, 0, colour, label);
    for (let step = 0; step < STEPS; step += 1) {
      const mosaic = machine.on(track, step) ? ON : OFF;
      screen.codes(row, stepColumn(step) - 1, [mosaicColour(stepColour(machine, track, step, playhead)), mosaic]);
    }
  });
  const marker = GRID_ROW + TRACKS.length;
  if (playhead !== null) {
    // The up arrow, which the Rust wrote as `^`, its code in the English
    // set (EN 300 706 Table 36).
    screen.coloured(marker, stepColumn(playhead) - 1, Colour.White, "↑");
  }
  if (machine.playing) {
    badge(screen, 13, 0, Colour.Green, Colour.Blue, " PLAYING ");
    screen.coloured(13, 13, Colour.White, "120 bpm, in a loop");
  } else {
    badge(screen, 13, 0, Colour.Blue, Colour.White, " STOPPED ");
    screen.coloured(13, 13, Colour.White, "Enter to play");
  }
  screen.coloured(15, 1, Colour.Green, "Arrows");
  screen.coloured(15, 12, Colour.White, "move the cursor");
  screen.coloured(16, 1, Colour.Green, "Space");
  screen.coloured(16, 12, Colour.White, "turn a step on or off");
  screen.coloured(17, 1, Colour.Green, "Enter");
  screen.coloured(17, 12, Colour.White, "play or stop");
  screen.coloured(18, 1, Colour.Green, "C");
  screen.coloured(18, 12, Colour.White, "clear the pattern");
  screen.coloured(20, 1, Colour.Cyan, "A click on a step turns it on or off.");
}

/** The track and step at a cell, for the mouse. */
export function stepAtCell(column: number, row: number): [track: number, step: number] | undefined {
  const track = row - GRID_ROW;
  if (track < 0 || track >= TRACKS.length || column < GRID_COLUMN) {
    return undefined;
  }
  const step = Math.floor((column - GRID_COLUMN) / STEP_CELLS);
  return step < STEPS ? [track, step] : undefined;
}
