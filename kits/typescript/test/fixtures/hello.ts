// A small channel for the server's end-to-end test
// (crates/teletext-native/tests/typescript_channel.rs): pages drawn with
// the kit, with control codes, the English set's own signs, mosaics,
// Fastext and sub-pages, so the test can check the server reads the TTI
// the kit writes as it was drawn.

import { runChannel } from "../../src/channel.ts";
import { Colour, Screen, Sixels, SIXELS_ACROSS, SIXELS_DOWN } from "../../src/screen.ts";

/** The pages the test reads. */
const INDEX = 0x100;
const CAROUSEL = 0x101;
/** The carousel's sub-pages. */
const SUBPAGES = [1, 2] as const;

function index(): Screen {
  const screen = new Screen();
  screen.header("Kit");
  screen.coloured(1, 0, Colour.Red, "Red #1 £2");
  screen.doubleHeight(2, 0, Colour.Yellow, "Tall");
  // One cell of every sixel and one of none.
  const canvas = new Sixels(SIXELS_ACROSS * 2, SIXELS_DOWN);
  for (let y = 0; y < SIXELS_DOWN; y += 1) {
    for (let x = 0; x < SIXELS_ACROSS; x += 1) {
      canvas.set(x, y, true);
    }
  }
  canvas.draw(screen, 4, 1, Colour.Green);
  screen.fastext([CAROUSEL, null, null, INDEX]);
  screen.indexLink(INDEX);
  return screen;
}

function subpage(number: number): Screen {
  const screen = new Screen();
  screen.coloured(1, 0, Colour.White, `Sub-page ${number}`);
  return screen;
}

await runChannel([
  {
    pages: () => [
      { page: INDEX, title: "Kit index" },
      { page: CAROUSEL, title: "Kit carousel" },
    ],
    start: (context) => {
      context.publish("channel", INDEX, index());
      context.publishSubpages(
        "channel",
        CAROUSEL,
        SUBPAGES.map((number) => ({ screen: subpage(number), subpage: number })),
      );
    },
  },
]);
