// Magazine 3's demonstrations of the library's tables, logs, trees, menus,
// sections, pagination and tabs (304 to 312), each in its variants.

import { Colour } from "../kit.ts";
import {
  Breadcrumbs,
  type Child,
  Collapsible,
  Columns,
  Component,
  DataTable,
  Gap,
  h,
  type LogEntry,
  LogView,
  Menu,
  type PageSpec,
  Pagination,
  Paragraph,
  Rows,
  Rule,
  TabStrip,
  TabView,
  Tree,
  type TreeNode,
} from "../../../../kits/typescript-components/src/index.ts";
import type { Article } from "../../../../kits/typescript-components/src/index.ts";
import { SOUNDS } from "../assets.ts";
import { frame, INDEX, LOOK, prompts } from "../look.ts";

export const TABLES = 0x304;
export const LOG = 0x305;
export const TREES = 0x306;
export const MENUS = 0x307;
export const SECTIONS = 0x308;
export const PAGES_DEMO = 0x309;
export const TABS = 0x310;
export const FASTEXT_TABS = 0x311;
export const NUMBERED_TABS = 0x312;

const LISTS = 0x300;
const words = (article: Article): number => article.blocks.reduce((sum, block) => sum + block.text.split(" ").length, 0);

/** Picks a table row and says which, by the activate event's callback. */
class TableDemo extends Component<{ readonly articles: readonly Article[] }, { readonly chosen: string }> {
  override state = { chosen: "nothing yet" };

  override render(): Child {
    const rows = this.props.articles.map((article) => [article.page.toString(16), article.title, article.writer, String(words(article))]);
    return [
      h(DataTable, {
        columns: [
          { title: "Pg", width: 4, foreground: Colour.Yellow },
          { title: "Title", width: 18 },
          { title: "By", width: 9, foreground: Colour.Cyan },
          { title: "Wds", width: 5, align: "right", foreground: Colour.Green },
        ],
        rows,
        shown: 8,
        selectable: true,
        sortable: true,
        sounds: { move: SOUNDS.tick, choose: SOUNDS.chime },
        onChoose: (row: readonly string[]) => this.setState({ chosen: row[1] ?? "" }),
      }),
      h(Paragraph, { text: `Chosen: ${this.state.chosen}`, foreground: Colour.Yellow, lines: 1 }),
      h(Rule, { title: "striped, not selectable", foreground: Colour.Blue }),
      h(DataTable, {
        columns: [
          { title: "Writer", width: 16 },
          { title: "Articles", width: 10, align: "right" },
        ],
        rows: [...new Set(this.props.articles.map((article) => article.writer))].map((writer) => [writer, String(this.props.articles.filter((article) => article.writer === writer).length)]),
        shown: 4,
        striped: true,
      }),
      h(DataTable, { columns: [{ title: "Empty", width: 20 }], rows: [], shown: 1, empty: "no rows: the empty text" }),
    ];
  }
}

/** A log fed by its own timer: a made-up service's lines, one every two seconds, while someone watches. */
class LogDemo extends Component<object, { readonly entries: readonly LogEntry[] }> {
  override state = { entries: [] as readonly LogEntry[] };
  static readonly LINES: readonly (readonly [LogEntry["level"], string])[] = [
    ["info", "page 100 published"],
    ["debug", "packet 26 written"],
    ["info", "viewer joined"],
    ["warning", "slow fetch: 2.4 s"],
    ["info", "page 304 published"],
    ["error", "feed refused: 503"],
    ["info", "retrying the feed"],
    ["info", "feed back"],
  ];

  override componentDidMount(): void {
    let next = 0;
    const add = (): void => {
      const [level, text] = LogDemo.LINES[next % LogDemo.LINES.length] ?? ["info", ""];
      next += 1;
      this.setState((state) => ({ entries: [...state.entries, { at: this.services.epochMs(), level, text }].slice(-60) }));
    };
    for (let line = 0; line < 12; line += 1) {
      add();
    }
    this.setInterval(add, 2_000, { whileViewed: true });
  }

  override render(): Child {
    return h(LogView, { entries: this.state.entries, shown: 16 });
  }
}

const TREE: readonly TreeNode[] = [
  {
    label: "Kitchensink",
    children: [
      { label: "Text", children: [{ label: "Styles", page: 0x200 }, { label: "Headlines", page: 0x201 }, { label: "Rich text", page: 0x204 }] },
      { label: "Lists", children: [{ label: "Tables", page: TABLES }, { label: "Log", page: LOG }, { label: "Menus", page: MENUS }] },
      { label: "Figures", children: [{ label: "Charts", page: 0x400 }, { label: "Clock", page: 0x402 }] },
    ],
  },
  { label: "Magazine", children: [{ label: "Front", page: 0x800 }, { label: "Every article", page: 0x801 }] },
  { label: "How it works", page: 0x102 },
];

const MENU = [
  { label: "New page", shortcut: "N" },
  { label: "Open", shortcut: "O" },
  { label: "Recent", items: [{ label: "Page 100" }, { label: "Page 304" }, { label: "Page 801" }] },
  { rule: true, label: "" },
  { label: "Export", items: [{ label: "As TTI" }, { label: "As a picture", disabled: true }, { label: "Print", items: [{ label: "A4" }, { label: "Letter" }] }] },
  { label: "Share", disabled: true },
  { rule: true, label: "" },
  { label: "Quit", shortcut: "Q" },
] as const;

class MenuDemo extends Component<object, { readonly chosen: string }> {
  override state = { chosen: "nothing yet" };

  override render(): Child {
    return [
      h(Breadcrumbs, { trail: [{ label: "Index", page: INDEX }, { label: "Lists", page: LISTS }, { label: "Menus" }] }),
      h(Gap, {}),
      h(Menu, { title: "File", items: MENU, numbered: true, sounds: { move: SOUNDS.tick, choose: SOUNDS.chime }, onChoose: (path: readonly string[]) => this.setState({ chosen: path.join(" → ") }) }),
      h(Gap, {}),
      h(Paragraph, { text: `Chosen: ${this.state.chosen}`, foreground: Colour.Yellow }),
      h(Gap, {}),
      h(Rule, { title: "a short menu that scrolls", foreground: Colour.Blue }),
      h(Menu, { items: Array.from({ length: 12 }, (_, index) => ({ label: `Choice ${index + 1}`, shortcut: `${index + 1}` })), shown: 4, onChoose: (path: readonly string[]) => this.setState({ chosen: path.join(" → ") }) }),
    ];
  }
}

/** Pagination stepping through the articles, five a page, in all three looks. */
class PagesDemo extends Component<{ readonly articles: readonly Article[] }, { readonly page: number }> {
  override state = { page: 0 };
  static readonly PER_PAGE = 5;

  override render(): Child {
    const count = Math.ceil(this.props.articles.length / PagesDemo.PER_PAGE);
    const shown = this.props.articles.slice(this.state.page * PagesDemo.PER_PAGE, (this.state.page + 1) * PagesDemo.PER_PAGE);
    return [
      ...shown.map((article) => h(Paragraph, { key: article.page, text: `${article.page.toString(16)} ${article.title}`, lines: 1 })),
      h(Gap, { rows: PagesDemo.PER_PAGE - shown.length }),
      h(Gap, {}),
      h(Paragraph, { text: "Numbers (Left Right, or click one):", foreground: Colour.Cyan }),
      h(Pagination, { count, value: this.state.page, onChange: (page: number) => this.setState({ page }) }),
      h(Gap, {}),
      h(Paragraph, { text: "A count, and dots: Tab to them", foreground: Colour.Cyan }),
      h(Pagination, { count, style: "count", value: this.state.page, onChange: (page: number) => this.setState({ page }) }),
      h(Pagination, { count, style: "dots", value: this.state.page, onChange: (page: number) => this.setState({ page }) }),
      h(Gap, {}),
      h(Pagination, { count: 20, initial: 9, sounds: { change: SOUNDS.tick } }),
      h(Paragraph, { text: "Twenty pages: the numbers round the current one", foreground: Colour.Blue }),
    ];
  }
}

/** The four looks of a tab strip, side by side in a static showing. */
function stripLooks(): Child {
  const tabs = ["News", "Sport", "TV"];
  return [
    h(Paragraph, { text: "chips", foreground: Colour.Cyan }),
    h(TabStrip, { tabs, active: 1, style: "chips", onChange: () => {} }),
    h(Paragraph, { text: "underline", foreground: Colour.Cyan }),
    h(TabStrip, { tabs, active: 1, style: "underline", onChange: () => {} }),
    h(Paragraph, { text: "numbered", foreground: Colour.Cyan }),
    h(TabStrip, { tabs, active: 1, style: "numbered", onChange: () => {} }),
    h(Paragraph, { text: "fastext (pages 311 and 312 take their keys)", foreground: Colour.Cyan }),
    h(TabStrip, { tabs: [...tabs, "Weather"], active: 1, style: "fastext", onChange: () => {} }),
  ];
}

export function listPages(articles: readonly Article[]): PageSpec[] {
  const next = (...links: (readonly [string, number])[]): ReturnType<typeof prompts> => prompts(["Index", INDEX], ...links);
  return [
    { page: TABLES, title: "Data tables", root: () => frame(LOOK.lists, "Data tables", next(["Log", LOG], ["Trees", TREES], ["Menus", MENUS]), undefined, h(TableDemo, { articles })) },
    { page: LOG, title: "A live log", root: () => frame(LOOK.lists, "A live log", next(["Tables", TABLES], ["Trees", TREES], ["Menus", MENUS]), undefined, h(LogDemo, {})) },
    {
      page: TREES,
      title: "Trees",
      root: () =>
        frame(
          LOOK.lists,
          "Trees",
          next(["Menus", MENUS], ["Sections", SECTIONS], ["Tables", TABLES]),
          undefined,
          h(Breadcrumbs, { trail: [{ label: "Index", page: INDEX }, { label: "Lists", page: LISTS }, { label: "Trees" }], separator: "slash" }),
          h(Columns, { height: 17 }, h(Tree, { nodes: TREE, shown: 17, open: ["Kitchensink", "Lists"] }), h(Tree, { nodes: TREE, shown: 17, guides: true, open: ["Kitchensink", "Text", "Magazine"] })),
          h(Paragraph, { text: "Left: indented. Right: mosaic guides. Tab between", foreground: Colour.Cyan, lines: 1 }),
        ),
    },
    { page: MENUS, title: "Menus", root: () => frame(LOOK.lists, "Menus", next(["Trees", TREES], ["Sections", SECTIONS], ["Pages", PAGES_DEMO]), undefined, h(MenuDemo, {})) },
    {
      page: SECTIONS,
      title: "Collapsible sections",
      root: () =>
        frame(
          LOOK.lists,
          "Collapsible sections",
          next(["Menus", MENUS], ["Pages", PAGES_DEMO], ["Tabs", TABS]),
          undefined,
          h(
            Collapsible,
            { titles: ["What it is", "How it opens", "An accordion"], accordion: true, initial: [0] },
            h(Paragraph, { text: "A heading on a band; its content under it while it is open.", foreground: Colour.White }),
            h(Paragraph, { text: "Up Down choose a heading; Enter, Space or a click opens or shuts it.", foreground: Colour.White }),
            h(Paragraph, { text: "As an accordion, opening one shuts the others.", foreground: Colour.White }),
          ),
          h(Gap, {}),
          h(
            Collapsible,
            { titles: ["Any can be open", "Together"], background: Colour.Magenta, initial: [0, 1] },
            h(Paragraph, { text: "Not an accordion: these open on their own.", foreground: Colour.Cyan }),
            h(Paragraph, { text: "Tab to these headings.", foreground: Colour.Cyan }),
          ),
        ),
    },
    { page: PAGES_DEMO, title: "Pagination", root: () => frame(LOOK.lists, "Pagination", next(["Sections", SECTIONS], ["Tabs", TABS], ["Tables", TABLES]), undefined, h(PagesDemo, { articles })) },
    {
      page: TABS,
      title: "Tabs",
      root: () =>
        frame(
          LOOK.lists,
          "Tabs",
          next(["Fastext tabs", FASTEXT_TABS], ["Numbered", NUMBERED_TABS], ["Pages", PAGES_DEMO]),
          undefined,
          h(
            TabView,
            { tabs: ["Looks", "About", "Empty"] },
            h(Rows, {}, stripLooks()),
            h(Paragraph, { text: "A tab view shows the chosen tab's content under its strip. Left Right or a click choose.", foreground: Colour.White }),
            h(Paragraph, { text: "", foreground: Colour.White }),
          ),
        ),
    },
    {
      page: FASTEXT_TABS,
      title: "Fastext tabs",
      root: () =>
        frame(
          LOOK.lists,
          "Fastext tabs",
          next(["Tabs", TABS], ["Numbered", NUMBERED_TABS], ["Tables", TABLES]),
          undefined,
          h(
            TabView,
            { tabs: ["News", "Sport", "TV", "Weather"], style: "fastext" },
            h(Paragraph, { text: "The coloured keys choose a tab here: the page takes them, so they do not follow the Fastext links.", foreground: Colour.White }),
            h(Paragraph, { text: "Sport: green.", foreground: Colour.Green }),
            h(Paragraph, { text: "TV: yellow.", foreground: Colour.Yellow }),
            h(Paragraph, { text: "Weather: cyan.", foreground: Colour.Cyan }),
          ),
        ),
    },
    {
      page: NUMBERED_TABS,
      title: "Numbered tabs",
      root: () =>
        frame(
          LOOK.lists,
          "Numbered tabs",
          next(["Tabs", TABS], ["Fastext tabs", FASTEXT_TABS], ["Tables", TABLES]),
          undefined,
          h(
            TabView,
            { tabs: ["One", "Two", "Three"], style: "numbered", sounds: { change: SOUNDS.tick } },
            h(Paragraph, { text: "The digits 1 to 3 choose a tab: the page takes the digits.", foreground: Colour.White }),
            h(Paragraph, { text: "Two.", foreground: Colour.Green }),
            h(Paragraph, { text: "Three, with a tick each change.", foreground: Colour.Yellow }),
          ),
        ),
    },
  ];
}
