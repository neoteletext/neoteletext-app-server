// Magazine 1's demonstrations of the library's layout and frame parts:
// panels and rules (104), status bars and key help (105), centred stacks
// (106).

import { Colour } from "../kit.ts";
import { Button, Centred, type Child, Columns, Component, DATE_FORMATS, formatDate, Gap, h, KeyHelp, Panel, Paragraph, Rule, StatusBar, Text, type PageSpec } from "../../../../kits/typescript-components/src/index.ts";
import { frame, INDEX, LOOK, prompts } from "../look.ts";

export const PANELS = 0x104;
export const BARS = 0x105;
export const CENTRED = 0x106;

const SECOND_MS = 1_000;

/** Buttons stacked in the middle, a blank row above each, saying which was pressed. */
class CentredButtons extends Component<object, { readonly pressed: string }> {
  override state = { pressed: "nothing yet" };

  override render(): Child {
    const press = (label: string): ((session: number) => void) => () => this.setState({ pressed: label });
    return [
      h(
        Centred,
        {},
        h(Gap, {}),
        h(Button, { label: "The challenge", background: Colour.Green, onPress: press("The challenge") }),
        h(Gap, {}),
        h(Button, { label: "The reveal", background: Colour.Yellow, onPress: press("The reveal") }),
        h(Gap, {}),
        h(Button, { label: "Just the sound", background: Colour.Magenta, onPress: press("Just the sound") }),
      ),
      h(Gap, {}),
      h(Text, { text: `Pressed: ${this.state.pressed}`, centre: true, foreground: Colour.Cyan }),
    ];
  }
}

/** A status bar with the time on the right, a second at a time. */
class LiveStatus extends Component<object, { readonly now: number }> {
  override state = { now: 0 };

  override componentDidMount(): void {
    const tick = (): void => this.setState({ now: this.services.epochMs() });
    tick();
    this.setInterval(tick, SECOND_MS);
  }

  override render(): Child {
    const time = formatDate(this.state.now, DATE_FORMATS.seconds);
    return h(StatusBar, { segments: [{ text: "ON AIR", foreground: Colour.Green }, { text: "3 viewers", foreground: Colour.White }, { text: "level 2.5", foreground: Colour.Cyan }], right: time, background: Colour.Blue });
  }
}

export function layoutPages(): PageSpec[] {
  return [
    {
      page: PANELS,
      title: "Panels and rules",
      root: () =>
        frame(
          LOOK.home,
          "Panels and rules",
          prompts(["Index", INDEX], ["Bars", BARS], ["Components", 0x101], ["Text", 0x200]),
          undefined,
          h(Rule, { style: "thin", title: "thin" }),
          h(Rule, { style: "bold", foreground: Colour.Cyan, title: "bold" }),
          h(Rule, { style: "double", foreground: Colour.Green, title: "double" }),
          h(Rule, { style: "dotted", foreground: Colour.Yellow, title: "dotted: separated" }),
          h(Rule, { style: "dashed", foreground: Colour.Magenta }),
          h(Panel, { title: "A line panel", foreground: Colour.Cyan }, h(Paragraph, { text: "Its frame is mosaic sixels: a colour and an edge cell each side of every row.", foreground: Colour.White })),
          h(Panel, { title: "A band panel", style: "band", foreground: Colour.Blue, titleForeground: Colour.Yellow }, h(Paragraph, { text: "A background of its own: two cells before the text.", foreground: Colour.White })),
          h(
            Columns,
            { height: 4 },
            h(Panel, { title: "Left", foreground: Colour.Green }, h(Paragraph, { text: "Side by side", foreground: Colour.Green })),
            h(Panel, { title: "Right", foreground: Colour.Magenta }, h(Paragraph, { text: "in columns", foreground: Colour.Magenta })),
          ),
        ),
    },
    {
      page: BARS,
      title: "Status bars and key help",
      root: () =>
        frame(
          LOOK.home,
          "Status bars, key help",
          prompts(["Index", INDEX], ["Panels", PANELS], ["Centred", CENTRED], ["Text", 0x200]),
          undefined,
          h(StatusBar, { segments: [{ text: "READY" }, { text: "12 items", foreground: Colour.Cyan }], right: "UTF-8" }),
          h(Gap, {}),
          h(LiveStatus, {}),
          h(Gap, {}),
          h(StatusBar, { segments: [{ text: "ERROR: connection lost", foreground: Colour.Yellow }], right: "F1 Help", background: Colour.Red, alarm: true }),
          h(Gap, {}),
          h(Paragraph, { text: "Key help, one column", foreground: Colour.Cyan }),
          h(KeyHelp, {
            keys: [
              ["Up Down", "Move"],
              ["Enter", "Choose"],
              ["Escape", "Back"],
              ["Tab", "Next control"],
            ],
          }),
          h(Gap, {}),
          h(Paragraph, { text: "Key help, two columns", foreground: Colour.Cyan }),
          h(KeyHelp, {
            columns: 2,
            keys: [
              ["Red", "Back"],
              ["Green", "Next"],
              ["Yellow", "Help"],
              ["Cyan", "Index"],
              ["Reveal", "Answer"],
              ["Hold", "Stop"],
            ],
          }),
        ),
    },
    {
      page: CENTRED,
      title: "Centred stacks",
      root: () =>
        frame(
          LOOK.home,
          "Centred stacks",
          prompts(["Index", INDEX], ["Panels", PANELS], ["Bars", BARS], ["Components", 0x101]),
          undefined,
          h(Centred, {}, h(Text, { text: "Each child at its own width", foreground: Colour.Yellow }), h(Text, { text: "in the middle", foreground: Colour.White }), h(Text, { text: "of the row", foreground: Colour.Cyan })),
          h(Gap, {}),
          h(CentredButtons, {}),
        ),
    },
  ];
}
