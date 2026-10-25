// Page 108, about NeoTeletext, and page 109, facts about teletext: two
// carousels that never change, so they are published once
// (crates/teletext-showcase/src/services/about.rs). The facts are kept to
// what is well established about teletext and its standard, ETSI EN 300
// 706.

import { page } from "../common.ts";
import { type Application, Colour, type Context, type PageEntry, Screen } from "../kit.ts";
import { FIRST_ROW, counter, next, paragraph, publishCarousel } from "./draw.ts";
import { ABOUT, FACTS, navigation } from "./pages.ts";

/** A sub-page of text: its heading and its paragraphs. */
interface Topic {
  readonly heading: string;
  readonly paragraphs: readonly string[];
}

/** Page 108's sub-pages. */
const ABOUT_TOPICS: readonly Topic[] = [
  {
    heading: "Teletext for the web",
    paragraphs: [
      "NeoTeletext brings teletext to the web: channels of pages, pushed live to a teletext browser as broadcasters once sent them over the air.",
      "It has three parts: the engine, the browser and the server. The next sub-pages take each in turn.",
      "Teletext is push: a page arrives when it changes, whether or not you are looking at it.",
    ],
  },
  {
    heading: "The engine",
    paragraphs: [
      "The engine decodes and renders teletext as ETSI EN 300 706 defines it, at Levels 1, 1.5, 2.5 and 3.5.",
      "It reads T42 packet files, transport streams, sliced VBI and TTI page files, and carries the input protocol and sound.",
      "It is written in Rust, and its core libraries use no floating point.",
    ],
  },
  {
    heading: "The browser",
    paragraphs: [
      "A desktop browser for teletext channels on the web, for macOS, Linux and Windows, built on the engine.",
      "Its parts run as separate processes, each confined by the operating system. It has no script engine.",
      "Tabs, history, bookmarks, sound, and a phone as a keyboard.",
    ],
  },
  {
    heading: "The server",
    paragraphs: [
      "Applications publish pages; the server sends each viewer the pages as 42-byte packets over a WebSocket.",
      "A shared page is encoded once for every viewer. A viewer's own page, such as a game, goes to that viewer alone.",
      "Its runtime does no input or output itself, so one code runs natively and builds for WebAssembly.",
    ],
  },
];

/** Page 109's sub-pages. */
const FACTS_TOPICS: readonly Topic[] = [
  {
    heading: "Ceefax, 1974 to 2012",
    paragraphs: [
      "Ceefax, the BBC's teletext service, began in 1974. ITV's service, ORACLE, also began in the 1970s.",
      "Ceefax closed in 2012, when the United Kingdom switched off analogue television.",
      "Teletext is still broadcast in several European countries, Germany among them.",
    ],
  },
  {
    heading: "Between the pictures",
    paragraphs: [
      "Teletext travels in the vertical blanking interval: lines of each television frame that carry no picture.",
      "A receiver picks the pages out of that stream as they pass.",
    ],
  },
  {
    heading: "Forty by twenty-four",
    paragraphs: [
      "A page is 40 characters wide and 24 rows deep, the first row being the header: the page number, the service's name and usually the time.",
      "A row below them, row 24, carries Fastext prompts.",
    ],
  },
  {
    heading: "Pages 100 to 899",
    paragraphs: [
      "Pages are numbered from 100 to 899. The first digit is the magazine, one of eight.",
      "The other two digits are sent in hexadecimal, so some pages, such as 1FF, cannot be keyed in on a remote control.",
    ],
  },
  {
    heading: "The carousel",
    paragraphs: [
      "Pages are sent one after another, round and round. A receiver waits for the page asked for to come round.",
      "A page can have sub-pages, sent in turn, which the receiver shows as they arrive.",
    ],
  },
  {
    heading: "Mosaic graphics",
    paragraphs: [
      "Pictures are drawn with mosaic characters, which divide a character cell into six blocks, two across and three down.",
      "Every map and chart on these pages is drawn that way.",
    ],
  },
  {
    heading: "Eight colours",
    paragraphs: [
      "Level 1 has eight colours: black, red, green, yellow, blue, magenta, cyan and white.",
      "Attributes add flash, double height, conceal, and graphics that are contiguous or separated.",
    ],
  },
  {
    heading: "Fastext",
    paragraphs: ["Fastext gives four coloured keys, linked to pages the editor chooses.", "A receiver can fetch the linked pages ahead, so they show at once."],
  },
  {
    heading: "Checked as it goes",
    paragraphs: [
      "Characters are sent with odd parity, so a receiver can spot a damaged one.",
      "Page addresses and headers are Hamming coded, so a single wrong bit is corrected.",
    ],
  },
];

/** The column of a heading's double-height attribute. */
const HEADING_COLUMN = 1;
/** The heading's row, a row below the title band. */
const HEADING_ROW = FIRST_ROW + 1;
/** The first paragraph's row, below the double-height heading and a gap. */
const TEXT_ROW = FIRST_ROW + 4;
/** The colours of a topic's paragraphs, in turn. */
const PARAGRAPH_COLOURS = [Colour.White, Colour.Cyan] as const;

/** Draws `topics` as sub-pages under `title`. */
function carousel(number: number, title: string, band: Colour, topics: readonly Topic[]): Screen[] {
  return topics.map((topic, index) => {
    const screen = page(title, band, navigation(number));
    counter(screen, index + 1, topics.length);
    screen.doubleHeight(HEADING_ROW, HEADING_COLUMN, Colour.Yellow, topic.heading);
    let row = TEXT_ROW;
    topic.paragraphs.forEach((text, place) => {
      row = paragraph(screen, row, PARAGRAPH_COLOURS[place % PARAGRAPH_COLOURS.length] ?? Colour.White, text) + 1;
    });
    // The last sub-page leads back to the first.
    next(screen, (topics[index + 1] ?? topics[0])?.heading ?? "");
    return screen;
  });
}

/** Page 108's sub-pages. */
export function aboutScreens(): Screen[] {
  return carousel(ABOUT, "About NeoTeletext", Colour.Magenta, ABOUT_TOPICS);
}

/** Page 109's sub-pages. */
export function factsScreens(): Screen[] {
  return carousel(FACTS, "Teletext facts", Colour.Green, FACTS_TOPICS);
}

/** The two pages. */
export class About implements Application {
  pages(): readonly PageEntry[] {
    return [
      { page: ABOUT, title: "About NeoTeletext" },
      { page: FACTS, title: "Teletext facts" },
    ];
  }

  start(context: Context): void {
    publishCarousel(context, ABOUT, aboutScreens());
    publishCarousel(context, FACTS, factsScreens());
  }
}
