// The kitchensink's pages: every component, and every technique of the
// showcase and the Hackaday channel, each on a page of its own, in eight
// magazines as a teletext service keeps a subject to a hundred.
//
//   1xx the kitchensink: its index, its components, how it works, a greeting
//   2xx text      3xx lists      4xx figures     5xx graphics
//   6xx input     7xx play       8xx a magazine of sample articles

import { type Article, articleLines, At, DATE_FORMATS, DateText, formatDate, type RowLook, Band, BarChart, BigDigits, Bouncer, Button, Byline, type Child, ChoiceList, chunks, Clock, Columns, Component, Concealed, Countdown, DayPages, Document, DoubleHeight, DoubleWidth, Flashing, Frame, Gap, Grid, h, Headline, Layers, type Line, Mosaic, Newsflash, type NumberedRow, NumberedRows, PageLink, type PageSpec, PaintCanvas, Paragraph, partName, Picture, Placeholder, Poll, QrPanel, Quiz, RawSubPage, Roulette, Rows, ScrollingText, Snake, SoundList, StoryPages, Table, Tally, Text, TEXT_WIDTH, TextField, Ticker, ToggleList, Wheel } from "../../../kits/typescript-components/src/index.ts";
import { type Picture as PictureFile, SOUNDS } from "./assets.ts";
import { ARTICLES } from "./content.ts";
import { BOXES, DIALOGS, FIELDS, FORM, inputPages, NUMBERS, SUGGEST, SWITCHES, WIZARD } from "./demos/input.ts";
import { CALENDAR, COLUMNS_CHART, DATES, figurePages, HEAT, LINES_CHART, PROGRESS, SPINNERS } from "./demos/figures.ts";
import { graphicsPages, ICONS_PAGE, POINTER_PAGE } from "./demos/graphics.ts";
import { BARS, CENTRED, layoutPages, PANELS } from "./demos/layout.ts";
import { FASTEXT_TABS, listPages, LOG, MENUS, NUMBERED_TABS, PAGES_DEMO, SECTIONS as SECTIONS_PAGE, TABLES, TABS, TREES } from "./demos/lists.ts";
import { OPEN_WITH, playPages, STREAMS } from "./demos/play.ts";
import { BADGES, RICH, textPages } from "./demos/text.ts";
import { frame, INDEX, LOOK, prompts } from "./look.ts";
import { EMBLEM } from "./emblem.ts";
import { Colour, type ServerStats, figure } from "./kit.ts";

/** What the pages take from outside the program (assets.ts). */
export interface Assets {
  readonly pictures: readonly PictureFile[];
  /** Each address's QR code, as TTI. */
  readonly qr: ReadonlyMap<string, string>;
}

/** The index, and each magazine's front. */
const COMPONENTS = 0x101;
const HOW = 0x102;
const GREETING = 0x103;
const TEXT = 0x200;
const HEADLINES = 0x201;
const DOCUMENT = 0x202;
const DAYS = 0x203;
const SCROLLING = 0x300;
const CHOICES = 0x301;
const ROWS_PAGE = 0x302;
const FAVOURITES = 0x303;
const CHARTS = 0x400;
const COUNTDOWN = 0x401;
const CLOCK = 0x402;
const BEHIND = 0x403;
const MOSAICS = 0x500;
const PICTURE = 0x501;
const GALLERY = 0x502;
const QR = 0x503;
const PAINT = 0x504;
const ANIMATION = 0x505;
const SEARCH = 0x600;
const QUIZ = 0x601;
const POLL = 0x602;
const BUTTONS = 0x603;
const RETITLE = 0x604;
const ROULETTE = 0x700;
const SOUNDBOARD = 0x701;
const SNAKE = 0x702;
const MAGAZINE = 0x800;
const EVERY_ARTICLE = 0x801;

// Magazine 1: the kitchensink.

/** The sections of the index, with what each shows. */
const SECTIONS: readonly (readonly [number, string])[] = [
  [COMPONENTS, "Components A-Z"],
  [HOW, "How it works"],
  [TEXT, "Text"],
  [SCROLLING, "Lists"],
  [CHARTS, "Figures"],
  [MOSAICS, "Graphics"],
  [SEARCH, "Input"],
  [ROULETTE, "Play"],
  [MAGAZINE, "A magazine"],
  [GREETING, "A greeting"],
];

class Front extends Component<{ readonly articles: readonly Article[] }> {
  override render(): Child {
    const newest = this.props.articles[0];
    return h(
      Frame,
      { title: "Kitchensink", background: LOOK.home.background, prompts: prompts(["Components", COMPONENTS], ["How", HOW], ["Text", TEXT], ["Play", ROULETTE]), index: INDEX, bare: true, hint: "Type a page number or press a colour" },
      h(
        Layers,
        { height: 7 },
        h(At, { row: 0, column: 1 }, h(Mosaic, { bitmap: EMBLEM, foreground: Colour.Cyan })),
        h(At, { row: 0, column: 15 }, h(DoubleWidth, { text: "KITCHENSINK", foreground: Colour.Yellow })),
        h(At, { row: 2, column: 15 }, h(Paragraph, { text: "every component, every trick", foreground: Colour.White, lines: 1 })),
        h(At, { row: 3, column: 15 }, h(Paragraph, { text: "class components, teletext", foreground: Colour.Cyan, lines: 1 })),
      ),
      h(Band, { text: "THE SECTIONS", background: Colour.Blue }),
      h(Grid, { columns: 2, height: 5 }, ...SECTIONS.map(([page, label]) => h(PageLink, { key: page, page, label }))),
      h(Gap, {}),
      newest === undefined ? null : h(Band, { text: "NEWEST ARTICLE", background: Colour.Red, foreground: Colour.Yellow }),
      newest === undefined ? null : h(Headline, { title: newest.title, page: newest.page }),
      h(Gap, { rows: 0, grow: 1 }),
      newest === undefined ? null : h(Newsflash, { text: `${newest.page.toString(16)} ${newest.title}` }),
      h(Ticker, { items: this.props.articles.map((article) => `${article.page.toString(16)} ${article.title}`), everyMs: 7_000 }),
    );
  }
}

/** One component in the catalogue: its name, the page that shows it, and what it is. */
const CATALOGUE: readonly (readonly [string, number, string])[] = [
  ["AudioPlayer", STREAMS, "progress, pause, stop"],
  ["Autocomplete", SUGGEST, "suggests as you type"],
  ["Badges", BADGES, "labels on chips"],
  ["Band", TEXT, "a coloured strip"],
  ["Bar", CHARTS, "to the half cell"],
  ["BarChart", CHARTS, "labelled bars"],
  ["BigDigits", COUNTDOWN, "large mosaic digits"],
  ["Bouncer", ANIMATION, "a shared animation"],
  ["Breadcrumbs", MENUS, "the way down"],
  ["Button", BUTTONS, "click or Enter"],
  ["Byline", HEADLINES, "writer, date, labels"],
  ["Calendar", CALENDAR, "a month, choose a day"],
  ["Centred", CENTRED, "a stack in the middle"],
  ["Checkbox", BOXES, "ticked, mixed, disabled"],
  ["CheckboxGroup", BOXES, "several, two columns"],
  ["ChoiceList", CHOICES, "text rows, component rows"],
  ["ListRow ListRows", CHOICES, "a row lit by the pointer"],
  ["Clock", CLOCK, "the time, live"],
  ["Collapsible", SECTIONS_PAGE, "sections, accordion"],
  ["ColumnChart", COLUMNS_CHART, "columns in colours"],
  ["Concealed", TEXT, "shown by Reveal"],
  ["Countdown", COUNTDOWN, "ticks each second"],
  ["DataTable", TABLES, "sort, choose, stripes"],
  ["DateText formatDate", DATES, "every date, one way"],
  ["DayPages", DAYS, "a sub-page a day"],
  ["Dialog Modal", DIALOGS, "alert, confirm, prompt"],
  ["Document", DOCUMENT, "lines over sub-pages"],
  ["DoubleHeight", TEXT, "two rows high"],
  ["DoubleWidth", TEXT, "two cells wide (2.5)"],
  ["FastextBar", TEXT, "row 24's four prompts"],
  ["Field", FORM, "label and control"],
  ["Flashing", TEXT, "flash, then steady"],
  ["Frame", TEXT, "band, body, hint, Fastext"],
  ["Grid", ROWS_PAGE, "a grid of cells"],
  ["Headline", HEADLINES, "a title, its page"],
  ["Heatmap", HEAT, "by colour or density"],
  ["HintLine", SCROLLING, "row 23: the keys"],
  ["Icon", ICONS_PAGE, "weather and marks"],
  ["KeyHelp", BARS, "keys, one or two cols"],
  ["LineChart", LINES_CHART, "line and area"],
  ["LineGauge", PROGRESS, "a thin line, held"],
  ["LogView", LOG, "follows, pauses"],
  ["Menu", MENUS, "nested, numbered"],
  ["Mosaic", MOSAICS, "a bitmap of sixels"],
  ["Newsflash", HEADLINES, "NEWSFLASH, flashing"],
  ["Notice", BADGES, "info to error"],
  ["NumberedRows", ROWS_PAGE, "pages and figures"],
  ["NowPlaying", STREAMS, "a stream's progress"],
  ["NumberInput", NUMBERS, "- 3 +, digits"],
  ["PageLink", HEADLINES, "a number to click"],
  ["Pagination", PAGES_DEMO, "numbers, count, dots"],
  ["PaintCanvas", PAINT, "drag to draw, a brush"],
  ["Panel", PANELS, "line or band frame"],
  ["Paragraph", TEXT, "wrapped text"],
  ["Picture", PICTURE, "TTI rows in a box"],
  ["Placeholder", HEADLINES, "while it comes"],
  ["Pointer areas", POINTER_PAGE, "every move, no button"],
  ["Poll", POLL, "live for everyone"],
  ["PositionLine", SCROLLING, "Lines 1-18 of 66"],
  ["ProgressBar", PROGRESS, "solid, segments, gauge"],
  ["QrPanel", QR, "a QR code to follow"],
  ["Quiz", QUIZ, "the coloured keys"],
  ["RadioGroup", BOXES, "down or across"],
  ["RawSubPage", GALLERY, "a whole TTI page"],
  ["RichText", RICH, "runs of styles"],
  ["Roulette", ROULETTE, "spins, sounds, goes"],
  ["Rule", PANELS, "five kinds of line"],
  ["ScatterPlot", LINES_CHART, "points on axes"],
  ["ScrollBar", SCROLLING, "grip and drag"],
  ["ScrollingText", SCROLLING, "keys, wheel, bar"],
  ["Select", NUMBERS, "turn or open a list"],
  ["Slider", SWITCHES, "a knob, half cells"],
  ["Snake", SNAKE, "a game a frame a tick"],
  ["SoundList", SOUNDBOARD, "plays a sound"],
  ["Sparkline", COLUMNS_CHART, "a series in a row"],
  ["Spinner", SPINNERS, "four kinds, then OK"],
  ["StatusBar", BARS, "segments, alarm"],
  ["StoryPages", 0x810, "an article's home"],
  ["Switch", SWITCHES, "ON and OFF"],
  ["Table", ROWS_PAGE, "labels and values"],
  ["TabStrip TabView", TABS, "four looks of tabs"],
  ["Tabs, coloured keys", FASTEXT_TABS, "Fastext tabs"],
  ["Tabs, digits", NUMBERED_TABS, "numbered tabs"],
  ["TextField", FIELDS, "label, mask, check"],
  ["Ticker", HEADLINES, "changes on a timer"],
  ["TitleBand", TEXT, "rows 1-2, the name"],
  ["ToastShelf", BADGES, "toasts that go"],
  ["ToggleList", FAVOURITES, "stars on and off"],
  ["Tree", TREES, "indented or guides"],
  ["Wheel", MOSAICS, "a roulette wheel"],
  ["Wizard", WIZARD, "steps, red and green"],
  ["Rows Columns At", ROWS_PAGE, "the layouts"],
  ["Layers Gap", 0x100, "overlaid, spaced"],
  ["Streams", STREAMS, "sound of any length"],
  ["Pages opened with", OPEN_WITH, "goto with params"],
];

/** Rows of a catalogue sub-page. */
const CATALOGUE_ROWS = 18;

class Catalogue extends Component {
  override render(): Child {
    const parts = chunks(CATALOGUE, CATALOGUE_ROWS, CATALOGUE_ROWS);
    return parts.map((part, index) =>
      frame(
        LOOK.home,
        partName("Components", index + 1, parts.length),
        prompts(["Index", INDEX], ["How", HOW], ["Text", TEXT], ["Lists", SCROLLING]),
        "Click a component to see it",
        h(NumberedRows, { key: `part-${index}`, entries: part.map(([name, page, what]): NumberedRow => ({ page, label: `${name}: ${what}` })) }),
      ),
    );
  }
}

/** Page 102's text. */
const HOW_TEXT: readonly (readonly [Colour, string])[] = [
  [Colour.Yellow, "Class components, as React's"],
  [Colour.White, "A component is a class with props from its parent and state of its own. setState merges a change and asks for a render; renders are batched until the event that caused them is handled."],
  [Colour.White, "render() answers elements, made with h(Type, props, ...children); a component that draws itself answers nothing and paints on a canvas of its own box instead."],
  [Colour.Yellow, "The lifecycle"],
  [Colour.White, "componentDidMount after its first render is on the page, shouldComponentUpdate before a render, componentDidUpdate after it, componentWillUnmount when it leaves. Timers made with setInterval are cleared then."],
  [Colour.Yellow, "Mounts"],
  [Colour.White, "Every page is mounted once at the start as the copy every viewer shares. A page that keeps state for each viewer is mounted again when a viewer opens it, and unmounted when they leave."],
  [Colour.White, "A viewer's copy is published only while it differs from the shared one, and released when it is the same again, so a viewer who only reads costs the server nothing."],
  [Colour.Yellow, "Input"],
  [Colour.White, "Keys and typing go to the focused component; a click or the wheel to the component drawn in that cell. Each bubbles to the parents until one answers true; what none takes goes to page selection."],
  [Colour.White, "What a click acts on is a control, which the browser lights under the pointer itself, with no message. A move with no button held is sent only from a pointer area a page declares, as the pointer page does."],
  [Colour.Yellow, "No global state"],
  [Colour.White, "Data comes down as props; components that share nothing else talk through the event bus, as the poll does."],
  [Colour.Yellow, "Teletext, not the web"],
  [Colour.White, "A screen is 40 by 25 cells. A colour takes a cell, a background runs to the row's end, double height covers the row below: the canvas and the layouts allow for each."],
];

function howLines(): Line[] {
  return HOW_TEXT.flatMap(([foreground, text], index) => [
    ...(index > 0 && foreground === Colour.Yellow ? [{ foreground: Colour.White, text: "" }] : []),
    ...articleLines([{ kind: "paragraph", text }], TEXT_WIDTH).map((line) => ({ ...line, foreground })),
  ]);
}

/** How long the greeting stays before the page is everyone's again. */
const GREETING_MS = 3_000;

class Greeting extends Component<object, { readonly greeting: boolean }> {
  override state = { greeting: false };

  /** Only a viewer's copy greets; the shared copy never does. */
  override componentDidMount(): void {
    if (this.services.session !== null) {
      this.setState({ greeting: true });
      this.setTimeout(() => this.setState({ greeting: false }), GREETING_MS);
    }
  }

  override render(): Child {
    return frame(
      LOOK.home,
      "A greeting",
      prompts(["Index", INDEX], ["How", HOW], ["Components", COMPONENTS], ["Text", TEXT]),
      undefined,
      this.state.greeting
        ? [h(DoubleHeight, { text: `Hello, viewer ${String(this.services.session)}`, foreground: Colour.Yellow }), h(Gap, {}), h(Paragraph, { text: "This copy is yours alone, for three seconds. Then the page is the same as everyone's, and the server is told to send you the shared copy again (release).", foreground: Colour.Cyan })]
        : [h(Paragraph, { text: "Everyone sees this. Open the page again to be greeted: for three seconds your copy differs, so it is published for you alone; then it matches and is released.", foreground: Colour.White })],
    );
  }
}

// Magazine 2: text.

class TextStyles extends Component {
  override render(): Child {
    const colours = [Colour.Red, Colour.Green, Colour.Yellow, Colour.Blue, Colour.Magenta, Colour.Cyan, Colour.White] as const;
    return frame(
      LOOK.text,
      "Text styles",
      prompts(["Index", INDEX], ["Headlines", HEADLINES], ["Document", DOCUMENT], ["Lists", SCROLLING]),
      "Press Reveal to see what is concealed",
      h(DoubleHeight, { text: "Double height", foreground: Colour.Yellow }),
      h(DoubleWidth, { text: "WIDE", foreground: Colour.Cyan }),
      h(Flashing, { text: "Flashing, then steady", foreground: Colour.Red }),
      h(Concealed, { text: "Concealed: you pressed Reveal", foreground: Colour.Magenta }),
      h(Band, { text: "A band: background to the box's end", background: Colour.Blue, foreground: Colour.Yellow }),
      h(Gap, {}),
      h(Columns, { height: 1 }, ...colours.map((foreground) => h(Paragraph, { key: foreground, text: "Text", foreground }))),
      h(Gap, {}),
      h(Paragraph, { text: "A paragraph wraps at spaces to its box's width, and a word longer than a line is cut across lines. Its colour takes the first cell of every line.", foreground: Colour.White }),
      h(Gap, {}),
      h(Columns, { widths: [20, null], height: 4 }, h(Paragraph, { text: "Two columns, each starting with its own colour cell.", foreground: Colour.Green }), h(Paragraph, { text: "The right one is clipped to its box.", foreground: Colour.Cyan })),
    );
  }
}

class HeadlinesDemo extends Component<{ readonly articles: readonly Article[] }> {
  override render(): Child {
    const [first, second, third] = this.props.articles;
    return frame(
      LOOK.text,
      "Headlines",
      prompts(["Index", INDEX], ["Text", TEXT], ["Document", DOCUMENT], ["Days", DAYS]),
      "Click a headline or a page number",
      first === undefined ? null : [h(Headline, { title: first.title, page: first.page, foreground: Colour.Yellow }), h(Byline, { writer: first.writer, date: first.date, labels: first.tags })],
      h(Gap, {}),
      second === undefined ? null : h(Headline, { title: second.title, page: second.page }),
      third === undefined ? null : h(Headline, { title: third.title, page: third.page }),
      h(Gap, {}),
      h(Grid, { columns: 2, height: 2 }, h(PageLink, { page: TEXT, label: "Text styles" }), h(PageLink, { page: DOCUMENT, label: "Document" }), h(PageLink, { page: DAYS, label: "A day a page" }), h(PageLink, { page: MAGAZINE, label: "Magazine" })),
      h(Gap, {}),
      h(Placeholder, { height: 3, text: "Placeholder: while something comes" }),
      h(Newsflash, { text: "a flashing word, then the news" }),
      h(Gap, {}),
      h(Ticker, { items: this.props.articles.map((article) => article.title), everyMs: 3_000 }),
    );
  }
}

function days(articles: readonly Article[]): { name: string; articles: Article[] }[] {
  const byDay = new Map<string, Article[]>();
  for (const article of articles) {
    const name = formatDate(article.date, DATE_FORMATS.weekday);
    byDay.set(name, [...(byDay.get(name) ?? []), article]);
  }
  return [...byDay.entries()].map(([name, held]) => ({ name, articles: held }));
}

// Magazine 3: lists.

class Favourites extends Component<{ readonly articles: readonly Article[] }, { readonly on: readonly number[] }> {
  override state = { on: [] as readonly number[] };

  override render(): Child {
    const tags = [...new Set(this.props.articles.flatMap((article) => article.tags))].sort().slice(0, 8);
    const wanted = new Set(this.state.on.map((index) => tags[index]));
    const chosen = this.props.articles.filter((article) => article.tags.some((tag) => wanted.has(tag))).slice(0, 4);
    return frame(
      LOOK.lists,
      "Favourites",
      prompts(["Index", INDEX], ["Lists", SCROLLING], ["Choose", CHOICES], ["Rows", ROWS_PAGE]),
      "Up Down, then Enter or a click to star",
      h(Paragraph, { text: "Your tags (yours alone: state lifted to the page)", foreground: Colour.Cyan }),
      h(ToggleList, { items: tags, onChange: (on) => this.setState({ on }) }),
      h(Gap, {}),
      h(Band, { text: wanted.size === 0 ? "STAR A TAG" : "YOUR LATEST", background: Colour.Blue }),
      ...chosen.map((article) => h(Headline, { key: article.page, title: article.title, page: article.page })),
    );
  }
}

// Magazine 4: figures.

class Stats extends Component<object, { readonly stats: ServerStats | null }> {
  override state = { stats: null as ServerStats | null };
  #unsubscribe: (() => void) | null = null;

  override componentDidMount(): void {
    this.#unsubscribe = this.services.bus.on("stats", (stats) => this.setState({ stats }));
  }

  override componentWillUnmount(): void {
    this.#unsubscribe?.();
  }

  override render(): Child {
    const { stats } = this.state;
    const body =
      stats === null
        ? [h(Placeholder, { text: "Waiting for the server's figures..." })]
        : [
            h(Paragraph, { text: "WATCHING NOW", foreground: Colour.Yellow }),
            h(BigDigits, { text: String(Math.min(9_999, figure(stats.channel, "viewers"))), foreground: Colour.Green }),
            h(Gap, {}),
            h(Table, {
              rows: [
                ["Viewers at most", String(figure(stats.channel, "viewers_peak"))],
                ["Pages opened", String(figure(stats.channel, "opens"))],
                ["Keys", String(figure(stats.channel, "keys"))],
                ["Shared pages drawn", String(figure(stats.channel, "shared_published"))],
                ["Viewers' own drawn", String(figure(stats.channel, "own_published"))],
                ["Server up, seconds", String(figure(stats.server, "uptime_s"))],
                ["Bytes sent", String(figure(stats.server, "bytes_sent"))],
              ],
              labelCells: 22,
            }),
          ];
    return frame(LOOK.figures, "Behind the scenes", prompts(["Index", INDEX], ["Charts", CHARTS], ["Clock", CLOCK], ["Countdown", COUNTDOWN]), "The server's figures every 10 s, by the bus", ...body);
  }
}

// Magazine 6: input.

class Search extends Component<{ readonly articles: readonly Article[] }, { readonly results: readonly Article[]; readonly asked: string }> {
  override state = { results: [] as readonly Article[], asked: "" };

  override render(): Child {
    return frame(
      LOOK.input,
      "Search",
      prompts(["Index", INDEX], ["Quiz", QUIZ], ["Poll", POLL], ["Buttons", BUTTONS]),
      this.state.results.length === 0 ? "Type words, then Enter" : "Up Down or click to choose, Enter reads",
      h(Paragraph, { text: "Search the sample articles: titles and summaries", foreground: Colour.Cyan }),
      h(Gap, {}),
      h(TextField, {
        longest: 30,
        onSubmit: (words: string) => {
          const wanted = words.toLowerCase();
          this.setState({ asked: words, results: this.props.articles.filter((article) => `${article.title} ${article.summary}`.toLowerCase().includes(wanted)) });
        },
      }),
      h(Gap, {}),
      this.state.asked === "" ? null : h(Paragraph, { text: `${this.state.results.length} found for "${this.state.asked}"`, foreground: Colour.Green }),
      h(ChoiceList, { key: this.state.asked, items: this.state.results.map((article) => article.title), shown: 12, itemRows: 2, onChoose: (index: number, session: number) => this.#read(index, session) }),
    );
  }

  #read(index: number, session: number): void {
    const article = this.state.results[index];
    if (article !== undefined) {
      this.services.goto(session, article.page);
    }
  }
}

const QUESTIONS = [
  { question: "Which page carried subtitles on UK teletext?", answers: ["100", "302", "500", "888"], right: 3 },
  { question: "How many sixels make a mosaic cell?", answers: ["4", "6", "8", "9"], right: 1 },
  { question: "How many characters fit across a row?", answers: ["32", "40", "64", "80"], right: 1 },
  { question: "How many colours has Level 1, black included?", answers: ["4", "8", "16", "32"], right: 1 },
] as const;

/** One copy for everyone: a count anyone can add to, and buttons that go somewhere and that play to everyone on the page. */
class SharedButtons extends Component<{ readonly articles: readonly Article[] }, { readonly presses: number; readonly last: string }> {
  override state = { presses: 0, last: "nobody yet" };

  override render(): Child {
    return frame(
      LOOK.input,
      "Buttons",
      prompts(["Index", INDEX], ["Search", SEARCH], ["Retitle", RETITLE], ["Play", ROULETTE]),
      "Click a button, or Tab then Enter",
      h(Paragraph, { text: "This page is the same copy for every viewer, so its count is everyone's.", foreground: Colour.Cyan }),
      h(Gap, {}),
      h(BigDigits, { text: String(this.state.presses % 10_000), foreground: Colour.Yellow }),
      h(Paragraph, { text: `presses, the last by viewer ${this.state.last}`, foreground: Colour.White }),
      h(Gap, {}),
      h(Button, { shared: true, label: "Press me", onPress: (session: number) => this.setState({ presses: this.state.presses + 1, last: String(session) }) }),
      h(Gap, {}),
      h(Button, { shared: true, label: "Read a random article", background: Colour.Green, onPress: (session: number) => this.#random(session) }),
      h(Gap, {}),
      h(Button, { shared: true, label: "Chime for everyone here", background: Colour.Magenta, onPress: () => this.services.play(SOUNDS.chime) }),
    );
  }

  #random(session: number): void {
    const article = this.props.articles[this.services.below(this.props.articles.length)];
    if (article !== undefined) {
      this.services.goto(session, article.page);
    }
  }
}

class Retitled extends Component<object, { readonly count: number }> {
  override state = { count: 0 };

  override render(): Child {
    return frame(
      LOOK.input,
      "Retitle",
      prompts(["Index", INDEX], ["Buttons", BUTTONS], ["Search", SEARCH], ["Quiz", QUIZ]),
      "The contents list names the page anew",
      h(Paragraph, { text: "Each press gives this page a new title in the channel's contents (the protocol's title message), which the browser's page list shows.", foreground: Colour.White }),
      h(Gap, {}),
      h(Button, {
        shared: true,
        label: "Retitle this page",
        onPress: () => {
          const count = this.state.count + 1;
          this.setState({ count });
          this.services.title(`Retitled ${count} times`);
        },
      }),
      h(Gap, {}),
      h(Paragraph, { text: `Retitled ${this.state.count} times`, foreground: Colour.Yellow }),
    );
  }
}

// Magazine 8: the magazine.

class MagazineFront extends Component<{ readonly articles: readonly Article[] }> {
  override render(): Child {
    const [lead, ...rest] = this.props.articles;
    const tags = new Map<string, number>();
    for (const article of this.props.articles) {
      for (const tag of article.tags) {
        tags.set(tag, (tags.get(tag) ?? 0) + 1);
      }
    }
    const busiest = [...tags.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 6);
    return frame(
      LOOK.articles,
      "The magazine",
      prompts(["Index", INDEX], ["Every one", EVERY_ARTICLE], ["By day", DAYS], ["Search", SEARCH]),
      undefined,
      lead === undefined ? null : [h(Headline, { title: lead.title, page: lead.page, foreground: Colour.Yellow }), h(Paragraph, { text: lead.summary, foreground: Colour.Cyan, lines: 2 })],
      h(Band, { text: `LATEST: ${this.props.articles.length} ARTICLES ON 801`, background: LOOK.articles.background, foreground: Colour.White }),
      ...rest.slice(0, 5).map((article) => h(Headline, { key: article.page, title: article.title, page: article.page })),
      h(Gap, { rows: 0, grow: 1 }),
      h(Band, { text: "BY TAG", background: Colour.Blue }),
      h(Grid, { columns: 2, height: 3 }, ...busiest.map(([tag, count]) => h(Paragraph, { key: tag, text: `${tag} ${count}`, foreground: Colour.White }))),
    );
  }
}

/** Where every article appears, for its "Where it appears" sub-page. */
function whereOf(article: Article): NumberedRow[] {
  return [
    { page: MAGAZINE, label: "The magazine's front" },
    { page: EVERY_ARTICLE, label: "Every article" },
    { page: DAYS, label: `Its day: ${formatDate(article.date, DATE_FORMATS.weekday)}` },
    { page: SEARCH, label: "Search: its title" },
  ];
}

/** Every page of the kitchensink. */
export function pages(assets: Assets): PageSpec[] {
  const articles = ARTICLES;
  const tally = new Tally("best", ["Ceefax", "Oracle", "Teletext Ltd", "Antiope", "Videotext", "Bildschirmtext"]);
  const scrollLines = articles.flatMap((article) => [{ foreground: Colour.Yellow, text: article.title }, ...articleLines(article.blocks, TEXT_WIDTH - 2), { foreground: Colour.White, text: "" }]);
  const gallery = assets.pictures.slice(0, 8);
  const pictureOf = (index: number): string | undefined => {
    const picture = assets.pictures[index % Math.max(1, assets.pictures.length)];
    return picture?.levels.find((level) => level.level === "l25")?.tti ?? picture?.levels[0]?.tti;
  };
  const firstPicture = assets.pictures[0]?.levels.find((level) => level.level === "l15")?.tti;

  const specs: PageSpec[] = [
    ...layoutPages(),
    ...textPages(),
    ...listPages(articles),
    ...figurePages(articles),
    ...graphicsPages(),
    ...inputPages(),
    ...playPages(),
    { page: INDEX, title: "Kitchensink", section: "Kitchensink", root: () => h(Front, { articles }) },
    { page: COMPONENTS, title: "Components A to Z", root: () => h(Catalogue, {}) },
    { page: HOW, title: "How it works", root: () => h(Document, { title: "How it works", ...LOOK.home, prompts: prompts(["Index", INDEX], ["Components", COMPONENTS], ["Greeting", GREETING], ["Text", TEXT]), index: INDEX, lines: howLines() }) },
    { page: GREETING, title: "A greeting", perViewer: true, root: () => h(Greeting, {}) },

    { page: TEXT, title: "Text styles", section: "Text", root: () => h(TextStyles, {}) },
    { page: HEADLINES, title: "Headlines", root: () => h(HeadlinesDemo, { articles }) },
    {
      page: DOCUMENT,
      title: "A document",
      root: () => h(Document, { title: "Document", ...LOOK.text, prompts: prompts(["Index", INDEX], ["Text", TEXT], ["Headlines", HEADLINES], ["Days", DAYS]), index: INDEX, lines: scrollLines.slice(0, 70) }),
    },
    { page: DAYS, title: "A sub-page a day", root: () => h(DayPages, { ...LOOK.text, prompts: prompts(["Index", INDEX], ["Text", TEXT], ["Magazine", MAGAZINE], ["Document", DOCUMENT]), index: INDEX, days: days(articles) }) },

    {
      page: SCROLLING,
      title: "Scrolling text",
      section: "Lists",
      perViewer: true,
      root: () => frame(LOOK.lists, "Scrolling text", prompts(["Index", INDEX], ["Choose", CHOICES], ["Rows", ROWS_PAGE], ["Favourites", FAVOURITES]), "PgDn PgUp arrows Home End, wheel, drag the bar", h(ScrollingText, { lines: scrollLines, shown: 19 })),
    },
    {
      page: CHOICES,
      title: "A list to choose from",
      perViewer: true,
      root: () =>
        frame(
          LOOK.lists,
          "Choose an article",
          prompts(["Index", INDEX], ["Scroll", SCROLLING], ["Rows", ROWS_PAGE], ["Favourites", FAVOURITES]),
          undefined,
          h(EveryArticle, { articles }),
        ),
    },
    {
      page: ROWS_PAGE,
      title: "Rows, grids and tables",
      root: () =>
        frame(
          LOOK.lists,
          "Rows, grids, tables",
          prompts(["Index", INDEX], ["Scroll", SCROLLING], ["Choose", CHOICES], ["Favourites", FAVOURITES]),
          "Click a row to go to its page",
          h(NumberedRows, { entries: articles.slice(0, 6).map((article) => ({ page: article.page, label: article.title, figure: article.blocks.reduce((sum, block) => sum + block.text.split(" ").length, 0) })) }),
          h(Gap, {}),
          h(Band, { text: "A GRID", background: Colour.Blue }),
          h(Grid, { columns: 3, height: 3 }, ...articles.slice(0, 9).map((article) => h(Paragraph, { key: article.page, text: article.writer, foreground: Colour.Cyan }))),
          h(Gap, {}),
          h(Band, { text: "A TABLE", background: Colour.Blue }),
          h(Table, { rows: [["Articles", String(articles.length)], ["Writers", String(new Set(articles.map((article) => article.writer)).size)], ["Newest", formatDate(articles[0]?.date ?? 0, DATE_FORMATS.stamp)]], labelCells: 14 }),
        ),
    },
    { page: FAVOURITES, title: "Favourites", perViewer: true, keepWhileAway: true, root: () => h(Favourites, { articles }) },

    {
      page: CHARTS,
      title: "Bar charts",
      section: "Figures",
      root: () => {
        const writers = new Map<string, number>();
        for (const article of articles) {
          writers.set(article.writer, (writers.get(article.writer) ?? 0) + 1);
        }
        return frame(
          LOOK.figures,
          "Bar charts",
          prompts(["Index", INDEX], ["Countdown", COUNTDOWN], ["Clock", CLOCK], ["Behind", BEHIND]),
          undefined,
          h(Paragraph, { text: "Articles by writer", foreground: Colour.Yellow }),
          h(BarChart, { bars: [...writers.entries()].map(([label, value]) => ({ label, value })), foreground: Colour.Green }),
          h(Gap, {}),
          h(Paragraph, { text: "Words in each article", foreground: Colour.Yellow }),
          h(BarChart, { bars: articles.slice(0, 8).map((article) => ({ label: article.title, value: article.blocks.reduce((sum, block) => sum + block.text.split(" ").length, 0) })), foreground: Colour.Magenta }),
        );
      },
    },
    {
      page: COUNTDOWN,
      title: "Countdown",
      root: () =>
        frame(
          LOOK.figures,
          "Countdown",
          prompts(["Index", INDEX], ["Charts", CHARTS], ["Clock", CLOCK], ["Behind", BEHIND]),
          "Ticks each second, a timer of its own",
          h(Paragraph, { text: "To the new year, UTC:", foreground: Colour.Yellow }),
          h(Gap, {}),
          h(Countdown, { until: Date.UTC(2027, 0, 1), foreground: Colour.Green }),
          h(Paragraph, { text: "days     hours  minutes  seconds", foreground: Colour.Cyan }),
        ),
    },
    {
      page: CLOCK,
      title: "Clock",
      root: () => frame(LOOK.figures, "Clock", prompts(["Index", INDEX], ["Charts", CHARTS], ["Countdown", COUNTDOWN], ["Behind", BEHIND]), "The page every viewer shares, redrawn each second", h(Gap, {}), h(Clock, { seconds: true }), h(Gap, {}), h(Paragraph, { text: "UTC, from the server's clock", foreground: Colour.Cyan })),
    },
    { page: BEHIND, title: "Behind the scenes", root: () => h(Stats, {}) },

    {
      page: MOSAICS,
      title: "Mosaics",
      section: "Graphics",
      root: () =>
        frame(
          LOOK.graphics,
          "Mosaics",
          prompts(["Index", INDEX], ["Picture", PICTURE], ["Gallery", GALLERY], ["Paint", PAINT]),
          undefined,
          h(Columns, { widths: [14, 14, null], height: 6 }, h(Mosaic, { bitmap: EMBLEM, foreground: Colour.Cyan }), h(Mosaic, { bitmap: EMBLEM, foreground: Colour.Yellow }), h(Mosaic, { bitmap: EMBLEM, foreground: Colour.Green })),
          h(Gap, {}),
          h(Paragraph, { text: "Bitmaps of sixels, two across and three down a cell; the colour takes the cell before.", foreground: Colour.White }),
          h(At, { row: 0, column: 10, rows: 11 }, h(Wheel, { lit: 0 })),
        ),
    },
    {
      page: PICTURE,
      title: "A picture in a box",
      root: () =>
        frame(
          LOOK.graphics,
          "A picture in a box",
          prompts(["Index", INDEX], ["Mosaics", MOSAICS], ["Gallery", GALLERY], ["QR", QR]),
          firstPicture === undefined ? "No pictures folder given" : "Level 1.5 rows, clipped to the box",
          firstPicture === undefined ? h(Placeholder, { text: "No pictures: start with --pictures" }) : h(Picture, { tti: firstPicture, firstRow: 3 }),
        ),
    },
    {
      page: GALLERY,
      title: "Gallery",
      root: () =>
        gallery.length === 0
          ? frame(LOOK.graphics, "Gallery", prompts(["Index", INDEX], ["Mosaics", MOSAICS], ["Picture", PICTURE], ["QR", QR]), undefined, h(Placeholder, { text: "No pictures: start with --pictures" }))
          : h(
              Rows,
              {},
              ...gallery.map((picture) =>
                h(RawSubPage, { key: picture.name, tti: picture.levels.find((level) => level.level === "l25")?.tti ?? picture.levels[0]?.tti ?? "", title: picture.caption, background: LOOK.graphics.background, foreground: LOOK.graphics.foreground, fastext: [INDEX, MOSAICS, PICTURE, QR] }),
              ),
            ),
    },
    {
      page: QR,
      title: "QR codes",
      root: () => {
        const article = articles[0];
        return frame(
          LOOK.graphics,
          "QR codes",
          prompts(["Index", INDEX], ["Mosaics", MOSAICS], ["Paint", PAINT], ["Animation", ANIMATION]),
          "Made by the engine's neottx qr",
          article === undefined ? null : h(QrPanel, { what: "The first article", title: article.title, ...(assets.qr.get(article.link) === undefined ? {} : { qr: assets.qr.get(article.link) as string }) }),
        );
      },
    },
    {
      page: PAINT,
      title: "Paint",
      perViewer: true,
      keepWhileAway: true,
      ownsDigits: true,
      root: () => frame(LOOK.graphics, "Paint", prompts(["Index", INDEX], ["Mosaics", MOSAICS], ["Pointer", POINTER_PAGE], ["Animation", ANIMATION]), "Drag to draw; 1-7 colour; Backspace clears", h(PaintCanvas, { columns: 40, rows: 20, brush: true })),
    },
    {
      page: ANIMATION,
      title: "Animation",
      root: () => frame(LOOK.graphics, "Animation", prompts(["Index", INDEX], ["Mosaics", MOSAICS], ["Paint", PAINT], ["Snake", SNAKE]), "One copy for every viewer, five frames a second", h(Bouncer, { everyMs: 200 })),
    },

    { page: SEARCH, title: "Search", section: "Input", perViewer: true, ownsDigits: true, root: () => h(Search, { articles }) },
    {
      page: QUIZ,
      title: "Quiz",
      perViewer: true,
      ownsColourKeys: true,
      root: () => frame(LOOK.input, "Quiz", prompts(["red", QUIZ], ["green", QUIZ], ["yellow", QUIZ], ["cyan", QUIZ]), "Answer with the coloured keys", h(Quiz, { questions: QUESTIONS, right: SOUNDS.right, wrong: SOUNDS.wrong })),
    },
    {
      page: POLL,
      title: "Live poll",
      perViewer: true,
      root: () => frame(LOOK.input, "Live poll", prompts(["Index", INDEX], ["Search", SEARCH], ["Quiz", QUIZ], ["Buttons", BUTTONS]), "Up Down to choose, Enter votes", h(Paragraph, { text: "The best teletext service was...", foreground: Colour.Yellow }), h(Gap, {}), h(Poll, { tally })),
    },
    { page: BUTTONS, title: "Buttons", root: () => h(SharedButtons, { articles }) },
    { page: RETITLE, title: "Retitle me", root: () => h(Retitled, {}) },

    {
      page: ROULETTE,
      title: "Roulette",
      section: "Play",
      perViewer: true,
      root: () => frame(LOOK.play, "Roulette", prompts(["Index", INDEX], ["Sounds", SOUNDBOARD], ["Snake", SNAKE], ["Magazine", MAGAZINE]), "Enter spins", h(Roulette, { articles, tick: SOUNDS.tick, done: SOUNDS.done })),
    },
    {
      page: SOUNDBOARD,
      title: "Sound board",
      perViewer: true,
      root: () => frame(LOOK.play, "Sound board", prompts(["Index", INDEX], ["Roulette", ROULETTE], ["Snake", SNAKE], ["Buttons", BUTTONS]), "Up Down or click, Enter plays", h(SoundList, { sounds: Object.values(SOUNDS), shown: 16 })),
    },
    {
      page: SNAKE,
      title: "Snake",
      perViewer: true,
      root: () => frame(LOOK.play, "Snake", prompts(["Index", INDEX], ["Roulette", ROULETTE], ["Sounds", SOUNDBOARD], ["Animation", ANIMATION]), "The arrows turn; Enter plays again", h(Snake, { everyMs: 150, rows: 18, eat: SOUNDS.eat, crash: SOUNDS.crash })),
    },

    { page: MAGAZINE, title: "The magazine", section: "A magazine", root: () => h(MagazineFront, { articles }) },
    {
      page: EVERY_ARTICLE,
      title: "Every article",
      perViewer: true,
      root: () =>
        frame(
          LOOK.articles,
          "Every article",
          prompts(["Index", INDEX], ["Magazine", MAGAZINE], ["By day", DAYS], ["Search", SEARCH]),
          "Up Down or click; Enter reads",
          h(EveryArticle, { articles }),
        ),
    },
    ...articles.map(
      (article, index): PageSpec => ({
        page: article.page,
        title: article.title,
        root: () =>
          h(StoryPages, {
            ...LOOK.articles,
            prompts: prompts(["Magazine", MAGAZINE], ["Every one", EVERY_ARTICLE], ["Next", articles[(index + 1) % articles.length]?.page ?? MAGAZINE], ["Index", INDEX]),
            index: MAGAZINE,
            article,
            where: whereOf(article),
            ...(pictureOf(index) === undefined ? {} : { picture: pictureOf(index) as string }),
            ...(assets.qr.get(article.link) === undefined ? {} : { qr: assets.qr.get(article.link) as string }),
          }),
      }),
    ),
  ];
  return specs;
}

/** Every article as a list to choose from; Enter goes to its home. */
/** The cells of a component row's page number and date columns, each after its colour's cell. */
const PAGE_CELLS = 4;
const DATE_CELLS = 10;

/** Two lists of the same articles: rows of text, two lines each, and rows of components, the page, the date and the title in columns. Their rows are controls, which the browser lights under the pointer. */
class EveryArticle extends Component<{ readonly articles: readonly Article[] }> {
  #read(index: number, session: number): void {
    const article = this.props.articles[index];
    if (article !== undefined) {
      this.services.goto(session, article.page);
    }
  }

  override render(): Child {
    return [
      h(Text, { text: "Rows of text, two lines each:", foreground: Colour.Cyan }),
      h(ChoiceList, {
        items: this.props.articles.map((article) => `${article.page.toString(16)} ${article.title}`),
        shown: 8,
        itemRows: 2,
        onChoose: (index: number, session: number) => this.#read(index, session),
      }),
      h(Text, { text: "Rows of components: page, date, title:", foreground: Colour.Cyan }),
      h(ChoiceList<Article>, {
        items: this.props.articles,
        shown: 8,
        render: (article: Article, look: RowLook) =>
          h(
            Columns,
            { widths: [PAGE_CELLS + 1, DATE_CELLS + 1, null] },
            h(Text, { text: article.page.toString(16), foreground: look.band ? look.foreground : Colour.Yellow }),
            h(DateText, { value: article.date, pattern: DATE_FORMATS.short, foreground: look.band ? look.foreground : Colour.Green }),
            h(Text, { text: article.title, foreground: look.foreground }),
          ),
        onChoose: (index: number, session: number) => this.#read(index, session),
      }),
    ];
  }
}

