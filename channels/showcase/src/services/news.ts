// Page 101: NeoTeletext's own news, as a carousel: the headlines on
// sub-page 1 and a story on each sub-page after it
// (crates/teletext-showcase/src/services/news.rs). The stories are about
// the project and say only what it does.

import { page } from "../common.ts";
import { type Application, Colour, type Context, type PageEntry, Screen } from "../kit.ts";
import { BODY_COLUMN, FIRST_ROW, counter, next, paragraph, publishCarousel, wrap } from "./draw.ts";
import { NEWS, navigation } from "./pages.ts";

/** A story: its headline and its paragraphs. */
interface Story {
  readonly headline: string;
  readonly paragraphs: readonly string[];
}

/** The stories, in the order of their sub-pages from 2. */
const STORIES: readonly Story[] = [
  {
    headline: "Teletext server goes live",
    paragraphs: [
      "The NeoTeletext server is publishing its first channel: the pages you are reading now.",
      "Applications draw pages as a terminal program draws its screen, and the server sends them to the browser over a WebSocket as 42-byte teletext packets.",
    ],
  },
  {
    headline: "One page, any number of viewers",
    paragraphs: [
      "A page the channel shares is encoded once, and the same bytes go to every viewer.",
      "A slow connection skips to each page's latest version, so no queue grows and memory stays bounded. Scale is measured with a load client, not assumed.",
    ],
  },
  {
    headline: "Every part in its own sandbox",
    paragraphs: [
      "The NeoTeletext browser runs as separate processes: a trusted core, a decoder, a fetcher, an interface and a keyboard.",
      "Each is confined by the operating system on macOS, Linux and Windows. There is no script engine: a page is teletext, never a program.",
    ],
  },
  {
    headline: "Type on your phone",
    paragraphs: [
      "Pages that take input can be typed into from a phone: the browser shows a QR code, and the phone becomes its keyboard.",
      "Text, keys and the cursor travel in the input protocol, which the engine defines and the server reads.",
    ],
  },
  {
    headline: "Teletext with sound",
    paragraphs: [
      "The engine carries sound in teletext packets: short sound objects sent once, then commands to play them, and live audio for radio.",
      "The browser plays it on every desktop. The games and the sound pages of this channel use it.",
    ],
  },
  {
    headline: "One runtime for every host",
    paragraphs: [
      "The server's runtime reads no clock and opens no socket: the host gives it each event with the time, and it answers with what to send.",
      "So the same code runs natively under tokio and builds for WebAssembly, and its tests run with no network.",
    ],
  },
];

/** The title band. */
const TITLE = "NeoTeletext News";
/** The headline's width: the double-height colour and height attributes take two cells after the margin. */
const HEADLINE_WIDTH = 36;
/** The headlines index's rows per story: its line and a gap. */
const INDEX_STEP = 2;
/** The column of a headline in the index, after its sub-page number. */
const INDEX_TEXT_COLUMN = 5;
/** The width of a headline in the index. */
const INDEX_WIDTH = 33;
/** The first headline's row in the index, below its double-height heading. */
const INDEX_FIRST_ROW = FIRST_ROW + 3;
/** A double-height line's rows. */
const DOUBLE_HEIGHT_ROWS = 2;
/** The colours of a story's paragraphs, in turn. */
const PARAGRAPH_COLOURS = [Colour.White, Colour.Cyan] as const;

/** The carousel's sub-pages: the headlines, then each story. */
export function screens(): Screen[] {
  const count = STORIES.length + 1;
  const index = page(TITLE, Colour.Blue, navigation(NEWS));
  counter(index, 1, count);
  index.doubleHeight(FIRST_ROW, BODY_COLUMN, Colour.Yellow, "Headlines");
  let row = INDEX_FIRST_ROW;
  STORIES.forEach((story, offset) => {
    index.coloured(row, BODY_COLUMN, Colour.Cyan, `${offset + 2}/${count}`);
    // Headlines are written to fit one line; a longer one is cut.
    index.coloured(row, INDEX_TEXT_COLUMN, Colour.White, wrap(story.headline, INDEX_WIDTH)[0] ?? "");
    row += INDEX_STEP;
  });
  index.coloured(row + 1, BODY_COLUMN, Colour.Green, "The stories follow as sub-pages.");
  const stories = STORIES.map((story, offset) => {
    const subpage = offset + 2;
    const screen = page(TITLE, Colour.Blue, navigation(NEWS));
    counter(screen, subpage, count);
    let at = FIRST_ROW + 1;
    for (const line of wrap(story.headline, HEADLINE_WIDTH)) {
      screen.doubleHeight(at, BODY_COLUMN, Colour.Yellow, line);
      at += DOUBLE_HEIGHT_ROWS;
    }
    at += 1;
    story.paragraphs.forEach((text, number) => {
      at = paragraph(screen, at, PARAGRAPH_COLOURS[number % PARAGRAPH_COLOURS.length] ?? Colour.White, text) + 1;
    });
    next(screen, STORIES[subpage - 1]?.headline ?? "Headlines");
    return screen;
  });
  return [index, ...stories];
}

/** The news carousel; it never changes, so it is published once. */
export class News implements Application {
  pages(): readonly PageEntry[] {
    return [{ page: NEWS, title: "NeoTeletext News" }];
  }

  start(context: Context): void {
    publishCarousel(context, NEWS, screens());
  }
}
