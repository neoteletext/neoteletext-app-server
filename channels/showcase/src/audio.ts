// Sound (pages 500 to 599; crates/teletext-showcase/src/audio.rs): a
// streamed radio, a piano, a sound board and a drum machine, in the
// engine's audio profile (decisions E.84 to E.86).
//
// - 500, the menu.
// - 501, NeoTeletext Radio: one live stream of synthesized music for every
//   listener (audio/radio.ts).
// - 502, the piano; 503, the sound board; 504, the drum machine: each
//   viewer's own, playing the showcase's effects library (audio/studio.ts).

import type { Area, Section } from "./area.ts";
import { BOARD, DRUMS, MENU, PIANO, RADIO } from "./audio/pages.ts";
import { Radio } from "./audio/radio.ts";
import { Studio } from "./audio/studio.ts";
import { page } from "./common.ts";
import { type Application, Colour, type Context, type PageEntry, pageText } from "./kit.ts";

/** The menu's entries: page, name and what it is, on two lines. */
const ENTRIES: readonly (readonly [number, string, string, string])[] = [
  [RADIO, "NeoTeletext Radio", "music made live, one stream", "for every listener"],
  [PIANO, "Piano", "an octave on your letter keys,", "on any keyboard layout"],
  [BOARD, "Sound board", "nine effects on the number keys", ""],
  [DRUMS, "Drum machine", "sixteen steps of your own,", "played in a loop"],
];
/** The menu's first entry's row. */
const FIRST_ROW = 4;
/** Rows from one entry to the next. */
const ROW_STEP = 4;

/** Page 500, shared: the menu. */
class Menu implements Application {
  pages(): readonly PageEntry[] {
    return [{ page: MENU, title: "Sound" }];
  }

  start(context: Context): void {
    const screen = page("Sound", Colour.Red, [
      ["Radio", RADIO],
      ["Piano", PIANO],
      ["Board", BOARD],
      ["Drums", DRUMS],
    ]);
    let row = FIRST_ROW;
    for (const [number, name, about, more] of ENTRIES) {
      screen.coloured(row, 1, Colour.Yellow, pageText(number));
      screen.coloured(row, 5, Colour.White, name);
      screen.coloured(row + 1, 5, Colour.Cyan, about);
      screen.coloured(row + 2, 5, Colour.Cyan, more);
      row += ROW_STEP;
    }
    screen.centred(21, Colour.Green, "Turn the browser's sound on to hear");
    screen.centred(22, Colour.Green, "these pages.");
    context.publish("channel", MENU, screen);
  }
}

/** Sound. */
export const audio: Area = {
  applications: () => [new Menu(), new Radio(), new Studio()],
  sections: (): readonly Section[] => [{ page: MENU, name: "Sound", about: "radio, piano, sound board, drums" }],
};
