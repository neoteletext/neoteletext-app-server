// Magazine 2's demonstrations of the library's rich text (204) and its
// badges, notices and toasts (205).

import { Colour } from "../kit.ts";
import { Badges, Button, Columns, Component, type Child, Gap, h, Notice, type PageSpec, Paragraph, RichText, Rows, ToastShelf } from "../../../../kits/typescript-components/src/index.ts";
import { frame, INDEX, LOOK, prompts } from "../look.ts";

export const RICH = 0x204;
export const BADGES = 0x205;

/** Buttons that raise a toast of each kind for everyone on the page, through the bus. */
class Toaster extends Component {
  override render(): Child {
    const kinds = [
      ["info", "Copied to the clipboard"],
      ["success", "Saved"],
      ["warning", "Low on space"],
      ["error", "Could not reach the server"],
    ] as const;
    const button = ([kind, text]: (typeof kinds)[number]): Child =>
      h(Button, { key: kind, shared: true, label: kind, background: kind === "error" ? Colour.Red : kind === "warning" ? Colour.Yellow : kind === "success" ? Colour.Green : Colour.Cyan, onPress: () => this.services.bus.emit("toast", { kind, text }) });
    return h(Rows, {}, h(Columns, { height: 1 }, button(kinds[0]), button(kinds[1])), h(Columns, { height: 1 }, button(kinds[2]), button(kinds[3])));
  }
}

export function textPages(): PageSpec[] {
  return [
    {
      page: RICH,
      title: "Rich text",
      root: () =>
        frame(
          LOOK.text,
          "Rich text",
          prompts(["Index", INDEX], ["Badges", BADGES], ["Text", 0x200], ["Headlines", 0x201]),
          "Press Reveal to see the hidden word",
          h(RichText, {
            runs: [
              { text: "Runs of their own styles:" },
              { text: "yellow,", foreground: Colour.Yellow },
              { text: "cyan,", foreground: Colour.Cyan },
              { text: "green", foreground: Colour.Green },
              { text: "and back to white. A change of colour takes the space between words." },
            ],
          }),
          h(Gap, {}),
          h(RichText, {
            runs: [
              { text: "A background" },
              { text: "of its own", foreground: Colour.Black, background: Colour.Yellow },
              { text: "takes three cells, and its end one." },
            ],
          }),
          h(Gap, {}),
          h(RichText, {
            runs: [
              { text: "This is" },
              { text: "flashing", foreground: Colour.Red, flash: true },
              { text: "and this is" },
              { text: "hidden", foreground: Colour.Magenta, conceal: true },
              { text: "until Reveal." },
            ],
          }),
          h(Gap, {}),
          h(RichText, {
            runs: [
              { text: "Wrapping counts every attribute's cell:", foreground: Colour.Cyan },
              { text: "red", foreground: Colour.Red },
              { text: "green", foreground: Colour.Green },
              { text: "yellow", foreground: Colour.Yellow },
              { text: "blue", foreground: Colour.Blue },
              { text: "magenta", foreground: Colour.Magenta },
              { text: "cyan", foreground: Colour.Cyan },
              { text: "white", foreground: Colour.White },
              { text: "on blue,", foreground: Colour.White, background: Colour.Blue },
              { text: "on red.", foreground: Colour.Yellow, background: Colour.Red },
            ],
          }),
        ),
    },
    {
      page: BADGES,
      title: "Badges, notices, toasts",
      root: () =>
        frame(
          LOOK.text,
          "Badges, notices, toasts",
          prompts(["Index", INDEX], ["Rich text", RICH], ["Text", 0x200], ["Headlines", 0x201]),
          "Click a button: everyone here sees the toast",
          h(Badges, { badges: [{ text: "LIVE", background: Colour.Red, flash: true }], text: "Badges in a line of text" }),
          h(Badges, { badges: [{ text: "NEW", background: Colour.Green, foreground: Colour.Black }, { text: "3", background: Colour.Blue }], text: "unread" }),
          h(Badges, { badges: [{ text: "BETA", background: Colour.Magenta }, { text: "2.5", background: Colour.Cyan, foreground: Colour.Blue }] }),
          h(Gap, {}),
          h(Notice, { kind: "info", text: "An update is ready" }),
          h(Notice, { kind: "success", text: "Saved" }),
          h(Notice, { kind: "warning", text: "The connection is slow" }),
          h(Notice, { kind: "error", text: "The file could not be written" }),
          h(Gap, {}),
          h(Paragraph, { text: "Toasts go after four seconds:", foreground: Colour.Cyan }),
          h(Toaster, {}),
          h(Gap, {}),
          h(ToastShelf, { maxShown: 3 }),
        ),
    },
  ];
}
