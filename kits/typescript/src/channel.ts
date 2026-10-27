// A channel's applications in one program: the plain state machines of
// crates/teletext-runtime/src/application.rs, hosted here rather than in
// the server. Each application serves some of the channel's pages and is
// called with what happened; it answers through a Context, which writes
// the protocol's lines. One program serves every viewer, so applications
// keep each session's state and forget it when the session closes
// (docs/applications.md section 4).
//
// Pages that do not use digits or the coloured keys themselves get page
// selection by them (navigation.ts), so every page can be left by number
// or by Fastext; every page is sent its clicks (INPUT.md section 6.3), and
// a click the page does not take follows the prompt or the page number it
// is on.

import { App, type Owner, type PageDeclaration, type ServerEvents, type ServerStats, type Streams, processStreams, serveLines } from "./app.ts";
import { type Input, clicked } from "./input.ts";
import { Navigation, type PageKeys, type PageView, clickedLink } from "./navigation.ts";
import { readTti } from "./read-tti.ts";
import { NO_LINKS, ROWS, type Screen, type Subpage, pageText } from "./screen.ts";
import type { Audience, SoundMember } from "./sound.ts";

/** One page an application serves: its number, its title, which keys it uses itself, and the section it starts, if any. */
export interface PageEntry extends PageKeys {
  readonly page: number;
  readonly title: string;
  /** The name of the section of `contents.txt` this page starts: its magazine's name (docs/applications.md section 3.1). */
  readonly section?: string;
}

/** A program behind some of a channel's pages. */
export interface Application {
  /** The pages it serves, for the channel's contents and to route input. */
  pages(): readonly PageEntry[];
  /** The channel started: publish the shared pages. */
  start(context: Context): void;
  /** A session began: a viewer connected. */
  joined?(session: number, context: Context): void;
  /** The session's viewer opened one of this application's pages. */
  opened?(session: number, page: number, context: Context): void;
  /** The session's viewer left one of this application's pages. */
  leftPage?(session: number, page: number, context: Context): void;
  /**
   * Input on one of this application's pages, open with the keys, that
   * navigation did not take. Answer true for a click the page took; any
   * other click follows the Fastext prompt or the page number it is on.
   */
  input?(session: number, page: number, input: Input, context: Context): boolean | void;
  /** The time asked for with {@link Context.wakeAt} has come. */
  wake?(context: Context): void;
  /** The session ended: forget it. */
  ended?(session: number, context: Context): void;
  /** The server's figures, every 10 seconds (docs/applications.md section 7), for a page to show. */
  stats?(stats: ServerStats, context: Context): void;
}

/** A page not yet published: nothing to click on, no links. */
const NO_VIEW: PageView = { rows: [], links: NO_LINKS };

/** What a viewer sees of `screen`, kept apart from it, since an application may draw on it again. */
function viewOf(screen: Screen): PageView {
  return { rows: Array.from({ length: ROWS }, (_, row) => [...(screen.row(row) ?? [])]), links: screen.fastextLinks() };
}

/** The channel's clock: milliseconds since the program started, never going back. */
export interface Clock {
  now(): number;
}

/** The 64 bits of xorshift64*'s state. */
const STATE_BITS = 64;
/** The multiplier of xorshift64* (Vigna 2016, "An experimental exploration of Marsaglia's xorshift generators"). */
const XORSHIFT_MULTIPLIER = 0x2545_f491_4f6c_dd1dn;
/** Its three shifts. */
const SHIFT_A = 12n;
const SHIFT_B = 25n;
const SHIFT_C = 27n;
/** The output is the product's top 32 bits. */
const OUTPUT_SHIFT = 32n;

/**
 * Pseudo-random numbers from a seed (xorshift64*, as the runtime's
 * `Context::random`): for games, never for secrets.
 */
export class Random {
  #state: bigint;

  /** Numbers from `seed`; xorshift needs a state other than zero. */
  constructor(seed: bigint) {
    this.#state = BigInt.asUintN(STATE_BITS, seed) | 1n;
  }

  /** The next number, 0 to 2^32 - 1. */
  next(): number {
    let state = this.#state;
    state ^= state >> SHIFT_A;
    state = BigInt.asUintN(STATE_BITS, state ^ (state << SHIFT_B));
    state ^= state >> SHIFT_C;
    this.#state = state;
    return Number(BigInt.asUintN(STATE_BITS, state * XORSHIFT_MULTIPLIER) >> OUTPUT_SHIFT);
  }

  /** A number below `bound`; 0 when `bound` is 0. */
  below(bound: number): number {
    return bound > 0 ? this.next() % bound : 0;
  }
}

/** What a context reaches of its channel. */
interface Host {
  readonly app: App;
  readonly random: Random;
  epochAtStart(): number;
  wakeAt(index: number, at: number): void;
  published(owner: Owner, page: number, view: PageView): void;
  released(session: number, page: number): boolean;
}

/** An application's view of the channel during one call. */
export class Context {
  readonly #host: Host;
  readonly #index: number;
  readonly #now: number;

  constructor(host: Host, index: number, now: number) {
    this.#host = host;
    this.#index = index;
    this.#now = now;
  }

  /** The time, in milliseconds since the program started. */
  now(): number {
    return this.#now;
  }

  /** The real time, in milliseconds since the Unix epoch (UTC), by the server's clock. */
  epochMs(): number {
    return this.#host.epochAtStart() + this.#now;
  }

  /** Publishes a page drawn on a screen, as its one sub-page, 0. */
  publish(owner: Owner, page: number, screen: Screen): void {
    this.publishSubpages(owner, page, [{ screen, subpage: 0 }]);
  }

  /** Publishes a page with its sub-pages, each numbered as given. */
  publishSubpages(owner: Owner, page: number, subpages: readonly Subpage[]): void {
    this.#host.app.publish(owner, page, subpages);
    const first = subpages[0]?.screen;
    this.#host.published(owner, page, first === undefined ? NO_VIEW : viewOf(first));
  }

  /** Publishes a page from TTI written elsewhere, such as a converted picture's file ({@link App.publishTti}). */
  publishTti(owner: Owner, page: number, text: string): void {
    this.#host.app.publishTti(owner, page, text);
    const first = readTti(text)[0];
    const links = first?.links ?? [];
    this.#host.published(owner, page, { rows: first?.rows ?? [], links: [links[0] ?? null, links[1] ?? null, links[2] ?? null, links[3] ?? null] });
  }

  /** Sends the session's viewer to `page` (INPUT.md section 7.1). */
  goto(session: number, page: number): void {
    this.#host.app.goto(session, page);
  }

  /** The title `contents.txt` lists one of the channel's pages under now, if it has the page. */
  titleOf(page: number): string | undefined {
    return this.#host.app.titleOf(page);
  }

  /** Gives `page` a new title in the channel's contents, for what it holds now (docs/applications.md section 3.5). */
  title(page: number, title: string): void {
    this.#host.app.title(page, title);
  }

  /**
   * Drops the session's own copy of `page`: from now on it sees the
   * channel's copy, as every viewer does, and is sent it at once
   * (docs/applications.md section 3.6). Nothing is sent if it has none.
   */
  release(session: number, page: number): void {
    if (this.#host.released(session, page)) {
      this.#host.app.release(session, page);
    }
  }

  /** Sends a sound (docs/applications.md section 3.4). */
  sound(to: Audience, member: SoundMember): void {
    this.#host.app.sound(to, member);
  }

  /** Asks for `wake` at `at`, in milliseconds; the earliest asked for wins. */
  wakeAt(at: number): void {
    this.#host.wakeAt(this.#index, at);
  }

  /** Asks for `wake` in `delay` milliseconds. */
  wakeIn(delay: number): void {
    this.wakeAt(this.#now + delay);
  }

  /** Reports `text` in the server's log. */
  log(text: string): void {
    this.#host.app.log(text);
  }

  /** A pseudo-random number, 0 to 2^32 - 1: for games, never for secrets. */
  random(): number {
    return this.#host.random.next();
  }

  /** A pseudo-random number below `bound` (0 when `bound` is 0). */
  below(bound: number): number {
    return this.#host.random.below(bound);
  }
}

/** How a channel is made. */
export interface ChannelOptions {
  readonly clock: Clock;
  readonly seed: bigint;
  /** Where a failing application is reported: standard error. */
  readonly errors: (text: string) => void;
}

/** The channel: its applications, the sessions it has seen, and each page's links as each viewer sees them. */
export class Channel implements ServerEvents {
  readonly #applications: readonly Application[];
  readonly #app: App;
  readonly #clock: Clock;
  readonly #errors: (text: string) => void;
  readonly #entries = new Map<number, PageEntry>();
  readonly #owners = new Map<number, number>();
  readonly #wakes: (number | null)[];
  readonly #sessions = new Set<number>();
  readonly #shared = new Map<number, PageView>();
  readonly #own = new Map<number, Map<number, PageView>>();
  readonly #navigation: Navigation;
  readonly #host: Host;
  #epochAtStart = 0;

  /** A channel of `applications`, writing through `app`; the first application to declare a page serves it. */
  constructor(applications: readonly Application[], app: App, options: ChannelOptions) {
    this.#applications = applications;
    this.#app = app;
    this.#clock = options.clock;
    this.#errors = options.errors;
    this.#wakes = applications.map(() => null);
    applications.forEach((application, index) => {
      for (const entry of application.pages()) {
        if (!this.#entries.has(entry.page)) {
          this.#entries.set(entry.page, entry);
          this.#owners.set(entry.page, index);
        }
      }
    });
    this.#navigation = new Navigation((page) => this.#entries.has(page));
    this.#host = {
      app,
      random: new Random(options.seed),
      epochAtStart: () => this.#epochAtStart,
      wakeAt: (index, at) => {
        const wake = this.#wakes[index];
        this.#wakes[index] = wake === null || wake === undefined ? at : Math.min(wake, at);
      },
      published: (owner, page, view) => {
        if (owner === "channel") {
          this.#shared.set(page, view);
        } else {
          const own = this.#own.get(owner) ?? new Map<number, PageView>();
          own.set(page, view);
          this.#own.set(owner, own);
        }
      },
      released: (session, page) => this.#own.get(session)?.delete(page) ?? false,
    };
    this.#epochAtStart = Date.now() - this.#clock.now();
  }

  /** The pages, in the order the applications declare them, for the first line. */
  declarations(): PageDeclaration[] {
    return [...this.#entries.values()].map(({ page, title, section }) => ({ page, title, ...(section === undefined ? {} : { section }) }));
  }

  /** Every page the applications serve. */
  entries(): readonly PageEntry[] {
    return [...this.#entries.values()];
  }

  /** The server said start: every application publishes its shared pages. */
  start(epochMs: number): void {
    this.#epochAtStart = epochMs - this.#clock.now();
    this.#applications.forEach((_, index) => this.#call(index, (application, context) => application.start(context)));
  }

  join(session: number): void {
    this.#know(session);
  }

  open(session: number, page: number): void {
    this.#know(session);
    this.#navigation.reset(session);
    this.#callOwner(page, (application, context) => application.opened?.(session, page, context));
  }

  leave(session: number, page: number): void {
    this.#know(session);
    this.#callOwner(page, (application, context) => application.leftPage?.(session, page, context));
  }

  input(session: number, page: number, input: Input): void {
    this.#know(session);
    const entry = this.#entries.get(page);
    if (entry === undefined) {
      this.#errors(`input on page ${pageText(page)}, which no application serves`);
      return;
    }
    const view = (): PageView => this.#own.get(session)?.get(page) ?? this.#shared.get(page) ?? NO_VIEW;
    const navigated = this.#navigation.take(session, input, entry, view().links);
    if (navigated.taken) {
      if (navigated.goto !== null) {
        this.#app.goto(session, navigated.goto);
      }
      return;
    }
    let taken = false;
    this.#callOwner(page, (application, context) => {
      taken = application.input?.(session, page, input, context) === true;
    });
    if (!taken && clicked(input) && input.kind === "mouse") {
      const link = clickedLink(view(), input.column, input.row);
      if (link !== null) {
        this.#app.goto(session, link);
      }
    }
  }

  close(session: number): void {
    if (!this.#sessions.delete(session)) {
      return;
    }
    this.#own.delete(session);
    this.#navigation.forget(session);
    this.#applications.forEach((_, index) => this.#call(index, (application, context) => application.ended?.(session, context)));
  }

  /** The server's figures came: every application may show them. */
  stats(stats: ServerStats): void {
    this.#applications.forEach((_, index) => this.#call(index, (application, context) => application.stats?.(stats, context)));
  }

  /** When the channel next needs {@link Channel.deadline}, by its clock. */
  nextWake(): number | null {
    const wakes = this.#wakes.filter((wake): wake is number => wake !== null);
    return wakes.length === 0 ? null : Math.min(...wakes);
  }

  /** The time has come for the applications whose wake is due now. */
  deadline(): void {
    const now = this.#clock.now();
    this.#wakes.forEach((wake, index) => {
      if (wake !== null && wake <= now) {
        this.#wakes[index] = null;
        this.#call(index, (application, context) => application.wake?.(context));
      }
    });
  }

  /**
   * A session the channel has not seen joins as it is first named: the
   * server says `join` once, so after this program is started again the
   * sessions it already had arrive without one.
   */
  #know(session: number): void {
    if (this.#sessions.has(session)) {
      return;
    }
    this.#sessions.add(session);
    this.#applications.forEach((_, index) => this.#call(index, (application, context) => application.joined?.(session, context)));
  }

  #callOwner(page: number, run: (application: Application, context: Context) => void): void {
    const index = this.#owners.get(page);
    if (index !== undefined) {
      this.#call(index, run);
    }
  }

  /** Runs one application with a context; a failure is reported, and the channel goes on serving. */
  #call(index: number, run: (application: Application, context: Context) => void): void {
    const application = this.#applications[index];
    if (application === undefined) {
      return;
    }
    try {
      run(application, new Context(this.#host, index, this.#clock.now()));
    } catch (error) {
      this.#errors(`application ${index} failed: ${error instanceof Error ? (error.stack ?? error.message) : String(error)}`);
    }
  }
}

/** How {@link runChannel} runs. */
export interface RunOptions {
  /** The games' seed; by default the time, since nothing here is a secret. */
  readonly seed?: bigint;
  readonly streams?: Streams;
}

/**
 * Runs `applications` as this process's channel: declares their pages,
 * serves the server's lines, wakes each application when it asked, and
 * resolves when the server closes standard input.
 */
export async function runChannel(applications: readonly Application[], options: RunOptions = {}): Promise<void> {
  const streams = options.streams ?? processStreams();
  const app = new App(streams.output);
  const started = performance.now();
  const clock: Clock = { now: () => Math.floor(performance.now() - started) };
  const channel = new Channel(applications, app, { clock, seed: options.seed ?? process.hrtime.bigint(), errors: streams.errors });
  app.declare(channel.declarations());
  let timer: NodeJS.Timeout | undefined;
  const schedule = (): void => {
    clearTimeout(timer);
    const at = channel.nextWake();
    timer =
      at === null
        ? undefined
        : setTimeout(() => {
            channel.deadline();
            schedule();
          }, Math.max(0, at - clock.now()));
  };
  // Any message may ask for a wake, so the timer is set again after each.
  const then = (act: () => void): void => {
    act();
    schedule();
  };
  const events: ServerEvents = {
    start: (epochMs) => then(() => channel.start(epochMs)),
    join: (session) => then(() => channel.join(session)),
    open: (session, page) => then(() => channel.open(session, page)),
    leave: (session, page) => then(() => channel.leave(session, page)),
    input: (session, page, input) => then(() => channel.input(session, page, input)),
    close: (session) => then(() => channel.close(session)),
    stats: (stats) => then(() => channel.stats(stats)),
  };
  await serveLines(events, streams);
  clearTimeout(timer);
}
