// Magazine 5's demonstration of the library's icons (506): every icon in
// its own colour, then a weather forecast as teletext had it, icons in
// columns with their figures; and of a pointer area (507): the whole page
// one, so the channel hears every move over it and marks where the pointer
// is.

import { Colour } from "../kit.ts";
import { type Canvas, Columns, Component, Gap, h, Icon, ICONS, type IconName, type PageSpec, Paragraph, type PointerEvent, Rows, type Size, wrap } from "../../../../kits/typescript-components/src/index.ts";
import { frame, INDEX, LOOK, prompts } from "../look.ts";

export const ICONS_PAGE = 0x506;
export const POINTER_PAGE = 0x507;

/** What the page does, under where the pointer is; the mark goes on the rows below. */
const POINTER_TEXT = "The whole page is a pointer area: every move over it comes to the channel, which marks the cell. Elsewhere a move comes only while a button is held.";

interface PointerState {
  /** The cell under the pointer, on the page; null until it moves. */
  readonly at: { readonly row: number; readonly column: number } | null;
}

/** Where the pointer is, anywhere on the page, from a whole-page pointer area: said on its first row, marked "+" in its own cells below its text. */
class PointerWatch extends Component<object, PointerState> {
  override state: PointerState = { at: null };

  override measure(space: Size): Size {
    return space;
  }

  override paint(canvas: Canvas): void {
    canvas.wholePagePointerArea();
    const { at } = this.state;
    canvas.coloured(0, 0, Colour.Yellow, at === null ? "Move the pointer over the page" : `The pointer is on row ${at.row}, column ${at.column}`);
    const lines = wrap(POINTER_TEXT, canvas.width - 1);
    lines.forEach((line, row) => canvas.coloured(1 + row, 0, Colour.Cyan, line));
    // In its own cells, below the text: the row's first cell is white text already, so "+" needs no colour of its own.
    const row = at === null ? -1 : at.row - canvas.box.row;
    if (at !== null && row > lines.length) {
      canvas.text(row, at.column - canvas.box.column, "+");
    }
  }

  override onPointer(event: PointerEvent): boolean {
    // A move with no button held, from the pointer area; a new cell is a new page, the same cell nothing.
    if (event.action === "move" && (event.row !== this.state.at?.row || event.column !== this.state.at.column)) {
      this.setState({ at: { row: event.row, column: event.column } });
    }
    return true;
  }
}

const NAMES = Object.keys(ICONS) as IconName[];
/** An icon and its name take a column of eight cells. */
const ICON_COLUMN = 8;
const PER_ROW = 5;

export function graphicsPages(): PageSpec[] {
  const rows: IconName[][] = [];
  for (let start = 0; start < NAMES.length; start += PER_ROW) {
    rows.push(NAMES.slice(start, start + PER_ROW));
  }
  const forecast = [
    ["Mon", "sun", "21"],
    ["Tue", "cloud", "18"],
    ["Wed", "rain", "15"],
    ["Thu", "storm", "14"],
    ["Fri", "snow", "2"],
  ] as const;
  return [
    {
      page: POINTER_PAGE,
      title: "Pointer areas",
      perViewer: true,
      root: () => frame(LOOK.graphics, "Pointer areas", prompts(["Index", INDEX], ["Mosaics", 0x500], ["Paint", 0x504], ["Icons", ICONS_PAGE]), "Move the pointer: no button needed", h(PointerWatch, {})),
    },
    {
      page: ICONS_PAGE,
      title: "Icons",
      root: () =>
        frame(
          LOOK.graphics,
          "Icons",
          prompts(["Index", INDEX], ["Mosaics", 0x500], ["Paint", 0x504], ["Animation", 0x505]),
          undefined,
          ...rows.map((names, row) =>
            h(
              Columns,
              { key: `row-${row}`, height: 3, widths: names.map(() => ICON_COLUMN) },
              ...names.map((name) => h(Rows, { key: name }, h(Icon, { name }), h(Paragraph, { text: name, foreground: Colour.White, lines: 1 }))),
            ),
          ),
          h(Gap, {}),
          h(Paragraph, { text: "THE WEEK AHEAD", foreground: Colour.Yellow }),
          h(
            Columns,
            { height: 4, widths: forecast.map(() => ICON_COLUMN) },
            ...forecast.map(([day, name, degrees]) => h(Rows, { key: day }, h(Paragraph, { text: day, foreground: Colour.Cyan, lines: 1 }), h(Icon, { name }), h(Paragraph, { text: `${degrees}C`, foreground: Colour.White, lines: 1 }))),
          ),
          h(Paragraph, { text: "A blue sun: any icon takes any colour", foreground: Colour.Blue }),
          h(Icon, { name: "sun", foreground: Colour.Blue }),
        ),
    },
  ];
}
