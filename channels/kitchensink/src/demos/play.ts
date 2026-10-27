// Magazine 7's demonstrations of the library's runtime: a live stream of
// sound longer than a stored sound may be (703), and pages opened with
// something to show (704 and 705), the router a site of many articles
// reads one by.

import { Colour } from "../kit.ts";
import {
  Button,
  type Child,
  ChoiceList,
  Columns,
  Component,
  DoubleHeight,
  Gap,
  h,
  NowPlaying,
  AudioPlayer,
  type PageSpec,
  Panel,
  Paragraph,
  type StreamSource,
} from "../../../../kits/typescript-components/src/index.ts";
import { frame, INDEX, LOOK, prompts } from "../look.ts";

export const STREAMS = 0x703;
export const OPEN_WITH = 0x704;
export const OPENED = 0x705;

/** The stream's rate: what the showcase radio and the Hackaday clips use. */
const RATE = 16_000;
/** A note's length and loudness. */
const NOTE_SECONDS = 0.25;
const LOUDNESS = 6_000;
/** A C major arpeggio, up and down, in hertz. */
const ARPEGGIO = [262, 330, 392, 523, 392, 330] as const;

/** `seconds` of an arpeggio, each note faded in and out so it does not click. */
function tune(seconds: number): Int16Array {
  const samples = new Int16Array(Math.round(seconds * RATE));
  const noteSamples = Math.round(NOTE_SECONDS * RATE);
  for (let index = 0; index < samples.length; index += 1) {
    const note = ARPEGGIO[Math.floor(index / noteSamples) % ARPEGGIO.length] ?? 0;
    const within = (index % noteSamples) / noteSamples;
    const envelope = Math.min(1, within * 20, (1 - within) * 20);
    samples[index] = Math.round(LOUDNESS * envelope * Math.sin((2 * Math.PI * note * index) / RATE));
  }
  return samples;
}

function source(name: string, samples: Int16Array): StreamSource {
  return { name, rate: RATE, samples: () => samples, complete: true, failure: null, expected: samples.length };
}

/** Three minutes: longer than a stored sound's two. */
const LONG_SECONDS = 180;
const SHORT_SECONDS = 4;

class Streams extends Component {
  override render(): Child {
    return [
      h(Paragraph, { text: "A stored sound is at most two minutes. A live stream is sent as it plays, a few blocks ahead, so it can be any length; leaving the page stops it.", foreground: Colour.White }),
      h(Gap, {}),
      h(
        Columns,
        { height: 1 },
        h(Button, { label: "Three minutes", background: Colour.Magenta, onPress: (session: number) => this.services.stream(source("Three minutes of arpeggio", tune(LONG_SECONDS)), session) }),
        h(Button, { label: "Four seconds", background: Colour.Green, onPress: (session: number) => this.services.stream(source("Four seconds", tune(SHORT_SECONDS)), session) }),
      ),
      h(Gap, {}),
      h(Button, { label: "Stop", background: Colour.White, onPress: (session: number) => this.services.stopStream(session) }),
      h(Gap, {}),
      h(NowPlaying, {}),
      h(Gap, {}),
      h(Paragraph, { text: "The player: the same stream, to pause and stop:", foreground: Colour.Cyan }),
      h(AudioPlayer, { foreground: Colour.Cyan }),
    ];
  }
}

const COLOURS = [
  ["Red", Colour.Red],
  ["Green", Colour.Green],
  ["Yellow", Colour.Yellow],
  ["Blue", Colour.Blue],
  ["Magenta", Colour.Magenta],
  ["Cyan", Colour.Cyan],
  ["White", Colour.White],
] as const;

class OpenWith extends Component {
  override render(): Child {
    return [
      h(Paragraph, { text: "Choose a colour: page 705 opens showing it. One page shows any of many things, as a reader shows any article: goto carries what to show.", foreground: Colour.White }),
      h(Gap, {}),
      h(ChoiceList, { items: COLOURS.map(([name]) => name), shown: 7, onChoose: (index: number, session: number) => this.services.goto(session, OPENED, { colour: index }) }),
    ];
  }
}

export function playPages(): PageSpec[] {
  return [
    { page: STREAMS, title: "Live streams", root: () => frame(LOOK.play, "Live streams", prompts(["Index", INDEX], ["Open with", OPEN_WITH], ["Sounds", 0x701], ["Roulette", 0x700]), undefined, h(Streams, {})) },
    { page: OPEN_WITH, title: "Pages opened with", root: () => frame(LOOK.play, "Pages opened with", prompts(["Index", INDEX], ["Streams", STREAMS], ["Opened", OPENED], ["Roulette", 0x700]), undefined, h(OpenWith, {})) },
    {
      page: OPENED,
      title: "Opened with a colour",
      perViewer: true,
      root: (params) => {
        const [name, colour] = COLOURS[Number(params?.["colour"] ?? 0)] ?? COLOURS[0];
        return frame(
          LOOK.play,
          "Opened with a colour",
          prompts(["Index", INDEX], ["Choose", OPEN_WITH], ["Streams", STREAMS], ["Roulette", 0x700]),
          undefined,
          h(DoubleHeight, { text: params === undefined ? "Opened with nothing" : `Opened with ${name}`, foreground: colour }),
          h(Gap, {}),
          h(Panel, { style: "band", foreground: colour, title: name, titleForeground: colour === Colour.White || colour === Colour.Yellow || colour === Colour.Cyan ? Colour.Blue : Colour.White }, h(Gap, { rows: 6 })),
        );
      },
    },
  ];
}
