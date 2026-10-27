// Driving a channel as the server drives it, for tests: sessions that
// join, open pages, type, press keys, click and turn the wheel, a clock the
// test moves, and what the program wrote, read back from its lines as the
// server would read them: each page's TTI decoded into its rows, links,
// cursor. It mirrors crates/teletext-showcase/src/testing.rs.

import { App, type ServerStats } from "./app.ts";
import { type Application, Channel, type Clock } from "./channel.ts";
import { Button, type HoverAction, type Input, type KeyAction, isCharacterKey } from "./input.ts";
import { type ReadPage, type Rows, readTti } from "./read-tti.ts";
import type { Cursor } from "./screen.ts";

export { type ReadPage, type Rows, readTti, rowOf, shows, text } from "./read-tti.ts";

/** A clock the test sets. */
export class ManualClock implements Clock {
  time = 0;

  now(): number {
    return this.time;
  }
}

/** A line the program wrote, parsed. */
export type Sent = Readonly<Record<string, unknown>>;

/** How a harness is made. */
export interface HarnessOptions {
  /** The real time when the channel starts, in milliseconds since the Unix epoch. */
  readonly epochMs?: number;
  readonly seed?: bigint;
}

const HEXADECIMAL = 16;

/** The seed of the runtime's own test harness. */
const TEST_SEED = 7n;

function pageOf(value: unknown): number | undefined {
  return typeof value === "string" ? Number.parseInt(value, HEXADECIMAL) : undefined;
}

/** A channel of some applications, started, and everything it wrote. */
export class Harness {
  readonly clock = new ManualClock();
  readonly channel: Channel;
  /** Every line written, parsed, the first being the pages. */
  readonly sent: Sent[] = [];
  /** What the channel reported on standard error. */
  readonly errors: string[] = [];
  readonly #open = new Map<number, number>();
  readonly #went = new Map<number, number>();
  /** Where each session's lines begin: what came before it joined was another session's of the same number. */
  readonly #since = new Map<number, number>();
  #counted = 0;

  constructor(applications: readonly Application[], options: HarnessOptions = {}) {
    const app = new App((line) => this.#record(line));
    this.channel = new Channel(applications, app, { clock: this.clock, seed: options.seed ?? TEST_SEED, errors: (error) => this.errors.push(error) });
    app.declare(this.channel.declarations());
    this.channel.start(options.epochMs ?? 0);
  }

  #record(line: string): void {
    const message = JSON.parse(line) as Sent;
    this.sent.push(message);
    if (message["type"] === "goto" && typeof message["session"] === "number") {
      const page = pageOf(message["page"]);
      if (page !== undefined) {
        this.#went.set(message["session"], page);
      }
    }
  }

  /** The time by the channel's clock. */
  get now(): number {
    return this.clock.time;
  }

  /** A session that joined and opened `page`. */
  join(session: number, page: number): void {
    this.#since.set(session, this.sent.length);
    this.channel.join(session);
    this.open(session, page);
  }

  /** The session opens `page`, leaving the page it had open, as the server says. */
  open(session: number, page: number): void {
    const before = this.#open.get(session);
    if (before !== undefined && before !== page) {
      this.channel.leave(session, before);
    }
    this.#open.set(session, page);
    this.#went.delete(session);
    this.channel.open(session, page);
  }

  /** The session ends. */
  close(session: number): void {
    this.#open.delete(session);
    this.#went.delete(session);
    this.channel.close(session);
  }

  /** Input from `session` on the page it has open. */
  send(session: number, input: Input): void {
    const page = this.#open.get(session);
    if (page === undefined) {
      throw new Error(`session ${session} has no page open`);
    }
    this.channel.input(session, page, input);
  }

  /** Raw text, as an input method or a phone commits it (INPUT.md section 5.3). */
  typeText(session: number, typed: string): void {
    this.send(session, { kind: "text", text: typed });
  }

  /** Types `typed` on the keyboard, as the browser sends it: each character a key's press with its text, then its release (INPUT.md section 6.3). */
  typeKeys(session: number, typed: string): void {
    for (const typedCharacter of typed) {
      this.send(session, { kind: "key", action: "press", key: typedCharacter, text: typedCharacter });
      this.send(session, { kind: "key", action: "release", key: typedCharacter });
    }
  }

  paste(session: number, pasted: string): void {
    this.send(session, { kind: "paste", text: pasted });
  }

  /** One key event by its INPUT.md name; a character key's press or repeat carries the character as its text, as a key pressed alone does. */
  key(session: number, key: string, action: KeyAction): void {
    this.send(session, { kind: "key", action, key, ...(isCharacterKey(key) && action !== "release" ? { text: key } : {}) });
  }

  /** Presses and releases a key. */
  press(session: number, key: string): void {
    this.key(session, key, "press");
    this.key(session, key, "release");
  }

  /** Presses and releases a mouse button on a cell. */
  click(session: number, button: number, column: number, row: number): void {
    this.mouse(session, "press", button, column, row);
    this.mouse(session, "release", button, column, row);
  }

  /** Presses or releases a mouse button on a cell. */
  mouse(session: number, action: "press" | "release", button: number, column: number, row: number): void {
    this.send(session, { kind: "mouse", action, button, column, row });
  }

  /** Moves the pointer onto a cell with `buttons` held ({@link Held}). */
  move(session: number, buttons: number, column: number, row: number): void {
    this.send(session, { kind: "mouse", action: "move", buttons, column, row });
  }

  /** The pointer enters or leaves the control whose first cell is `column`, `row` (INPUT.md section 5.4, HOVER). */
  hover(session: number, action: HoverAction, column: number, row: number): void {
    this.send(session, { kind: "hover", action, column, row });
  }

  /** A left click. */
  clickLeft(session: number, column: number, row: number): void {
    this.click(session, Button.Left, column, row);
  }

  /** The server's figures arrive, as they do every 10 seconds. */
  stats(stats: ServerStats): void {
    this.channel.stats(stats);
  }

  /** Turns the wheel over a cell: `dy` in 1/120ths of a line, down positive. */
  wheel(session: number, dy: number, column: number, row: number): void {
    this.send(session, { kind: "wheel", dx: 0, dy, column, row });
  }

  /** Runs the clock on by `by`, waking the applications once if a wake is due, as the runtime's harness does. */
  advance(by: number): void {
    this.clock.time += by;
    const wake = this.channel.nextWake();
    if (wake !== null && wake <= this.clock.time) {
      this.channel.deadline();
    }
  }

  /** Runs the clock on to `until`, waking the applications at each time they asked for. */
  runUntil(until: number): void {
    for (let wake = this.channel.nextWake(); wake !== null && wake <= until; wake = this.channel.nextWake()) {
      this.clock.time = Math.max(this.clock.time, wake);
      this.channel.deadline();
    }
    this.clock.time = Math.max(this.clock.time, until);
  }

  /** The publish lines, in order. */
  publications(): Sent[] {
    return this.sent.filter((message) => message["type"] === "publish");
  }

  /** How many times `page` was published shared since the last call. */
  sharedPublications(page: number): number {
    const count = this.sent.slice(this.#counted).filter((message) => isPublication(message, page, "channel")).length;
    this.#counted = this.sent.length;
    return count;
  }

  /** The latest shared copy of `page`, every sub-page. */
  sharedPages(page: number): ReadPage[] {
    const latest = this.sent.findLast((message) => isPublication(message, page, "channel"));
    if (latest === undefined) {
      throw new Error(`page ${page.toString(16)} was never published shared`);
    }
    return readTti(String(latest["tti"]));
  }

  /** The latest copy of `page` sent to `session` alone, every sub-page; undefined if none since it joined, or since it was released. */
  ownPages(session: number, page: number): ReadPage[] | undefined {
    const since = this.sent.slice(this.#since.get(session) ?? 0);
    const latest = since.findLastIndex((message) => isPublication(message, page, session) || isRelease(message, page, session));
    const message = since[latest];
    return message === undefined || message["type"] === "release" ? undefined : readTti(String(message["tti"]));
  }

  /** The title `contents.txt` lists `page` under now: its declaration's, or the last it was given. */
  title(page: number): string | undefined {
    for (const message of this.sent.toReversed()) {
      if (message["type"] === "title" && pageOf(message["page"]) === page) {
        return String(message["title"]);
      }
      if (message["type"] === "pages" && Array.isArray(message["pages"])) {
        const declared = (message["pages"] as Sent[]).find((entry) => pageOf(entry["page"]) === page);
        return declared === undefined ? undefined : String(declared["title"]);
      }
    }
    return undefined;
  }

  /** The shared copy's rows: its last sub-page, as a receiver ends up holding it. */
  shared(page: number): Rows {
    return last(this.sharedPages(page)).rows;
  }

  /** The rows of the latest copy of `page` sent to `session` alone. */
  own(session: number, page: number): Rows {
    const pages = this.ownPages(session, page);
    if (pages === undefined) {
      throw new Error(`page ${page.toString(16)} was never sent to session ${session} alone`);
    }
    return last(pages).rows;
  }

  /** The copy of `page` `session` sees: its own if it has one, else the channel's. */
  seen(session: number, page: number): ReadPage {
    return last(this.ownPages(session, page) ?? this.sharedPages(page));
  }

  /** The page the session is at: where it was last sent, or the page it opened. */
  at(session: number): number | undefined {
    return this.#went.get(session) ?? this.#open.get(session);
  }

  /** The cursor of the page the session is at, as it sees the page. */
  cursor(session: number): Cursor | null {
    const page = this.at(session);
    return page === undefined ? null : this.seen(session, page).cursor;
  }

  /** The pages `session` was sent to, in order. */
  gotos(session: number): number[] {
    return this.sent
      .filter((message) => message["type"] === "goto" && message["session"] === session)
      .map((message) => pageOf(message["page"]) ?? 0);
  }

  /** The sound lines for `session`, in order. */
  sounds(session: number): Sent[] {
    return this.sent.filter((message) => {
      const to = message["to"];
      return message["type"] === "sound" && typeof to === "object" && to !== null && (to as Sent)["session"] === session;
    });
  }
}

function isPublication(message: Sent, page: number, owner: "channel" | number): boolean {
  return message["type"] === "publish" && pageOf(message["page"]) === page && message["owner"] === owner;
}

function isRelease(message: Sent, page: number, session: number): boolean {
  return message["type"] === "release" && pageOf(message["page"]) === page && message["session"] === session;
}

function last(pages: readonly ReadPage[]): ReadPage {
  const page = pages[pages.length - 1];
  if (page === undefined) {
    throw new Error("a publication with no sub-page");
  }
  return page;
}
