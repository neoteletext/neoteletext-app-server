// A small channel for the server's end-to-end test
// (crates/teletext-native/tests/typescript_channel.rs): pages drawn with
// the kit, with control codes, the English set's own signs, mosaics,
// Fastext, links to a sub-page and sub-pages, so the test can check the
// server reads the TTI the kit writes as it was drawn.

import { runChannel } from "../../src/channel.ts";
import { packedLinksLine } from "../../src/packed-links.ts";
import { Colour, CursorFlag, Mode, Screen, Sixels, SIXELS_ACROSS, SIXELS_DOWN, decimalSubcode } from "../../src/screen.ts";

/** The pages the test reads. */
const INDEX = 0x100;
const CAROUSEL = 0x101;
const RAW = 0x102;
const SUBPAGE_LINK = 0x103;
/** A page in TTI as a converter writes it, with lines the kit never writes (DE, PS). */
const RAW_TTI = "DE,Raw\nPN,10200\nSC,0000\nPS,8000\nOL,1,\u001bBRaw\nFL,100,8FF,8FF,8FF,8FF,100\n";
/** The carousel's sub-pages. */
const SUBPAGES = [1, 2] as const;
/** A page whose red link names the carousel's second sub-page, which only a packed OL,27 line can say. */
const SUBPAGE_LINK_TTI = `PN,10300\nSC,0000\n${packedLinksLine(
  SUBPAGE_LINK,
  [{ page: CAROUSEL, subcode: decimalSubcode(2) }, null, null, null, null, { page: INDEX, subcode: null }],
  true,
)}\nOL,1,Links\n`;

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
  // A cursor and input modes, which the server must accept too.
  screen.cursor(6, 2, 10, CursorFlag.Echo);
  screen.modes(Mode.MouseButtons);
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
      { page: RAW, title: "Kit raw" },
      { page: SUBPAGE_LINK, title: "Kit sub-page link" },
    ],
    start: (context) => {
      context.publish("channel", INDEX, index());
      context.publishSubpages(
        "channel",
        CAROUSEL,
        SUBPAGES.map((number) => ({ screen: subpage(number), subpage: number })),
      );
      context.publishTti("channel", RAW, RAW_TTI);
      context.publishTti("channel", SUBPAGE_LINK, SUBPAGE_LINK_TTI);
    },
  },
]);
