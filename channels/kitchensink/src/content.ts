// The kitchensink's sample articles: made up for the demonstration, about
// teletext and the hardware behind it, so nothing of anyone's site or
// recordings is in the repository (the owner's rule). Each has a page of
// its own in magazine 8 (pages.ts).

import type { Article, Block } from "../../../kits/typescript-components/src/index.ts";

/** The first article's page; the rest follow it, counting in decimal, as a viewer keys pages. */
export const FIRST_ARTICLE_PAGE = 810;

/** Page `decimal` (such as 820) as its page number, whose digits are hexadecimal digits (0x820). */
function pageOfDecimal(decimal: number): number {
  return Number.parseInt(String(decimal), 16);
}
/** The sample's newest article's time: Monday 26 October 2026, 08:00 UTC. */
const NEWEST = Date.UTC(2026, 9, 26, 8, 0);
/** Articles come about every five hours. */
const APART_MS = 5 * 60 * 60 * 1_000;

interface Draft {
  readonly title: string;
  readonly writer: string;
  readonly summary: string;
  readonly tags: readonly string[];
  readonly body: readonly (string | readonly ["heading" | "quote" | "item", string])[];
}

const DRAFTS: readonly Draft[] = [
  {
    title: "A Teletext Inserter Built From A Single Microcontroller",
    writer: "Ada Fernsby",
    summary: "One small chip times its output to the video lines and squeezes a whole magazine into the blanking interval.",
    tags: ["teletext", "microcontroller", "video"],
    body: [
      "Teletext rides in the lines of a television picture that nobody sees: the vertical blanking interval. An inserter puts it there, and for decades that meant a rack of equipment.",
      ["heading", "Counting lines"],
      "This build uses one microcontroller to count the video lines, wait for the right one, and clock out 45 bytes at 6.9375 megabits a second. The timing is the hard part; the data is easy.",
      ["quote", "The first time the header clock ticked on a real set, I laughed out loud."],
      "A magazine of a hundred pages takes about twenty seconds to cycle, so the page you ask for arrives when its turn comes round.",
      ["item", "One microcontroller and a video sync separator"],
      ["item", "A resistor ladder for the levels"],
      ["item", "Pages loaded from a memory card as TTI files"],
    ],
  },
  {
    title: "Restoring A Ceefax Adapter With Its Original Decoder Chip",
    writer: "Tom Hallett",
    summary: "A forty-year-old adapter gets new capacitors and shows pages again.",
    tags: ["retro", "repair", "teletext"],
    body: [
      "The adapter arrived dead: no picture, no pages, a smell of old electrolytics.",
      ["heading", "Recapping"],
      "Every capacitor came out. The decoder chip itself was fine, which is lucky, because there are not many left.",
      "With the supply rails back in tolerance the adapter locked to a test signal at once and drew a page of mosaics.",
    ],
  },
  {
    title: "Mosaic Graphics: Why Six Blocks To A Cell",
    writer: "Priya Raman",
    summary: "The two-by-three grid of teletext graphics was a careful compromise between memory and shape.",
    tags: ["teletext", "graphics", "history"],
    body: [
      "A teletext cell is a character or a block of six sixels, two across and three down. Sixty-four patterns fit in the codes left over after the letters.",
      ["heading", "Contiguous and separated"],
      "Separated graphics leave a gap round each sixel, which made smooth shapes look like a grid but kept text-heavy pages crisp.",
      ["item", "64 patterns: every combination of six sixels"],
      ["item", "Hold mosaics repeats the last block under an attribute"],
      "Clever artists used both, and the results still look good on a modern screen.",
    ],
  },
  {
    title: "A Weather Station That Publishes Its Own Teletext Page",
    writer: "Ada Fernsby",
    summary: "Temperature, wind and rain, drawn as a page with a bar chart and refreshed every minute.",
    tags: ["weather", "sensors", "teletext"],
    body: [
      "Weather was always one of the busiest pages on a teletext service, so it seemed right that a weather station should have one.",
      "The station reads its sensors every minute and redraws the page: the figures in large digits, the day's temperature as a bar chart.",
      ["quote", "A page that changes is a page people come back to."],
    ],
  },
  {
    title: "Typing Back: Interactive Pages Over A Return Channel",
    writer: "Sam Okafor",
    summary: "Teletext was a broadcast, one way; a return channel turns a page into something you can type into.",
    tags: ["interaction", "network", "teletext"],
    body: [
      "Broadcast teletext could not hear you. Viewdata could, over a telephone line, and the two ideas met in a few services that never quite caught on.",
      ["heading", "The cursor"],
      "Here the page says where the cursor goes and how wide the field is, and the receiver echoes what is typed there before the channel answers.",
      "Search, quizzes and polls all come from that one mechanism.",
    ],
  },
  {
    title: "Driving A Flip-Dot Sign From A Teletext Page",
    writer: "Lena Marsh",
    summary: "Each sixel becomes a dot that flips with a satisfying click.",
    tags: ["displays", "flip-dot", "teletext"],
    body: [
      "Flip-dot signs and mosaic graphics are close cousins: both are grids of on and off.",
      "Mapping a page's sixels to the sign's dots took an afternoon; tuning the timing so the dots did not stick took a week.",
    ],
  },
  {
    title: "Ask The Readers: What Was On Your Favourite Page?",
    writer: "Priya Raman",
    summary: "Football results, the weather, the subtitles: we want to hear which page you knew by heart.",
    tags: ["community", "history"],
    body: [
      "Everyone of a certain age knows a page number by heart. For some it was the football; for others the subtitles on 888.",
      "Tell us yours, and why it stuck.",
    ],
  },
  {
    title: "Building A QR Code Out Of Teletext Mosaics",
    writer: "Sam Okafor",
    summary: "A QR code's modules map neatly onto sixels, so a phone can follow a link from the screen.",
    tags: ["qr", "graphics", "phones"],
    body: [
      "A QR code is a grid of dark and light modules. Two modules across and three down make a cell of mosaic graphics.",
      "The quiet zone round the code matters: without it a phone camera will not find the code at all.",
      ["item", "Version 3 codes fit in fourteen rows"],
      ["item", "White on black reads as well as black on white on most phones"],
    ],
  },
  {
    title: "A Synthesiser That Plays The Tones Of A Teletext Header",
    writer: "Lena Marsh",
    summary: "Turning the bytes of a page header into notes makes a surprisingly catchy tune.",
    tags: ["audio", "music", "synth"],
    body: [
      "Each byte of a header picks a note from a scale; the clock's digits pick the rhythm.",
      "The result changes every second and never quite repeats.",
    ],
  },
  {
    title: "Fail Of The Week: The Page That Would Not Stop Flashing",
    writer: "Tom Hallett",
    summary: "One missing Steady attribute turned a whole page into a strobe.",
    tags: ["fail", "teletext", "attributes"],
    body: [
      "Flash is an attribute, and attributes last to the end of the row unless something ends them.",
      "Forget the Steady after a flashing word and everything after it flashes too, including, in this case, the phone number for complaints.",
      ["quote", "We had a lot of complaints, though not through the number."],
    ],
  },
  {
    title: "Snake On A Teletext Page, Ten Frames A Second",
    writer: "Ada Fernsby",
    summary: "Each frame is a whole page sent to one viewer; at ten a second it is just about playable.",
    tags: ["games", "teletext", "animation"],
    body: [
      "Teletext was never meant for games, but a page is only a kilobyte, and a kilobyte ten times a second is nothing to a modern connection.",
      "The snake is sixels; the food is a sixel; the walls are the edges of the page.",
    ],
  },
  {
    title: "Live Polls: When Every Viewer Sees The Votes Change",
    writer: "Sam Okafor",
    summary: "A shared page updates for everyone the moment anyone votes.",
    tags: ["interaction", "polls"],
    body: [
      "A poll is the simplest shared state there is: a count for each option.",
      "Each viewer's copy of the page marks their own vote; the counts are everyone's.",
    ],
  },
];

/** The sample articles, newest first, each with its page and its time. */
export const ARTICLES: readonly Article[] = DRAFTS.map((draft, index) => ({
  page: pageOfDecimal(FIRST_ARTICLE_PAGE + index),
  title: draft.title,
  writer: draft.writer,
  date: NEWEST - index * APART_MS,
  summary: draft.summary,
  tags: draft.tags,
  link: `https://example.org/kitchensink/${index + 1}`,
  blocks: [
    { kind: "paragraph", text: draft.summary },
    ...draft.body.map((part): Block => (typeof part === "string" ? { kind: "paragraph", text: part } : { kind: part[0], text: part[1] })),
  ],
}));
