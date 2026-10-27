// Magazine 4's demonstrations of the library's progress bars and gauges,
// spinners, column charts and sparklines, line charts and scatter plots,
// heat maps, calendars and dates (404 to 40A), each in its variants. The moving
// ones run only while someone watches (core/component.ts, whileViewed).

import { Colour } from "../kit.ts";
import {
  type Article,
  Calendar,
  type CalendarDate,
  type Child,
  ColumnChart,
  Columns,
  Component,
  DATE_FORMATS,
  DateText,
  GAUGE,
  Gap,
  h,
  Heatmap,
  LineChart,
  LineGauge,
  type PageSpec,
  Paragraph,
  ProgressBar,
  Rule,
  ScatterPlot,
  Sparkline,
  Spinner,
  Text,
} from "../../../../kits/typescript-components/src/index.ts";
import { frame, INDEX, LOOK, prompts } from "../look.ts";

export const PROGRESS = 0x404;
export const SPINNERS = 0x405;
export const COLUMNS_CHART = 0x406;
export const LINES_CHART = 0x407;
export const HEAT = 0x408;
export const CALENDAR = 0x409;
export const DATES = 0x40a;
/** The name column of the dates page. */
const NAME_CELLS = 10;
const SECOND_MS = 1_000;

/** Every preset of the one date formatter on the time now, then dates in other forms, each beside its name. */
class DatesDemo extends Component<object, { readonly now: number }> {
  override state = { now: 0 };

  override componentDidMount(): void {
    const tick = (): void => this.setState({ now: this.services.epochMs() });
    tick();
    this.setInterval(tick, SECOND_MS, { whileViewed: true });
  }

  override render(): Child {
    const row = (name: string, date: Child): Child => h(Columns, { widths: [NAME_CELLS, null] }, h(Text, { text: name, foreground: Colour.Cyan }), date);
    return [
      h(Paragraph, { text: "The presets, UTC, now:", foreground: Colour.Yellow }),
      ...Object.entries(DATE_FORMATS).map(([name, pattern]) => row(name, h(DateText, { value: this.state.now, pattern }))),
      h(Paragraph, { text: "Other forms, Moment's tokens:", foreground: Colour.Yellow }),
      row("ISO day", h(DateText, { value: "2021-07-16", foreground: Colour.Green })),
      row("No zone", h(DateText, { value: "2026-10-27 23:30:00", pattern: DATE_FORMATS.stamp, foreground: Colour.Green })),
      row("Own", h(DateText, { value: this.state.now, pattern: "dddd D MMMM [at] H:mm", foreground: Colour.Magenta })),
      row("Not dated", h(DateText, { value: "soon", fallback: "no date yet", foreground: Colour.Red })),
      h(DateText, { value: this.state.now, pattern: DATE_FORMATS.weekday, centre: true, foreground: Colour.White }),
    ];
  }
}

/** A share that rises and wraps, a step a second, while someone watches. */
class Rising extends Component<{ readonly render: (step: number) => Child; readonly everyMs?: number }, { readonly step: number }> {
  override state = { step: 0 };

  override componentDidMount(): void {
    this.setInterval(() => this.setState((state) => ({ step: state.step + 1 })), this.props.everyMs ?? 1_000, { whileViewed: true });
  }

  override render(): Child {
    return this.props.render(this.state.step);
  }
}

/** A made-up series: a slow wave with a little rise, `count` long. */
function wave(count: number, phase = 0): number[] {
  return Array.from({ length: count }, (_, index) => Math.round(50 + 30 * Math.sin((index + phase) / 4) + index / 2));
}

class Spinners extends Component<object, { readonly done: boolean }> {
  override state = { done: false };

  override componentDidMount(): void {
    this.setInterval(() => this.setState((state) => ({ done: !state.done })), 6_000, { whileViewed: true });
  }

  override render(): Child {
    return [
      h(Spinner, { label: "Orbit: a sixel round a cell", style: "orbit" }),
      h(Spinner, { label: "Fill: a cell filling and emptying", style: "fill", foreground: Colour.Yellow }),
      h(Spinner, { label: "Bounce: a block to and fro", style: "bounce", foreground: Colour.Green }),
      h(Spinner, { label: "Dots after the words", style: "dots" }),
      h(Spinner, { label: "Slow orbit, magenta", style: "orbit", everyMs: 1_000, foreground: Colour.Magenta }),
      h(Gap, {}),
      h(Paragraph, { text: "Done turns it into OK, and its timer stops:", foreground: Colour.Cyan }),
      h(Spinner, { label: "Fetching the feed", style: "bounce", done: this.state.done }),
    ];
  }
}

class CalendarDemo extends Component<{ readonly marked: readonly CalendarDate[] }, { readonly chosen: string; readonly now: number }> {
  override state = { chosen: "nothing yet", now: 0 };

  override componentDidMount(): void {
    this.setState({ now: this.services.epochMs() });
  }

  override render(): Child {
    if (this.state.now === 0) {
      return null;
    }
    const today = new Date(this.state.now);
    return [
      h(
        Columns,
        { widths: [29, null], height: 8 },
        h(Calendar, {
          year: today.getUTCFullYear(),
          month: today.getUTCMonth(),
          today: this.state.now,
          marked: this.props.marked,
          selectable: true,
          onChoose: (date: CalendarDate) => this.setState({ chosen: `${date.year}-${String(date.month + 1).padStart(2, "0")}-${String(date.day).padStart(2, "0")}` }),
        }),
        h(Paragraph, { text: "Arrows: a day, a week. PgUp PgDn: a month. Enter chooses. Yellow today, green an article, cyan the weekend.", foreground: Colour.White }),
      ),
      h(Paragraph, { text: `Chosen: ${this.state.chosen}`, foreground: Colour.Yellow }),
      h(Rule, { title: "a month to read, not choose", foreground: Colour.Blue }),
      h(Calendar, { year: 2027, month: 0, marked: [{ year: 2027, month: 0, day: 1 }] }),
    ];
  }
}

export function figurePages(articles: readonly Article[]): PageSpec[] {
  const next = (...links: (readonly [string, number])[]): ReturnType<typeof prompts> => prompts(["Index", INDEX], ...links);
  const writers = [...new Set(articles.map((article) => article.writer))];
  const marked = articles.map((article) => {
    const date = new Date(article.date);
    return { year: date.getUTCFullYear(), month: date.getUTCMonth(), day: date.getUTCDate() };
  });
  return [
    {
      page: PROGRESS,
      title: "Progress and gauges",
      root: () =>
        frame(
          LOOK.figures,
          "Progress and gauges",
          next(["Spinners", SPINNERS], ["Columns", COLUMNS_CHART], ["Lines", LINES_CHART]),
          "They move once a second while you watch",
          h(Rising, {
            render: (step: number) => {
              const share = (offset: number): number => (step * 7 + offset) % 101;
              return [
                h(ProgressBar, { label: "Download", value: share(0), max: 100 }),
                h(ProgressBar, { label: "Build", value: share(40), max: 100, foreground: Colour.Cyan }),
                h(ProgressBar, { label: "Segments", value: share(70), max: 100, separated: true, foreground: Colour.Yellow }),
                h(ProgressBar, { label: "No figure", value: share(20), max: 100, percent: false, foreground: Colour.Magenta }),
                h(ProgressBar, { value: share(55), max: 100, foreground: Colour.White }),
                h(Gap, {}),
                h(Paragraph, { text: "Gauges: green, yellow past 60%, red past 85%", foreground: Colour.Cyan }),
                h(ProgressBar, { label: "CPU", value: share(10), max: 100, thresholds: GAUGE }),
                h(ProgressBar, { label: "Disk", value: share(60), max: 100, thresholds: GAUGE }),
                h(ProgressBar, { label: "Load", value: 100, max: 100, thresholds: GAUGE }),
                h(ProgressBar, { label: "Idle", value: 0, max: 100, thresholds: GAUGE }),
                h(Gap, {}),
                h(Paragraph, { text: "Line gauges: Hold Mosaics, no gap", foreground: Colour.Cyan }),
                h(LineGauge, { label: "Progress", value: share(30), max: 100 }),
                h(LineGauge, { label: "Volume", value: share(80), max: 100, foreground: Colour.Green, rest: Colour.Red }),
                h(LineGauge, { label: "Full", value: 1, max: 1, foreground: Colour.Yellow }),
              ];
            },
          }),
        ),
    },
    { page: SPINNERS, title: "Spinners", root: () => frame(LOOK.figures, "Spinners", next(["Progress", PROGRESS], ["Columns", COLUMNS_CHART], ["Lines", LINES_CHART]), "Four frames a second while you watch", h(Spinners, {})) },
    {
      page: COLUMNS_CHART,
      title: "Columns and sparklines",
      root: () =>
        frame(
          LOOK.figures,
          "Columns, sparklines",
          next(["Lines", LINES_CHART], ["Heat map", HEAT], ["Progress", PROGRESS]),
          undefined,
          h(ColumnChart, {
            bars: writers.map((writer, index) => ({ label: writer.slice(0, 2), value: articles.filter((article) => article.writer === writer).length, foreground: [Colour.Red, Colour.Green, Colour.Yellow, Colour.Cyan, Colour.Magenta, Colour.White][index % 6] ?? Colour.Green })),
            plotRows: 4,
            figures: true,
          }),
          h(ColumnChart, { bars: articles.slice(0, 12).map((article) => ({ label: String(article.page % 100), value: article.blocks.length })), plotRows: 3, columnCells: 1 }),
          h(Gap, {}),
          h(Sparkline, { label: "Wave", values: wave(60) }),
          h(Sparkline, { label: "Tall", values: wave(60, 9), plotRows: 2, foreground: Colour.Yellow }),
          h(Sparkline, { label: "Rising", values: Array.from({ length: 60 }, (_, index) => index), foreground: Colour.Cyan }),
          h(Sparkline, { label: "Flat", values: Array.from({ length: 60 }, () => 5), foreground: Colour.Magenta }),
          h(Sparkline, { label: "None", values: [], empty: "no readings yet" }),
        ),
    },
    {
      page: LINES_CHART,
      title: "Line charts and scatter",
      root: () =>
        frame(
          LOOK.figures,
          "Line charts, scatter",
          next(["Heat map", HEAT], ["Calendar", CALENDAR], ["Columns", COLUMNS_CHART]),
          undefined,
          h(LineChart, { values: wave(40), plotRows: 5, labels: ["Mon", "Tue", "Wed", "Thu"] }),
          h(LineChart, { values: wave(40, 12), plotRows: 4, area: true, foreground: Colour.Cyan }),
          h(ScatterPlot, { points: articles.map((article) => [article.blocks.length, articles.indexOf(article)] as const), plotRows: 5, labels: ["fewer blocks", "more"] }),
          h(LineChart, { values: [], plotRows: 1, empty: "no series: the empty text" }),
        ),
    },
    {
      page: HEAT,
      title: "Heat maps",
      root: () =>
        frame(
          LOOK.figures,
          "Heat maps",
          next(["Calendar", CALENDAR], ["Lines", LINES_CHART], ["Progress", PROGRESS]),
          undefined,
          h(Paragraph, { text: "By colour, two cells a value, no gaps:", foreground: Colour.Cyan }),
          h(Heatmap, { rows: ["Mon", "Tue", "Wed", "Thu", "Fri"].map((label, row) => ({ label, values: wave(16, row * 3) })), legend: true }),
          h(Gap, {}),
          h(Paragraph, { text: "By density, a cell a value, readable in one colour:", foreground: Colour.Cyan }),
          h(Heatmap, { rows: ["Sat", "Sun", "Hol"].map((label, row) => ({ label, values: wave(32, row * 5) })), scale: "density", legend: true }),
          h(Heatmap, { rows: ["Green"].map((label) => ({ label, values: wave(32, 2) })), scale: "density", foreground: Colour.Green }),
        ),
    },
    { page: CALENDAR, title: "Calendars", root: () => frame(LOOK.figures, "Calendars", next(["Heat map", HEAT], ["Dates", DATES], ["Clock", 0x402]), undefined, h(CalendarDemo, { marked })) },
    { page: DATES, title: "Dates", root: () => frame(LOOK.figures, "Dates", next(["Calendar", CALENDAR], ["Clock", 0x402], ["Progress", PROGRESS]), "One formatter draws every date", h(DatesDemo, {})) },
  ];
}
