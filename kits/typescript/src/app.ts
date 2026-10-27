// A channel's program as the server runs it (docs/applications.md): one
// JSON object a line on standard input and output, the pages declared in
// the first line, then the server's `start`, `join`, `open`, `leave`,
// `input`, `close` and `stats`, and the program's `publish`, `goto`,
// `sound`, `title`, `release` and `log`. Standard output carries protocol lines and nothing else; anything
// for a person goes to standard error, which the server logs.
//
// Everything the server sends is checked whole before it is acted on, and
// a line that is not valid is reported on standard error and dropped: the
// program never stops for a bad line.

import { CONTROL_CHARACTER, type Input, readInput } from "./input.ts";
import { MOST_SUBPAGES, Screen, type Subpage, isPageNumber, pageText, tti } from "./screen.ts";
import { type Audience, type SoundMember, soundFields } from "./sound.ts";

/** The protocol's version, in `start` (docs/applications.md). */
export const PROTOCOL = 1;
/** The longest line either side may send (docs/applications.md section 1): 16 MiB. */
export const LONGEST_LINE = 16 * 1024 * 1024;
/** The most pages a program may declare (docs/applications.md section 3.1). */
export const MOST_PAGES = 800;
/** The longest page title (section 3.1, the engine's `contents.txt` rule). */
export const LONGEST_TITLE = 32;
/** The highest sub-page number: TTI's PN line writes it in two decimal digits. */
const LAST_SUBPAGE = 99;
/** A line's end. */
const NEWLINE = 0x0a;
/** The return a line may end with before its newline. */
const RETURN = "\r";
/** Seven-bit text: what a TTI text may hold. */
const ASCII = /^[\u0000-\u007f]*$/u;
/** The line that starts each sub-page of a TTI text. */
const PAGE_NUMBER_LINE = "PN,";

/** Who a publication belongs to: the channel's one shared copy, or one session's own (SERVER.md section 7.3). */
export type Owner = "channel" | number;

/** A page the program serves, for the channel's contents. */
export interface PageDeclaration {
  readonly page: number;
  readonly title: string;
  /** The name of the section this page starts in `contents.txt`, which a receiver shows as its magazine's name (docs/applications.md section 3.1). */
  readonly section?: string;
}

/** Whether `text` is a title or a section's name the server takes: at most {@link LONGEST_TITLE} characters, none a control. */
function isTitle(text: string): boolean {
  return [...text].length <= LONGEST_TITLE && !CONTROL_CHARACTER.test(text);
}

/** One object of the server's figures: each name a whole number, or an object of them, such as `most_opened`. */
export type Figures = Readonly<Record<string, unknown>>;

/** The server's figures, sent every 10 seconds (docs/applications.md section 7): its own, and this channel's. */
export interface ServerStats {
  readonly server: Figures;
  readonly channel: Figures;
}

/** The figure `name` of `figures`: a whole number, or 0 where it is missing or is not one. */
export function figure(figures: Figures, name: string): number {
  const value = figures[name];
  return typeof value === "number" && Number.isSafeInteger(value) ? value : 0;
}

/**
 * The figures of the object `name` of `figures`, such as `most_opened`'s
 * pages or `viewers`' channels, the largest first and equal ones by name,
 * as the server ranks the pages: a parsed object puts names that look
 * like whole numbers ("110") first, so the order it was written in is lost.
 */
export function figuresIn(figures: Figures, name: string): [string, number][] {
  const value = figures[name];
  if (!isObject(value)) {
    return [];
  }
  return Object.entries(value)
    .filter((entry): entry is [string, number] => typeof entry[1] === "number" && Number.isSafeInteger(entry[1]))
    .sort(([oneName, one], [twoName, two]) => two - one || (oneName < twoName ? -1 : oneName > twoName ? 1 : 0));
}

function isObject(value: unknown): value is Figures {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** A message from the server, checked. */
export type ServerMessage =
  | { readonly type: "start"; readonly protocol: number; readonly epochMs: number }
  | { readonly type: "join"; readonly session: number }
  | { readonly type: "open"; readonly session: number; readonly page: number }
  | { readonly type: "leave"; readonly session: number; readonly page: number }
  | { readonly type: "input"; readonly session: number; readonly page: number; readonly input: Input }
  | { readonly type: "close"; readonly session: number }
  | { readonly type: "stats"; readonly stats: ServerStats };

/** What the program does with each of the server's messages. */
export interface ServerEvents {
  start(epochMs: number): void;
  join(session: number): void;
  open(session: number, page: number): void;
  leave(session: number, page: number): void;
  input(session: number, page: number, input: Input): void;
  close(session: number): void;
  stats(stats: ServerStats): void;
}

/** Whether `value` is a session as the protocol writes it: a whole number. */
export function isSession(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/** A page as the protocol writes it, "100" to "8FF", as a number; undefined for anything else. */
export function readPage(value: unknown): number | undefined {
  if (typeof value !== "string" || !/^[1-8][0-9A-F]{2}$/u.test(value)) {
    return undefined;
  }
  return Number.parseInt(value, 16);
}

/** One line from the server as a message, checked whole; or why it is refused. */
export function readMessage(line: string): ServerMessage | string {
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch (error) {
    return `not JSON: ${error instanceof Error ? error.message : String(error)}`;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return "not an object";
  }
  const fields = value as Readonly<Record<string, unknown>>;
  const type = fields["type"];
  if (type === "start") {
    const protocol = fields["protocol"];
    const epochMs = fields["epoch_ms"];
    if (typeof protocol !== "number" || !Number.isSafeInteger(epochMs) || typeof epochMs !== "number") {
      return "start: protocol and epoch_ms are whole numbers";
    }
    return { type, protocol, epochMs };
  }
  if (type === "stats") {
    const server = fields["server"];
    const channel = fields["channel"];
    return isObject(server) && isObject(channel) ? { type, stats: { server, channel } } : "stats: server and channel are objects";
  }
  if (type !== "join" && type !== "close" && type !== "open" && type !== "leave" && type !== "input") {
    return `unknown type ${JSON.stringify(type) ?? "undefined"}`;
  }
  const session = fields["session"];
  if (!isSession(session)) {
    return `${type}: a session is a whole number`;
  }
  if (type === "join" || type === "close") {
    return { type, session };
  }
  const page = readPage(fields["page"]);
  if (page === undefined) {
    return `${type}: a page is "100" to "8FF"`;
  }
  if (type === "open" || type === "leave") {
    return { type, session, page };
  }
  const input = readInput(fields["event"]);
  return typeof input === "string" ? input : { type, session, page, input };
}

/** Hands a checked message to `events`. */
export function dispatch(message: ServerMessage, events: ServerEvents): void {
  switch (message.type) {
    case "start":
      events.start(message.epochMs);
      return;
    case "join":
      events.join(message.session);
      return;
    case "open":
      events.open(message.session, message.page);
      return;
    case "leave":
      events.leave(message.session, message.page);
      return;
    case "input":
      events.input(message.session, message.page, message.input);
      return;
    case "close":
      events.close(message.session);
      return;
    case "stats":
      events.stats(message.stats);
      return;
  }
}

/** The program's side of the protocol: each call writes one line, checked first as the server will check it. */
export class App {
  readonly #write: (line: string) => void;
  /** Each declared page's title as `contents.txt` lists it now. */
  readonly #titles = new Map<number, string>();
  #declared = false;

  /** A program that writes its lines, each without its newline, to `write`. */
  constructor(write: (line: string) => void) {
    this.#write = write;
  }

  #send(message: Readonly<Record<string, unknown>>): void {
    this.#write(JSON.stringify(message));
  }

  /** Declares the program's pages: the first line, once. */
  declare(pages: readonly PageDeclaration[]): void {
    if (this.#declared) {
      throw new Error("pages are declared once, in the first line");
    }
    if (pages.length > MOST_PAGES) {
      throw new RangeError(`at most ${MOST_PAGES} pages`);
    }
    for (const { page, title, section } of pages) {
      if (!isPageNumber(page) || this.#titles.has(page)) {
        throw new RangeError(`page ${page} is not a page number, or is declared twice`);
      }
      if (!isTitle(title)) {
        throw new RangeError(`the title of ${pageText(page)} is over ${LONGEST_TITLE} characters or has controls`);
      }
      if (section !== undefined && !isTitle(section)) {
        throw new RangeError(`the section ${pageText(page)} starts is over ${LONGEST_TITLE} characters or has controls`);
      }
      this.#titles.set(page, title);
    }
    this.#declared = true;
    this.#send({
      type: "pages",
      pages: pages.map(({ page, title, section }) => ({ page: pageText(page), title, ...(section === undefined ? {} : { section }) })),
    });
  }

  /** Whether `page` was declared. */
  serves(page: number): boolean {
    return this.#titles.has(page);
  }

  /** The title `contents.txt` lists `page` under now, if it was declared. */
  titleOf(page: number): string | undefined {
    return this.#titles.get(page);
  }

  /** Gives `page` a new title in the channel's contents, for what it holds now; nothing is sent if it already has it (docs/applications.md section 3.5). */
  title(page: number, title: string): void {
    const now = this.#titles.get(page);
    if (now === undefined) {
      throw new RangeError(`title: ${pageText(page)} was not declared`);
    }
    if (!isTitle(title)) {
      throw new RangeError(`title: the title of ${pageText(page)} is over ${LONGEST_TITLE} characters or has controls`);
    }
    if (now !== title) {
      this.#titles.set(page, title);
      this.#send({ type: "title", page: pageText(page), title });
    }
  }

  /** Drops `session`'s own copy of `page`: from now on it sees the channel's (docs/applications.md section 3.6). */
  release(session: number, page: number): void {
    if (!isSession(session) || !this.#titles.has(page)) {
      throw new RangeError(`release: session ${session}, page ${page}, which was not declared`);
    }
    this.#send({ type: "release", session, page: pageText(page) });
  }

  /** Publishes `page`: one screen as sub-page 0, or sub-pages each numbered as given. */
  publish(owner: Owner, page: number, content: Screen | readonly Subpage[]): void {
    if (!this.#titles.has(page)) {
      throw new RangeError(`publish: ${pageText(page)} was not declared`);
    }
    if (owner !== "channel" && !isSession(owner)) {
      throw new RangeError(`publish: owner is "channel" or a session, not ${String(owner)}`);
    }
    const subpages = content instanceof Screen ? [{ screen: content, subpage: 0 }] : content;
    if (subpages.length === 0 || subpages.length > MOST_SUBPAGES) {
      throw new RangeError(`publish: 1 to ${MOST_SUBPAGES} sub-pages`);
    }
    if (subpages.some(({ subpage }) => !Number.isInteger(subpage) || subpage < 0 || subpage > LAST_SUBPAGE)) {
      throw new RangeError(`publish: a sub-page is 0 to ${LAST_SUBPAGE}`);
    }
    this.#send({ type: "publish", page: pageText(page), owner, tti: tti(page, subpages) });
  }

  /**
   * Publishes `page` from TTI written elsewhere, such as a converted
   * picture's file: every sub-page from its PN line, which the server
   * addresses to `page` whatever the line says. The text must be ASCII,
   * control codes in the escape form: the protocol carries it as a JSON
   * string, and a byte of the 8-bit form (0x80 to 0x9F) would arrive as
   * two bytes of UTF-8.
   */
  publishTti(owner: Owner, page: number, text: string): void {
    if (!this.#titles.has(page)) {
      throw new RangeError(`publish: ${pageText(page)} was not declared`);
    }
    if (owner !== "channel" && !isSession(owner)) {
      throw new RangeError(`publish: owner is "channel" or a session, not ${String(owner)}`);
    }
    if (!ASCII.test(text) || text.length > LONGEST_LINE) {
      throw new RangeError("publish: TTI is ASCII, control codes escaped, within the line's bound");
    }
    const subpages = text.split("\n").filter((line) => line.startsWith(PAGE_NUMBER_LINE)).length;
    if (subpages === 0 || subpages > MOST_SUBPAGES) {
      throw new RangeError(`publish: 1 to ${MOST_SUBPAGES} sub-pages, each from a PN line`);
    }
    this.#send({ type: "publish", page: pageText(page), owner, tti: text });
  }

  /** Sends the session's viewer to `page` (INPUT.md section 7.1, REQ-INT-113). */
  goto(session: number, page: number): void {
    if (!isSession(session) || !isPageNumber(page)) {
      throw new RangeError(`goto: session ${session} to page ${page}`);
    }
    this.#send({ type: "goto", session, page: pageText(page) });
  }

  /** A sound for `to`: an object, a live sound, live blocks, or a command (docs/applications.md section 3.4). */
  sound(to: Audience, member: SoundMember): void {
    const audience = "session" in to ? { session: to.session } : { page: pageText(to.page) };
    if ("session" in to ? !isSession(to.session) : !isPageNumber(to.page)) {
      throw new RangeError("sound: to is a session or a page");
    }
    this.#send({ type: "sound", to: audience, ...soundFields(member) });
  }

  /** A line for the server's log. */
  log(text: string): void {
    this.#send({ type: "log", text });
  }
}

/**
 * Splits bytes into lines of at most {@link LONGEST_LINE} bytes, each
 * without its newline (or a return before it), decoded as UTF-8. A longer
 * line, or one that is not UTF-8, is reported and dropped whole.
 */
export class LineSplitter {
  readonly #onLine: (line: string) => void;
  readonly #onFault: (why: string) => void;
  readonly #decoder = new TextDecoder("utf-8", { fatal: true });
  #pieces: Buffer[] = [];
  #length = 0;
  #skipping = false;

  constructor(onLine: (line: string) => void, onFault: (why: string) => void) {
    this.#onLine = onLine;
    this.#onFault = onFault;
  }

  /** Takes the next bytes. */
  push(chunk: Buffer): void {
    let rest = chunk;
    for (let end = rest.indexOf(NEWLINE); end >= 0; end = rest.indexOf(NEWLINE)) {
      this.#take(rest.subarray(0, end));
      this.#finish();
      rest = rest.subarray(end + 1);
    }
    this.#take(rest);
  }

  /** The input ended: a last line without its newline counts. */
  end(): void {
    if (this.#length > 0 || this.#skipping) {
      this.#finish();
    }
  }

  #take(bytes: Buffer): void {
    if (this.#skipping || bytes.length === 0) {
      return;
    }
    if (this.#length + bytes.length > LONGEST_LINE) {
      this.#skipping = true;
      this.#pieces = [];
      this.#length = 0;
      return;
    }
    this.#pieces.push(bytes);
    this.#length += bytes.length;
  }

  #finish(): void {
    if (this.#skipping) {
      this.#skipping = false;
      this.#onFault(`a line over ${LONGEST_LINE} bytes, dropped`);
      return;
    }
    const bytes = Buffer.concat(this.#pieces, this.#length);
    this.#pieces = [];
    this.#length = 0;
    let line: string;
    try {
      line = this.#decoder.decode(bytes);
    } catch {
      this.#onFault("a line that is not UTF-8, dropped");
      return;
    }
    if (line.endsWith(RETURN)) {
      line = line.slice(0, -RETURN.length);
    }
    if (line.length > 0) {
      this.#onLine(line);
    }
  }
}

/** Where a program reads and writes: the process's own streams, or a test's. */
export interface Streams {
  readonly input: NodeJS.ReadableStream;
  readonly output: (line: string) => void;
  readonly errors: (text: string) => void;
}

/** The process's standard input, output and error. */
export function processStreams(): Streams {
  return {
    input: process.stdin,
    output: (line) => process.stdout.write(`${line}\n`),
    errors: (text) => process.stderr.write(`${text}\n`),
  };
}

/**
 * The line loop: hands each valid line from `streams.input` to `events`,
 * reports each bad one on `streams.errors`, and resolves when the input
 * ends, which is the server going away.
 */
export function serveLines(events: ServerEvents, streams: Streams): Promise<void> {
  const splitter = new LineSplitter(
    (line) => {
      const message = readMessage(line);
      if (typeof message === "string") {
        streams.errors(`refused a line from the server: ${message}`);
        return;
      }
      if (message.type === "start" && message.protocol !== PROTOCOL) {
        streams.errors(`the server speaks protocol ${message.protocol}; this kit speaks ${PROTOCOL}`);
      }
      dispatch(message, events);
    },
    (why) => streams.errors(`refused a line from the server: ${why}`),
  );
  return new Promise((resolve) => {
    streams.input.on("data", (chunk: Buffer | string) => splitter.push(typeof chunk === "string" ? Buffer.from(chunk) : chunk));
    streams.input.on("end", () => {
      splitter.end();
      resolve();
    });
  });
}
