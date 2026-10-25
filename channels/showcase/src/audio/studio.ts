// Pages 502 to 504, the instruments (crates/teletext-showcase/src/audio/
// studio.rs): the piano, the sound board and the drum machine. They are
// one application so a viewer is sent the effects library once for all
// three, and so the drum machines share one wake.

import { page } from "../common.ts";
import { type Application, Button, Colour, type Context, type Input, Key, Mode, type PageEntry, type Screen, character, isCharacterKey, pressed } from "../kit.ts";
import { Sounds } from "../sound.ts";
import * as board from "./board.ts";
import * as drums from "./drums.ts";
import { BOARD, DRUMS, MENU, PIANO, RADIO } from "./pages.ts";
import * as piano from "./piano.ts";
import { asciiLower } from "./tracker.ts";

/** A viewer's instruments. */
class Viewer {
  piano: piano.Held = piano.released();
  board: number | null = null;
  readonly machine = new drums.Machine();
}

/** The page for `number`, with what `viewer` has. */
function screen(number: number, viewer: Viewer, playhead: number | null): Screen {
  switch (number) {
    case PIANO: {
      const screen = page("Piano", Colour.Green, [
        ["Sound", MENU],
        ["Radio", RADIO],
        ["Board", BOARD],
        ["Drums", DRUMS],
      ]);
      piano.draw(screen, viewer.piano);
      screen.modes(Mode.AllKeys | Mode.KeyRelease | Mode.BaseKey | Mode.MouseButtons);
      return screen;
    }
    case BOARD: {
      const screen = page("Sound board", Colour.Magenta, [
        ["Sound", MENU],
        ["Radio", RADIO],
        ["Piano", PIANO],
        ["Drums", DRUMS],
      ]);
      board.draw(screen, viewer.board);
      screen.modes(Mode.MouseButtons);
      return screen;
    }
    default: {
      const screen = page("Drum machine", Colour.Blue, [
        ["Sound", MENU],
        ["Radio", RADIO],
        ["Piano", PIANO],
        ["Board", BOARD],
      ]);
      drums.draw(screen, viewer.machine, playhead);
      screen.modes(Mode.MouseButtons);
      return screen;
    }
  }
}

/** The key's character by position: `base` when the receiver gives it, the key's own character otherwise. */
function keyCharacter(input: Input): string | undefined {
  if (input.kind !== "key") {
    return undefined;
  }
  if (input.base !== undefined) {
    return [...input.base][0];
  }
  return isCharacterKey(input.key) ? input.key : undefined;
}

/** The arrow keys' moves of the drum machine's cursor: tracks down and steps across. */
const MOVES: readonly (readonly [key: string, down: number, across: number])[] = [
  [Key.Up, -1, 0],
  [Key.Down, 1, 0],
  [Key.Left, 0, -1],
  [Key.Right, 0, 1],
];

/** The instruments. */
export class Studio implements Application {
  readonly #sounds = new Sounds();
  readonly #viewers = new Map<number, Viewer>();
  /** Whether a wake is asked for, for the drum machines' next step. */
  #ticking = false;

  // The board takes the number keys; the piano's and the drum machine's
  // keys are letters, arrows, Space and Enter, so digits and the coloured
  // keys select pages there.
  pages(): readonly PageEntry[] {
    return [
      { page: PIANO, title: "Piano" },
      { page: BOARD, title: "Sound board", ownsDigits: true },
      { page: DRUMS, title: "Drum machine" },
    ];
  }

  start(context: Context): void {
    const viewer = new Viewer();
    for (const number of [PIANO, BOARD, DRUMS]) {
      context.publish("channel", number, screen(number, viewer, null));
    }
  }

  opened(session: number, page: number, context: Context): void {
    if (!this.#viewers.has(session)) {
      this.#viewers.set(session, new Viewer());
    }
    // The library before the first press, so the first note is heard at
    // once.
    this.#sounds.load(session, context);
    this.#draw(session, page, context);
  }

  leftPage(session: number, page: number): void {
    const viewer = this.#viewers.get(session);
    if (viewer === undefined) {
      return;
    }
    if (page === PIANO) {
      viewer.piano = piano.released(viewer.piano.last);
    } else if (page === DRUMS) {
      viewer.machine.playing = false;
    }
  }

  input(session: number, page: number, input: Input, context: Context): void {
    let changed = false;
    if (page === PIANO) {
      changed = this.#piano(session, input, context);
    } else if (page === BOARD) {
      changed = this.#board(session, input, context);
    } else if (page === DRUMS) {
      changed = this.#drums(session, input, context);
    }
    if (changed) {
      this.#draw(session, page, context);
    }
  }

  wake(context: Context): void {
    this.#ticking = false;
    const step = drums.stepAt(context.now());
    // In session order, as the Rust's map is kept, so the library's
    // channels rotate over the plays as they do there.
    const playing = [...this.#viewers.entries()].filter(([, viewer]) => viewer.machine.playing).sort(([one], [two]) => one - two);
    for (const [session, viewer] of playing) {
      drums.TRACKS.forEach(([, effect], track) => {
        if (viewer.machine.on(track, step)) {
          this.#sounds.play(session, effect, context);
        }
      });
      this.#draw(session, DRUMS, context);
    }
    this.#tick(context);
  }

  ended(session: number): void {
    this.#viewers.delete(session);
    this.#sounds.forget(session);
  }

  #draw(session: number, number: number, context: Context): void {
    const viewer = this.#viewers.get(session);
    if (viewer === undefined) {
      return;
    }
    const playhead = viewer.machine.playing ? drums.stepAt(context.now()) : null;
    context.publish(session, number, screen(number, viewer, playhead));
  }

  #piano(session: number, input: Input, context: Context): boolean {
    const viewer = this.#viewers.get(session);
    if (viewer === undefined) {
      return false;
    }
    const held = viewer.piano;
    switch (input.kind) {
      case "key": {
        if (input.action === "repeat") {
          return false;
        }
        const typed = keyCharacter(input);
        const note = typed === undefined ? undefined : piano.noteOf(typed);
        if (note === undefined) {
          return false;
        }
        if (input.action === "press") {
          held.keys |= 1 << note;
          held.last = note;
          this.#sounds.play(session, { note }, context);
        } else {
          held.keys &= ~(1 << note);
        }
        return true;
      }
      // Text comes only from a receiver that did not take ALL_KEYS.
      case "text": {
        const typed = character(input);
        const note = typed === undefined ? undefined : piano.noteOf(typed);
        if (note === undefined) {
          return false;
        }
        held.last = note;
        this.#sounds.play(session, { note }, context);
        return true;
      }
      case "mouse": {
        if (!input.pressed) {
          const wasHeld = held.mouse !== null;
          held.mouse = null;
          return wasHeld;
        }
        if (input.button !== Button.Left) {
          return false;
        }
        const note = piano.noteAt(input.column, input.row);
        if (note === undefined) {
          return false;
        }
        held.mouse = note;
        held.last = note;
        this.#sounds.play(session, { note }, context);
        return true;
      }
      case "blur":
        viewer.piano = piano.released(held.last);
        return true;
      default:
        return false;
    }
  }

  #board(session: number, input: Input, context: Context): boolean {
    let button: number | undefined;
    if (input.kind === "mouse" && input.pressed && input.button === Button.Left) {
      button = board.buttonAt(input.column, input.row);
    } else {
      const typed = character(input);
      button = typed === undefined ? undefined : board.buttonOf(typed);
    }
    const viewer = this.#viewers.get(session);
    if (button === undefined || viewer === undefined) {
      return false;
    }
    viewer.board = button;
    const effect = board.BUTTONS[button]?.[0];
    if (effect !== undefined) {
      this.#sounds.play(session, effect, context);
    }
    return true;
  }

  #drums(session: number, input: Input, context: Context): boolean {
    const viewer = this.#viewers.get(session);
    if (viewer === undefined) {
      return false;
    }
    const machine = viewer.machine;
    const move = MOVES.find(([key]) => pressed(input, key));
    if (move !== undefined) {
      machine.go(move[1], move[2]);
    } else if (pressed(input, Key.Enter)) {
      machine.playing = !machine.playing;
    } else if (input.kind === "mouse" && input.pressed && input.button === Button.Left) {
      const place = drums.stepAtCell(input.column, input.row);
      if (place === undefined) {
        return false;
      }
      machine.cursor = place;
      machine.toggle();
    } else {
      const typed = character(input);
      const lower = typed === undefined ? undefined : asciiLower(typed);
      if (lower === " ") {
        machine.toggle();
      } else if (lower === "c") {
        machine.clear();
      } else {
        return false;
      }
    }
    if (machine.playing) {
      this.#tick(context);
    }
    return true;
  }

  /** Asks for the next step's wake if a machine plays and none is asked. */
  #tick(context: Context): void {
    if (!this.#ticking && [...this.#viewers.values()].some((viewer) => viewer.machine.playing)) {
      const now = context.now();
      context.wakeAt(now + drums.STEP_MS - (now % drums.STEP_MS));
      this.#ticking = true;
    }
  }
}
